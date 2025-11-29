///
/// WebSocket protocol frame parser
///
/// Reference: <https://websocket.org/guides/websocket-protocol/>
///
pub mod ws_frame;
mod constants;

use std::io;
use std::process::exit;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::constants::*;

async fn handle_client(stream: &mut TcpStream) -> io::Result<()> {
    let mut buf = [0; 1024];

    let mut buf_size = stream.read(&mut buf).await?;
    let buf_str = str::from_utf8(&buf[0..buf_size])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut accept_key = None;
    for line in buf_str.split("\r\n") {
        let mut it = line.split(": ");
        let key = it.next().unwrap_or_default();
        let value = it.next().unwrap_or_default();

        match key {
            "Connection" => {
                if value != "Upgrade" {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        ERR_WS_CONN,
                    ));
                }
            }
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

        loop {
            buf_size = stream.read(&mut buf).await?;

            // TODO: Check if close frame to close the connection.

            if let Ok(msg) = ws_frame::get_text(&buf[..buf_size]) {
                buf_size = ws_frame::set_text(&mut buf, msg.as_str());
                stream.write_all(&buf[..buf_size]).await?;
            }
            tokio::task::yield_now().await;
        }
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
        if let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                if let Err(err) = handle_client(&mut stream).await {
                    let msg = err.to_string();
                    let response = format!(
                        "HTTP/1.1 400 Bad Request\r\n\
                         Content-Type: text/plain\r\n\
                         Content-Length: {}\r\n\r\n{}",
                        msg.len(),
                        msg
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            });
        }
    }
}
