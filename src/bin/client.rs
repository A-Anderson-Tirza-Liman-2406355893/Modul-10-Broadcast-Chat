use futures_util::SinkExt;
use futures_util::stream::StreamExt;
use http::Uri;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_websockets::{ClientBuilder, Message};

#[tokio::main]
async fn main() -> Result<(), tokio_websockets::Error> {
    let (mut ws_stream, _) = ClientBuilder::from_uri(Uri::from_static("ws://127.0.0.1:2000"))
        .connect()
        .await?;

    let stdin = tokio::io::stdin();
    let mut stdin = BufReader::new(stdin).lines();

    println!("Connected to the chat server! You can start typing...");

    loop {
        tokio::select! {
            // Task 1: Membaca input teks user dari terminal dan mengirim ke server
            line = stdin.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        ws_stream.send(Message::text(text)).await?;
                    }
                    Ok(None) => break, // End of File
                    Err(e) => {
                        eprintln!("Error reading from stdin: {}", e);
                        break;
                    }
                }
            }
            // Task 2: Menerima pesan yang di-broadcast server dan menampilkan ke user
            incoming = ws_stream.next() => {
                match incoming {
                    Some(Ok(msg)) => {
                        if let Some(text) = msg.as_text() {
                            println!("[Pesan Masuk] : {}", text);
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("Error receiving from server: {}", e);
                        break;
                    }
                    None => {
                        println!("Server terputus.");
                        break;
                    }
                }
            }
        }
    }
    
    Ok(())
}