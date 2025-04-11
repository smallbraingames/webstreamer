mod event_ws;
use event_ws::EventWebsocketClient;
use reqwest::Client;
use serde_json::json;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::Arc,
    time::SystemTime,
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
    eventsub::{Event, Message, Payload},
    helix::{Scope, users::User},
    twitch_oauth2::{ClientSecret, DeviceUserTokenBuilder, TwitchToken, UserToken},
    types::UserId,
};

pub async fn run_twitch(
    twitch_client_id: &str,
    twitch_client_secret: &str,
    twitch_rtmp_url: &str,
    stream_rx: Receiver<Bytes>,
    ws_tx: Sender<String>,
) -> (JoinHandle<()>, JoinHandle<()>) {
    let server = TwitchServer::new(twitch_client_id, twitch_client_secret).await;
    let event_listener = spawn(async move { server.run_event_listener(ws_tx).await });
    let twitch_rtmp_url = twitch_rtmp_url.to_string();
    let stream = spawn(async move { run_stream(&twitch_rtmp_url, stream_rx).await });
    (event_listener, stream)
}

struct TwitchServer {
    user_token: Arc<Mutex<UserToken>>,
    helix_client: HelixClient<'static, Client>,
}

impl TwitchServer {
    pub async fn new(twitch_client_id: &str, twitch_client_secret: &str) -> Self {
        let client: HelixClient<reqwest::Client> = twitch_api::HelixClient::with_client(
            ClientDefault::default_client_with_name(Some("webstreamer".parse().unwrap())).unwrap(),
        );
        let mut builder = DeviceUserTokenBuilder::new(
            twitch_client_id,
            vec![Scope::UserReadChat, Scope::UserWriteChat],
        );
        let code = builder.start(&client).await.unwrap();
        info!("authenticate twitch: {}", code.verification_uri);
        let mut user_token = builder.wait_for_code(&client, sleep).await.unwrap();
        user_token.set_secret(Some(ClientSecret::new(twitch_client_secret.to_string())));
        TwitchServer {
            user_token: Arc::new(Mutex::new(user_token)),
            helix_client: client,
        }
    }

    pub async fn run_event_listener(&self, ws_tx: Sender<String>) {
        type CachedUser = (User, SystemTime);

        let user_cache = Arc::new(Mutex::new(HashMap::<UserId, CachedUser>::new()));
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
            let client = self.helix_client.clone();
            let token = self.user_token.clone();
            let user_cache = user_cache.clone();
            async move {
                info!("ws event: {:?}, timestamp: {:?}", e, ts);
                let message = json!({
                    "type": "twitch-event",
                    "event": e,
                    "timestamp": ts
                });
                ws_tx.send(message.to_string()).await.unwrap();
                if let Event::ChannelChatMessageV1(Payload {
                    message: Message::Notification(payload),
                    ..
                }) = e
                {
                    info!("message from user_id: {}", payload.chatter_user_id);
                    let id = payload.chatter_user_id;
                    let mut user_cache = user_cache.lock().await;

                    let now = SystemTime::now();
                    let cache_timeout = std::time::Duration::from_secs(30 * 60); // 30 minutes

                    let is_cache_valid = user_cache
                        .get(&id)
                        .map(|(_, cached_time)| {
                            now.duration_since(*cached_time).unwrap_or(cache_timeout)
                                < cache_timeout
                        })
                        .unwrap_or(false);

                    let user = if is_cache_valid {
                        user_cache.get(&id).unwrap().0.clone()
                    } else {
                        let log_message = if user_cache.contains_key(&id) {
                            "cache expired for user_id"
                        } else {
                            "fetching user info"
                        };
                        info!("{}: {}", log_message, id);

                        let token = token.lock().await;
                        let user = client
                            .get_user_from_id(&id, &*token)
                            .await
                            .unwrap()
                            .unwrap();

                        user_cache.insert(id.clone(), (user.clone(), now));
                        user
                    };

                    let message = json!({
                        "type": "twitch-user",
                        "user": user,
                    });
                    ws_tx.send(message.to_string()).await.unwrap();
                }
            }
        });
        join!(ws, refresh_token);
    }
}

pub async fn run_stream(twitch_rtmp_url: &str, mut stream_rx: Receiver<Bytes>) {
    info!("starting ffmpeg process");
    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-i",
            "-",
            "-vsync",
            "cfr",
            "-c:v",
            "h264_nvenc",
            "-rc",
            "cbr",
            "-r",
            "60",
            "-s",
            "1280x720",
            "-b:v",
            "4500k",
            "-maxrate",
            "4500k",
            "-bufsize",
            "9000k",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-f",
            "flv",
            twitch_rtmp_url,
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
