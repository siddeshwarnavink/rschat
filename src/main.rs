mod constants;
pub mod service;
pub mod ws;

use std::io;
use std::process::exit;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use bytes::BytesMut;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::constants::*;
use crate::service::message::{Message, MessageKind};
use crate::service::user::User;
use crate::ws::frame;

async fn handle_client_request(
    shared_stream: Arc<Mutex<TcpStream>>,
    mut buf: BytesMut,
    user_id: String,
) -> io::Result<()> {
    loop {
        let mut stream = shared_stream.lock().await;
        buf.clear();

        let mut len = stream.read_buf(&mut buf).await?;
        if len < 4 {
            User::leave(&user_id);
            break Ok(());
        }

        if let Ok(req) = frame::get_text(&buf[..len]) {
            if req.len() < 4 {
                println!("[error] invalid request from {user_id}");
                drop(stream);
                tokio::task::yield_now().await;
                continue;
            }

            let cmd = &req[..3];
            let payload = &req[4..];

            let response = format!("cmd = \"{cmd}\" payload = \"{payload}\"");

            buf.clear();
            len = frame::set_text(&mut buf, &response);

            stream.write_all(&buf[..len]).await?;
        }

        drop(stream);
        tokio::task::yield_now().await;
    }
}

async fn handle_new_client(
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

        let user_id = accept.to_string();
        User::join(&user_id, DEFAULT_NAME);
        Message::post(
            MessageKind::ServerMessage,
            &format!("{} joined the chat.", DEFAULT_NAME),
        );

        drop(response);
        drop(stream);

        handle_client_request(
            shared_stream.clone(),
            buf.into(),
            user_id.into(),
        )
        .await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let addr = "127.0.01:3333";
    let listener = TcpListener::bind(addr).await.unwrap_or_else(|_| {
        eprintln!("Error: Failed to listen to {}", addr);
        exit(1);
    });

    println!("Listening to ws://{}/", addr);

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let shared_stream = Arc::new(Mutex::new(stream));

            tokio::spawn(async move {
                if let Err(err) = handle_new_client(shared_stream.clone()).await
                {
                    let msg = err.to_string();
                    eprintln!("[error] {msg}");

                    let response = format!(
                        "HTTP/1.1 400 Bad Request\r\n\
                         Content-Type: text/plain\r\n\
                         Content-Length: {}\r\n\r\n{}",
                        msg.len(),
                        msg
                    );

                    let mut stream = shared_stream.lock().await;
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            });
        }
    }
}
