mod event_ws;
use event_ws::EventWebsocketClient;
use reqwest::Client;
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::Arc,
};
use tokio::{
    join, spawn,
    sync::{
        Mutex,
        mpsc::{Receiver, Sender},
    },
    task::JoinHandle,
    time::sleep,
};
use tokio_tungstenite::tungstenite::Bytes;
use tracing::{info, warn};
use twitch_api::{
    HelixClient, TWITCH_EVENTSUB_WEBSOCKET_URL,
    client::ClientDefault,
    helix::Scope,
    twitch_oauth2::{DeviceUserTokenBuilder, TwitchToken, UserToken},
};

pub async fn run_twitch(
    twitch_client_id: &str,
    twitch_rmtp_url: &str,
    stream_rx: Receiver<Bytes>,
    ws_tx: Sender<String>,
) -> (JoinHandle<()>, JoinHandle<()>) {
    let server = TwitchServer::new(twitch_client_id).await;
    let event_listener = spawn(async move { server.run_event_listener(ws_tx).await });
    let twitch_rmtp_url = twitch_rmtp_url.to_string();
    let stream = spawn(async move { run_stream(&twitch_rmtp_url, stream_rx).await });
    (event_listener, stream)
}

struct TwitchServer {
    user_token: Arc<Mutex<UserToken>>,
    helix_client: HelixClient<'static, Client>,
}

impl TwitchServer {
    pub async fn new(twitch_client_id: &str) -> Self {
        let client: HelixClient<reqwest::Client> = twitch_api::HelixClient::with_client(
            ClientDefault::default_client_with_name(Some("webstreamer".parse().unwrap())).unwrap(),
        );
        let mut builder = DeviceUserTokenBuilder::new(
            twitch_client_id,
            vec![Scope::UserReadChat, Scope::UserWriteChat],
        );
        let code = builder.start(&client).await.unwrap();
        info!("authenticate twitch: {}", code.verification_uri);
        let user_token = builder.wait_for_code(&client, sleep).await.unwrap();
        TwitchServer {
            user_token: Arc::new(Mutex::new(user_token)),
            helix_client: client,
        }
    }

    pub async fn run_event_listener(&self, ws_tx: Sender<String>) {
        let ws = EventWebsocketClient {
            session_id: None,
            token: self.user_token.clone(),
            client: self.helix_client.clone(),
            chats: vec![self.user_token.lock().await.user_id.clone()],
            connect_url: TWITCH_EVENTSUB_WEBSOCKET_URL.to_string(),
        };
        let refresh_token = async move {
            let token = self.user_token.clone();
            let client = self.helix_client.clone();
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let mut token = token.lock().await;
                if token.expires_in() < std::time::Duration::from_secs(60) {
                    token.refresh_token(&self.helix_client).await.unwrap();
                }
                token.validate_token(&client).await.unwrap();
            }
        };
        let ws = ws.run(|e, ts| {
            let ws_tx = ws_tx.clone();
            async move {
                info!("ws event: {:?}, timestamp: {:?}", e, ts);
                let message = json!({
                    "type": "twitch",
                    "event": e,
                    "timestamp": ts
                });
                ws_tx.send(message.to_string()).await.unwrap();
            }
        });
        join!(ws, refresh_token);
    }
}

pub async fn run_stream(twitch_rmtp_url: &str, mut stream_rx: Receiver<Bytes>) {
    info!("starting ffmpeg process");
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
            twitch_rmtp_url,
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
}
