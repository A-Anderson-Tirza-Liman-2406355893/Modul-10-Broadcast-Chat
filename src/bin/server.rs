use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::{Sender, channel};
use tokio_websockets::{Message, ServerBuilder, WebSocketStream};

async fn handle_connection(
    addr: SocketAddr,
    mut ws_stream: WebSocketStream<TcpStream>,
    bcast_tx: Sender<(SocketAddr, String)>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Subscribe ke broadcast channel untuk mendapatkan receiver
    let mut bcast_rx = bcast_tx.subscribe();

    loop {
        // Menggunakan tokio::select! untuk 2 task secara konkuren
        tokio::select! {
            // Task 1: Menerima pesan masuk dari client ini dan me-lemparnya ke broadcast channel
            incoming = ws_stream.next() => {
                match incoming {
                    Some(Ok(msg)) => {
                        if let Some(text) = msg.as_text() {
                            println!("Menerima pesan dari {addr}: {text}");
                            // Kirim pesan sekaligus alamat pengirimnya
                            let _ = bcast_tx.send((addr, text.to_string()));
                        }
                    }
                    Some(Err(e)) => return Err(e.into()),
                    None => break, // Client terputus (disconnect)
                }
            }
            // Task 2: Menerima pesan dari channel broadcast lalu mengirim ke client ini
            msg = bcast_rx.recv() => {
                match msg {
                    Ok((sender_addr, text)) => {
                        // Optional Task: Broadcast pesan ke seluruh clients, KECUALI pengirim aslinya
                        if sender_addr != addr {
                            ws_stream.send(Message::text(text)).await?;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Modifikasi channel untuk mengirimkan SocketAddr dan String
    let (bcast_tx, _) = channel::<(SocketAddr, String)>(16);
    
    // Ganti port 2000 menjadi 8080
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Listening on port 8080");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {addr:?}");
        let bcast_tx = bcast_tx.clone();
        
        tokio::spawn(async move {
            // Wrap the raw TCP stream into a websocket.
            let (_req, ws_stream) = ServerBuilder::new().accept(socket).await?;
            handle_connection(addr, ws_stream, bcast_tx).await
        });
    }
}