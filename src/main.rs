mod browser_capture;
mod env;
mod twitch;
mod ws;
use browser_capture::CapturedBrowser;
use std::{thread, time::Duration};
use tokio::{spawn, sync::mpsc};
use tokio_tungstenite::tungstenite::Bytes;
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use ws::get_ws_stream;

const WS_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    setup_tracing();

    let (stream_tx, stream_rx) = mpsc::channel::<Bytes>(10);
    spawn(async move {
        get_ws_stream(WS_PORT, stream_tx).await;
    });

    let mut browser = CapturedBrowser::new((1280, 720)).await;
    browser
        .start_capture("https://www.dubs.toys", WS_PORT)
        .await;

    spawn(async move {
        twitch::stream(stream_rx).await;
    });

    thread::sleep(Duration::from_secs(300));
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
