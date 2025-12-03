use lazy_static::lazy_static;
use std::sync::Mutex;

pub mod user {
    use super::*;
    use std::collections::HashMap;

    pub struct User {
        pub name: String,
    }
    type UserCollection = HashMap<String, User>;

    lazy_static! {
        static ref USERS: Mutex<UserCollection> = Mutex::new(HashMap::new());
    }

    impl User {
        pub fn join(id: &str, name: &str) {
            let mut users = USERS.lock().unwrap();
            let new_user = Self { name: name.into() };

            users.insert(id.into(), new_user);
            println!("[info] New user {id} joined.");
        }

        pub fn leave(id: &str) {
            let mut users = USERS.lock().unwrap();
            users.remove(id);
            println!("[info] User {id} left.");
        }
    }
}

pub mod message {
    use super::*;

    pub enum MessageKind {
        ServerMessage,
        UserMessage,
    }

    pub struct Message {
        pub kind: MessageKind,
        pub text: String,
    }
    type MessageCollection = Vec<Message>;

    lazy_static! {
        static ref MESSAGES: Mutex<MessageCollection> = Mutex::new(Vec::new());
    }

    impl Message {
        pub fn post(kind: MessageKind, text: &str) {
            let mut messages = MESSAGES.lock().unwrap();
            let new_msg = Self {
                kind: kind,
                text: text.into(),
            };
            messages.push(new_msg);
            println!("[info] New message \"{text}\".");
        }
    }
}
