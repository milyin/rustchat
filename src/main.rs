#[macro_use]
extern crate rocket;

#[cfg(test)]
mod tests;

use std::convert::Infallible;

use azure_data_cosmos::clients::{CosmosClient, CosmosOptions};
use azure_data_cosmos::prelude::AuthorizationToken;
use azure_data_cosmos::resources::permission::AuthorizationTokenParseError;
use azure_data_cosmos::ConsistencyLevel;
use rocket::form::Form;
use rocket::fs::FileServer;
use rocket::request::{self, FromRequest};
use rocket::response::stream::{Event, EventStream};
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::select;
use rocket::tokio::sync::broadcast::{channel, error::RecvError, Sender};
use rocket::{Request, Shutdown, State};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    AuthorizationTokenParse(AuthorizationTokenParseError),
}

impl From<AuthorizationTokenParseError> for Error {
    fn from(e: AuthorizationTokenParseError) -> Self {
        Error::AuthorizationTokenParse(e)
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
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => continue,
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

struct DbConnection {
    pub account: String,
    pub master_key: String,
    // cosmos_client: CosmosClient,
}

impl DbConnection {
    fn new(account: String, master_key: String) -> Result<Self> {
        // let auth_token = AuthorizationToken::primary_from_base64(&master_key)?;
        // let cosmos_client =
        //     CosmosClient::new(account.clone(), auth_token, CosmosOptions::default());
        Ok(Self {
            account,
            master_key,
            // cosmos_client,
        })
    }
}

#[get("/db")]
fn db(state: &State<DbConnection>) -> String {
    let master_key = &state.master_key;
    let account = &state.account;
    format!("{master_key} {account}")
}

#[launch]
fn rocket() -> _ {
    // let master_key = std::env::var("COSMOS_MASTER_KEY").expect("COSMOS_MASTER_KEY not set");
    // let account = std::env::var("COSMOS_ACCOUNT").expect("COSMOS_ACCOUNT not set");
    // let db_connection = DbConnection::new(account, master_key).expect("DbConnection:");
    let master_key =
        std::env::var("COSMOS_MASTER_KEY").unwrap_or("COSMOS_MASTER_KEY not set".into());
    let account = std::env::var("COSMOS_ACCOUNT").unwrap_or("COSMOS_ACCOUNT not set".into());
    let db_connection = DbConnection::new(account, master_key).expect("DbConnection:");
    rocket::build()
        .manage(db_connection)
        .manage(channel::<Message>(1024).0)
        .mount("/", routes![post, events, user, db])
        .mount("/", FileServer::from("static"))
}
