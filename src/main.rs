mod browser_capture;
mod twitch;
mod ws;
use browser_capture::CapturedBrowser;
use futures::SinkExt;
use std::{env, time::Duration};
use tokio::{join, spawn, sync::mpsc, time::sleep};
use tokio_tungstenite::tungstenite::{Bytes, Message};
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use twitch::run_twitch;
use ws::run_ws_stream;

const WS_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    setup_tracing();
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap();

    let (stream_tx, stream_rx) = mpsc::channel::<Bytes>(10);
    let (ws_json_tx, mut ws_json_rx) = mpsc::channel::<String>(10);

    let twitch_client_id = env::var("TWITCH_CLIENT_ID").unwrap();
    let twitch_rmtp_url = env::var("TWITCH_RMTP_URL").unwrap();

    let (twitch_server_handle, twitch_event_handle) =
        run_twitch(&twitch_client_id, &twitch_rmtp_url, stream_rx, ws_json_tx).await;

    let mut captured_browser = CapturedBrowser::new((1280, 720)).await;
    let browser_handle = spawn(async move {
        sleep(Duration::from_secs(1)).await;
        captured_browser
            .start_capture("https://wikipedia.com", WS_PORT)
            .await;
        loop {
            sleep(Duration::from_secs(1)).await;
        }
    });

    let (ws_handle, mut ws_rx) = run_ws_stream(WS_PORT, stream_tx).await;
    let ws_forward_handle = spawn(async move {
        loop {
            let message = ws_json_rx.recv().await.unwrap();
            ws_rx.send(Message::text(message)).await.unwrap();
        }
    });

    let _results = join!(
        browser_handle,
        ws_handle,
        ws_forward_handle,
        twitch_server_handle,
        twitch_event_handle
    );
}

fn setup_tracing() {
    let filter = EnvFilter::from_default_env()
        .add_directive(Level::INFO.into())
        .add_directive("webstreamer=debug".parse().unwrap());

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .pretty()
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();
}
