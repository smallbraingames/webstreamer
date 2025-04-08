mod browser_capture;
mod env;
use browser_capture::CapturedBrowser;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tokio::{spawn, sync::mpsc};
use tokio_tungstenite::tungstenite::Bytes;
use tracing::{Level, info, warn};
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

    let mut stream_rx = stream_rx;
    spawn(async move {
        info!("starting ffmpeg process");
        let twitch_rmtp_url = env!("TWITCH_RMTP_URL");
        let mut ffmpeg = Command::new("ffmpeg")
            .args([
                "-i",
                "-",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-tune",
                "zerolatency",
                "-b:v",
                "3000k",
                "-maxrate",
                "3000k",
                "-bufsize",
                "6000k",
                "-pix_fmt",
                "yuv420p",
                "-g",
                "30",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-ar",
                "44100",
                "-f",
                "flv",
                &twitch_rmtp_url,
            ])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        let mut stdin = ffmpeg.stdin.take().unwrap();

        let stderr = ffmpeg.stderr.take().unwrap();

        spawn(async move {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => info!("ffmpeg: {}", line),
                    Err(e) => warn!("Error reading ffmpeg stderr: {}", e),
                }
            }
        });

        while let Some(data) = stream_rx.recv().await {
            stdin.write_all(&data).unwrap();
        }
        drop(stdin);
        match ffmpeg.wait() {
            Ok(status) => info!("ffmpeg process exited with status: {}", status),
            Err(e) => warn!("failed to wait for ffmpeg process: {}", e),
        }
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

mod ws {
    use futures_util::StreamExt;
    use std::net::SocketAddr;
    use tokio::{net::TcpListener, sync::mpsc};
    use tokio_tungstenite::{
        accept_async,
        tungstenite::{Bytes, Message},
    };
    use tracing::info;

    pub async fn get_ws_stream(port: u16, stream_tx: mpsc::Sender<Bytes>) {
        let addr = format!("0.0.0.0:{}", port).parse::<SocketAddr>().unwrap();
        let listener = TcpListener::bind(&addr).await.unwrap();
        info!("ws listening on: {}", addr);
        let (stream, _) = listener.accept().await.unwrap();
        let ws_stream = accept_async(stream).await.unwrap();
        let (_, mut ws_receiver) = ws_stream.split();
        info!("ws connection established");
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    info!("recv data");
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
    }
}
