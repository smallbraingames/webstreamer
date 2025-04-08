mod env;
use chromiumoxide::{
    Browser, BrowserConfig, browser::HeadlessMode, cdp::browser_protocol::log::EventEntryAdded,
};
use futures_util::StreamExt;
use std::{
    fs::File,
    io::Write,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tokio::{net::TcpListener, spawn, time::sleep};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{Level, debug, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const WS_PORT: u16 = 8080;

#[tokio::main]
async fn main() {
    setup_tracing();

    let file_path = "recording.webm";
    spawn(async move {
        run_websocket_server(file_path).await;
    });

    let extension_path = Path::new("./extension").canonicalize().unwrap();
    let extension_id = include_str!("../extension/id.txt").trim();
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .with_head()
            .extension(extension_path.to_str().unwrap())
            .disable_default_args()
            .headless_mode(HeadlessMode::False)
            .arg("--autoplay-policy=no-user-gesture-required")
            .arg("--auto-accept-this-tab-capture")
            .arg("--no-sandbox")
            .arg("--disable-setuid-sandbox")
            .arg(format!(
                "--disable-extensions-except={}",
                extension_path.to_str().unwrap()
            ))
            .arg(format!("--allowlisted-extension-id={}", extension_id))
            .window_size(500, 500)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();

    let handle = spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                warn!("invalid message in handler: {:?}", h);
            }
        }
    });

    browser.clear_cookies().await.unwrap();

    let page = browser.new_page("https://youtube.com").await.unwrap();
    page.wait_for_navigation_response().await.unwrap();

    let mut events = page.event_listener::<EventEntryAdded>().await.unwrap();
    spawn(async move {
        while let Some(event) = events.next().await {
            debug!(
                "brower log: [{:?}] {:?}",
                event.entry.level, event.entry.text
            );
        }
    });

    page.evaluate(format!(
        r#"
        window.postMessage({{
            type: 'CAPTURE_COMMAND',
            command: 'start',
            port: {}
        }}, '*');
        console.log("[rs] sent start capture message");
        "#,
        WS_PORT
    ))
    .await
    .unwrap();

    info!("sent start message");

    thread::sleep(Duration::from_secs(30));

    browser.close().await.unwrap();
    handle.await.unwrap();
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

async fn run_websocket_server(file_path: &str) {
    let file = Arc::new(Mutex::new(File::create(file_path).unwrap()));

    let addr = format!("0.0.0.0:{}", WS_PORT)
        .parse::<SocketAddr>()
        .unwrap();
    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("ws server listening on: {}", addr);

    if let Ok((stream, _)) = listener.accept().await {
        info!("new ws connection");

        let file_clone = file.clone();

        if let Ok(ws_stream) = accept_async(stream).await {
            let (mut ws_sender, mut ws_receiver) = ws_stream.split();

            tokio::spawn(async move {
                while let Some(msg) = ws_receiver.next().await {
                    match msg {
                        Ok(Message::Binary(data)) => {
                            info!("got data");
                            if let Ok(mut file) = file_clone.lock() {
                                file.write_all(&data).unwrap();
                            }
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

            tokio::spawn(async move {
                // while let Some(msg) = rx.recv().await {
                //     if ws_sender.send(msg).await.is_err() {
                //         break;
                //     }
                // }
            });
        }
    }
}
