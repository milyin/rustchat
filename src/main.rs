#[macro_use]
extern crate rocket;

#[cfg(test)]
mod tests;

use std::convert::Infallible;

use rocket::form::Form;
use rocket::fs::FileServer;
use rocket::request::{self, FromRequest};
use rocket::response::stream::{Event, EventStream};
use rocket::serde::{Deserialize, Serialize};
use rocket::tokio::select;
use rocket::tokio::sync::broadcast::{channel, error::RecvError, Sender};
use rocket::{Request, Shutdown, State};

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

#[launch]
fn rocket() -> _ {
    rocket::build()
        .manage(channel::<Message>(1024).0)
        .mount("/", routes![post, events, user])
        .mount("/", FileServer::from("static"))
}
