use std::io;
use std::process::exit;

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use sha1::{Sha1, Digest};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

//
// Source: https://websocket.org/guides/websocket-protocol/
//
async fn send_text_frame(stream: &mut TcpStream, text: &str) -> io::Result<()> {
    let mut frame = Vec::new();

    frame.push(0x81); // FIN + Text frame + MASK

    let len = text.len();
    if len <= 125 {
        frame.push(len as u8);
    } else if len < 65536 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    frame.extend_from_slice(text.as_bytes());
    stream.write_all(&frame).await?;
    Ok(())
}

async fn handle_client(stream: &mut TcpStream) -> io::Result<()> {
    let mut buf_bytes = [0; 1024];

    let buf_size = stream.read(&mut buf_bytes).await?;
    let buf = str::from_utf8(&buf_bytes[0..buf_size])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut accept_key = None;
    for line in buf.split("\r\n") {
        let mut it = line.split(": ");
        let key = it.next().unwrap_or_default();
        let value = it.next().unwrap_or_default();

        match key {
            "Connection" => {
                if value != "Upgrade" {
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                        "Invalid Websocket Handshake."));
                }
            },
            "Upgrade" => {
                if value != "websocket" {
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                        "Invalid Websocket Handshake."));
                }
            },
            "Sec-WebSocket-Version" => {
                if value != "13" {
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                        "Unsupported WebSocket version."));
                }
            },
            "Sec-WebSocket-Key" => {
                let combined = format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11",
                    value);

                let mut hasher = Sha1::new();
                hasher.update(combined.as_bytes());
                let hashed = hasher.finalize();

                let encoded = B64.encode(hashed);

                accept_key = Some(encoded);
            },
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
        send_text_frame(stream, "urmom").await?;
        loop {
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
                         msg.len(), msg);
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            });
        }
    }
}
