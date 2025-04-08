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
    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .with_head()
            .extension(extension_path.to_str().unwrap())
            .disable_default_args()
            .headless_mode(HeadlessMode::New)
            .arg("--autoplay-policy=no-user-gesture-required")
            .arg("--auto-accept-this-tab-capture")
            .window_size(1920, 1080)
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

    let extension_id = "fnlggohdgbdagbaifmomnkffednjnklg";

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

    // Wait 10 seconds before starting the capture
    sleep(Duration::from_secs(10)).await;
    info!("Waited 10 seconds, now starting capture");

    // Simulate a click somewhere on the page - center of the window
    page.evaluate(format!(
        r#"
        (async () => {{


            // Now start the capture
            window.postMessage({{
                type: 'CAPTURE_COMMAND',
                command: 'start',
                port: {}
            }}, '*');
            console.log("Sent start capture message");
        }})();
        "#,
        WS_PORT
    ))
    .await
    .unwrap();

    // Short pause to ensure interactions register
    thread::sleep(Duration::from_secs(1));

    info!("sent start message");

    // Wait a moment for the page to fully load and any initial logs to appear
    thread::sleep(Duration::from_secs(30));

    // page.evaluate(
    //     r#"
    //         window.postMessage({
    //             type: 'CAPTURE_COMMAND',
    //             command: 'stop'
    //         }, '*');
    //         "#,
    // )
    // .await
    // .unwrap();

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
        .with_thread_ids(true)
        .with_thread_names(true)
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
