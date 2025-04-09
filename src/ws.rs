use futures::stream::SplitSink;
use futures_util::StreamExt;
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    spawn,
    sync::mpsc,
    task::JoinHandle,
};
use tokio_tungstenite::{
    WebSocketStream, accept_async,
    tungstenite::{Bytes, Message},
};
use tracing::info;

pub async fn run_ws_stream(
    port: u16,
    stream_tx: mpsc::Sender<Bytes>,
) -> (
    JoinHandle<()>,
    SplitSink<WebSocketStream<TcpStream>, Message>,
) {
    let addr = format!("0.0.0.0:{}", port).parse::<SocketAddr>().unwrap();
    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("ws listening on: {}", addr);
    let (stream, _) = listener.accept().await.unwrap();
    let ws_stream = accept_async(stream).await.unwrap();
    let (ws_sender, mut ws_receiver) = ws_stream.split();
    info!("ws connection established");
    let handle = spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    stream_tx.send(data).await.unwrap();
                }
                Ok(Message::Text(text)) => {
                    info!("ws received: {}", text);
                }
                Ok(Message::Close(_)) => {
                    println!("ws connection closed");
                    break;
                }
                Err(e) => {
                    println!("ws error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });
    (handle, ws_sender)
}
