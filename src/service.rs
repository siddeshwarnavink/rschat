use std::io;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use bytes::BytesMut;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::yield_now;
use tokio::time::timeout;

use crate::constants::*;
use crate::ws::frame;
use crate::{MESSAGES, USERS};

pub enum MessageKind {
    ServerMessage,
    UserMessage,
}

pub struct Message {
    pub kind: MessageKind,
    pub text: String,
}

pub struct User {
    pub name: String,
    pub shared_stream: Arc<Mutex<TcpStream>>,
}

pub async fn client_request_handler(
    shared_stream: Arc<Mutex<TcpStream>>,
    mut buf: BytesMut,
    user_id: String,
) -> io::Result<()> {
    loop {
        let mut stream = shared_stream.lock().await;

        if let Err(_) =
            timeout(Duration::from_millis(10), stream.readable()).await
        {
            drop(stream);
            yield_now().await;
            continue;
        }

        buf.clear();

        let len = stream.read_buf(&mut buf).await?;
        drop(stream);

        if len < 1 {
            user_leave(&user_id).await;
            break Ok(());
        }

        if let Ok(req) = frame::get_text(&buf[..len]) {
            if req.len() < 4 {
                println!("[error] invalid request from {user_id}");
                yield_now().await;
                continue;
            }

            let cmd = &req[..3];
            let payload = &req[4..];

            match cmd {
                CMD_MESSAGE => {
                    post_user_message(user_id.as_str(), payload).await
                }
                CMD_RENAME => user_rename(user_id.as_str(), payload).await,
                _ => println!("[error] invalid command {cmd} from {user_id}"),
            }
        }

        yield_now().await;
    }
}

pub async fn new_client_handler(
    shared_stream: Arc<Mutex<TcpStream>>,
) -> io::Result<()> {
    let mut stream = shared_stream.lock().await;

    let mut buf = BytesMut::with_capacity(4096);
    buf.reserve(1024);

    let len = stream.read_buf(&mut buf).await?;
    let s = str::from_utf8(&buf[0..len])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut accept_key = None;
    for line in s.split("\r\n") {
        let mut it = line.split(": ");
        let key = it.next().unwrap_or_default();
        let value = it.next().unwrap_or_default();

        match key {
            "Upgrade" => {
                if value != "websocket" {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        ERR_WS_CONN,
                    ));
                }
            }
            "Sec-WebSocket-Version" => {
                if value != "13" {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        ERR_WS_VER,
                    ));
                }
            }
            "Sec-WebSocket-Key" => {
                let combined = format!("{}{}", value, WS_GUID);

                let mut hasher = Sha1::new();
                hasher.update(combined.as_bytes());
                let hashed = hasher.finalize();

                let encoded = B64.encode(hashed);

                accept_key = Some(encoded);
            }
            _ => {}
        }
    }

    if let Some(accept) = accept_key {
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\r\n",
            accept
        );
        stream.write_all(response.as_bytes()).await?;

        drop(response);
        drop(stream);

        let user_id = accept.to_string();
        user_join(&user_id, shared_stream.clone(), DEFAULT_NAME).await;

        client_request_handler(
            shared_stream.clone(),
            buf.into(),
            user_id.into(),
        )
        .await?;
    }

    Ok(())
}

async fn user_join(id: &str, shared_stream: Arc<Mutex<TcpStream>>, name: &str) {
    let mut users = USERS.lock().await;
    let new_user = User {
        name: name.into(),
        shared_stream: shared_stream.clone(),
    };

    users.insert(id.into(), new_user);
    println!("[info] New user {id} joined.");

    drop(users);

    post_server_message(&format!("{} joined the chat.", name)).await;
}

async fn user_leave(id: &str) {
    let mut users = USERS.lock().await;

    users.remove(id);
    println!("[info] User {id} left.");
}

async fn user_rename(id: &str, name: &str) {
    let mut users = USERS.lock().await;
    if let Some(user) = users.get_mut(id) {
        user.name = name.into();
        println!("[info] User {id} changed name to {name}.");
    }
}

async fn post_user_message(id: &str, text: &str) {
    let users = USERS.lock().await;
    let mut messages = MESSAGES.lock().await;
    if let Some(user) = users.get(id) {
        let new_msg = Message {
            kind: MessageKind::ServerMessage,
            text: format!("{}: {}", user.name, text),
        };

        messages.push(new_msg);
        println!("[info] User {id} posted new message.");

        drop(users);
        drop(messages);

        dispatch_messages().await;
    }
}

async fn post_server_message(text: &str) {
    let mut messages = MESSAGES.lock().await;
    let new_msg = Message {
        kind: MessageKind::ServerMessage,
        text: text.into(),
    };

    messages.push(new_msg);
    println!("[info] New message \"{text}\".");

    drop(messages);

    dispatch_messages().await;
}

async fn dispatch_messages() {
    let mut buf = BytesMut::with_capacity(4096);

    let mut messages = MESSAGES.lock().await;
    if messages.len() < 1 {
        return;
    }

    let users = USERS.lock().await;

    while let Some(message) = messages.pop() {
        for (_, user) in users.iter() {
            let mut stream = user.shared_stream.lock().await;
            let response = format!("{}", message.text);

            buf.clear();
            let len = frame::set_text(&mut buf, &response);

            let _ = stream.write_all(&buf[..len]).await;
        }
    }
}
