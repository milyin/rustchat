#[macro_use]
extern crate rocket;

#[cfg(test)]
mod tests;

use azure_data_cosmos::clients::{CosmosClient, CosmosOptions};
use azure_data_cosmos::prelude::AuthorizationToken;
use azure_data_cosmos::resources::permission::AuthorizationTokenParseError;
use azure_data_cosmos::ConsistencyLevel;
use futures::{FutureExt, StreamExt};
use rocket::form::Form;
use rocket::fs::FileServer;
use rocket::request::{self, FromRequest};
use rocket::response::stream::{Event, EventStream};
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::sync::broadcast::{channel, Sender};
use rocket::tokio::sync::mpsc::error::SendError;
use rocket::tokio::sync::{mpsc, oneshot};
use rocket::tokio::task::LocalSet;
use rocket::tokio::{self, select, task};
use rocket::{Request, Shutdown, State};
use std::convert::Infallible;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    AuthorizationTokenParse(AuthorizationTokenParseError),
    #[error("Failed to communicate with DB access thread")]
    DbThreadError,
}

impl From<AuthorizationTokenParseError> for Error {
    fn from(e: AuthorizationTokenParseError) -> Self {
        Error::AuthorizationTokenParse(e)
    }
}
impl From<SendError<DbTask>> for Error {
    fn from(e: SendError<DbTask>) -> Self {
        Error::DbThreadError
    }
}

impl From<oneshot::error::RecvError> for Error {
    fn from(e: oneshot::error::RecvError) -> Self {
        Error::DbThreadError
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq, UriDisplayQuery))]
#[serde(crate = "rocket::serde")]
struct Message {
    pub room: String,
    pub username: String,
    pub message: String,
}

#[derive(Debug, Clone, FromForm, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct MessageForm {
    #[field(validate = len(..30))]
    pub room: String,
    pub message: String,
}

struct User {
    pub username: Option<String>,
}

#[async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = Infallible;

    async fn from_request(request: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let username = request.headers().get_one("X-MS-CLIENT-PRINCIPAL-NAME");
        let username = username.map(|v| v.to_string());
        request::Outcome::Success(Self { username })
    }
}

/// Returns an infinite stream of server-sent events. Each event is a message
/// pulled from a broadcast queue sent by the `post` handler.
#[get("/events")]
async fn events(queue: &State<Sender<Message>>, mut end: Shutdown) -> EventStream![] {
    let mut rx = queue.subscribe();
    EventStream! {
        loop {
            let msg = select! {
                msg = rx.recv() => match msg {
                    Ok(msg) => msg,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                },
                _ = &mut end => break,
            };

            yield Event::json(&msg);
        }
    }
}

/// Receive a message from a form submission and broadcast it to any receivers.
#[post("/message", data = "<form>")]
fn post(form: Form<MessageForm>, user: User, queue: &State<Sender<Message>>) {
    // A send 'fails' if there are no active subscribers. That's okay.
    let form = form.into_inner();
    let message = Message {
        room: form.room,
        username: user.username.unwrap_or("guest".into()),
        message: form.message,
    };
    let _res = queue.send(message);
}

#[get("/user")]
fn user(user: User) -> String {
    user.username.unwrap_or("anonymous".into())
}

// #[get("/db")]
// fn db() -> String {
//     let master_key = std::env::var("COSMOS_MASTER_KEY").expect("COSMOS_MASTER_KEY not set");
//     let account = std::env::var("COSMOS_ACCOUNT").expect("COSMOS_ACCOUNT not set");
//     let auth_token =
//         AuthorizationToken::primary_from_base64(&master_key).expect("create auth token failed");
//     let client = CosmosClient::new(account.clone(), auth_token, CosmosOptions::default());
//     let q = client.create_database("rustchat").consistency_level(ConsistencyLevel::Eventual).
//     let database_client = client.into_database_client("rustchat");
//     let collection_client = database_client.into_collection_client("messages");

//     format!("{master_key} {account}")
// }

enum DbTask {
    GetTables(oneshot::Sender<Vec<String>>),
}

struct DbConnection {
    send: mpsc::UnboundedSender<DbTask>,
}

impl DbConnection {
    fn new(account: String, master_key: String) -> Result<Self> {
        let auth_token = AuthorizationToken::primary_from_base64(&master_key)?;
        let client = CosmosClient::new(account.clone(), auth_token, CosmosOptions::default());

        dbg!(&account);
        dbg!(&master_key);

        let (send, mut recv) = mpsc::unbounded_channel();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        std::thread::spawn(move || {
            let local = LocalSet::new();

            local.spawn_local(async move {
                dbg!("start");
                while let Some(task) = recv.recv().await {
                    let client = client.clone();
                    match task {
                        DbTask::GetTables(response) => {
                            tokio::task::spawn_local(async move {
                                let _ = response.send(Self::get_tables_impl(client).await);
                            });
                        }
                    }
                }
                // If the while loop returns, then all the LocalSpawner
                // objects have have been dropped.
            });

            // This will return once all senders are dropped and all
            // spawned tasks have returned.
            rt.block_on(local);
        });

        Ok(Self { send })
    }

    async fn get_tables_impl(client: CosmosClient) -> Vec<String> {
        dbg!("get_tables_impl");
        let mut list = Vec::new();
        let mut dbs = client.list_databases().into_stream();
        while let Some(Ok(db)) = dbs.next().await {
            let dbs = db
                .databases
                .iter()
                .map(|v| v.id.as_ref())
                .collect::<Vec<_>>()
                .join(",");
            list.push(dbs);
        }
        list
    }

    pub async fn get_tables(&self) -> Result<Vec<String>> {
        dbg!("get_tables");
        let (send, response) = oneshot::channel();
        let task = DbTask::GetTables(send);
        self.send.send(task)?;
        Ok(response.await?)
    }
}

#[get("/db")]
async fn db(state: &State<DbConnection>) -> String {
    let tables = state.get_tables().await.unwrap_or(Vec::new()); // TODO: fire error 500
    tables.join(",")
}

#[launch]
fn rocket() -> _ {
    let master_key = std::env::var("COSMOS_MASTER_KEY").expect("env var COSMOS_MASTER_KEY");
    let account = std::env::var("COSMOS_ACCOUNT").expect("env var COSMOS_ACCOUNT");

    dbg!(&account);
    dbg!(&master_key);

    let db_connection = DbConnection::new(account, master_key).expect("DbConnection::new");
    rocket::build()
        .manage(db_connection)
        .manage(channel::<Message>(1024).0)
        .mount("/", routes![post, events, user, db])
        .mount("/", FileServer::from("static"))
}
