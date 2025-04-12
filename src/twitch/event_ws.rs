// Adapted from https://github.com/twitch-rs/twitch_api/blob/main/examples/chatbot/src/websocket.rs
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite;
use tracing::{info, warn};
use twitch_api::{
    HelixClient,
    eventsub::{
        self, Event,
        event::websocket::{EventsubWebsocketData, ReconnectPayload, SessionData, WelcomePayload},
    },
    twitch_oauth2::{TwitchToken, UserToken},
    types::{self},
};

pub struct EventWebsocketClient {
    pub session_id: Option<String>,
    pub token: Arc<Mutex<UserToken>>,
    pub client: HelixClient<'static, reqwest::Client>,
    pub chats: Vec<twitch_api::types::UserId>,
    pub connect_url: String,
}

impl EventWebsocketClient {
    /// Connect to the websocket and return the stream
    async fn connect(
        &self,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        tracing::info!("connecting to twitch");
        let config = tungstenite::protocol::WebSocketConfig::default();
        let (socket, _) =
            tokio_tungstenite::connect_async_with_config(&self.connect_url, Some(config), false)
                .await
                .unwrap();

        socket
    }

    pub async fn run<Fut>(mut self, mut event_fn: impl FnMut(Event, types::Timestamp) -> Fut)
    where
        Fut: std::future::Future<Output = ()>,
    {
        loop {
            log::info!("connecting to twitch");
            let mut s = self.connect().await;
            while let Some(msg) = futures::StreamExt::next(&mut s).await {
                let msg = match msg {
                    Err(tungstenite::Error::Protocol(
                        tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    )) => {
                        warn!(
                            "connection was sent an unexpected frame or was reset, reestablishing it"
                        );
                        continue;
                    }
                    _ => msg.unwrap(),
                };
                self.process_message(msg, &mut event_fn).await;
            }
        }
    }

    async fn process_message<Fut>(
        &mut self,
        msg: tungstenite::Message,
        event_fn: &mut impl FnMut(Event, types::Timestamp) -> Fut,
    ) where
        Fut: std::future::Future<Output = ()>,
    {
        match msg {
            tungstenite::Message::Text(s) => match Event::parse_websocket(&s).unwrap() {
                EventsubWebsocketData::Welcome {
                    payload: WelcomePayload { session },
                    ..
                }
                | EventsubWebsocketData::Reconnect {
                    payload: ReconnectPayload { session },
                    ..
                } => {
                    self.process_welcome_message(session).await;
                }
                EventsubWebsocketData::Notification { metadata, payload } => {
                    event_fn(payload, metadata.message_timestamp.into_owned()).await;
                }
                re @ EventsubWebsocketData::Revocation { .. } => {
                    panic!("got revocation event: {re:?}")
                }
                EventsubWebsocketData::Keepalive {
                    metadata: _,
                    payload: _,
                } => (),
                _ => (),
            },
            tungstenite::Message::Close(_) => {
                warn!("websocket connection closed, attempting to reconnect");
                return;
            }
            _ => (),
        }
    }

    async fn process_welcome_message(&mut self, data: SessionData<'_>) {
        info!("connected to twitch chat");
        self.session_id = Some(data.id.to_string());
        if let Some(url) = data.reconnect_url {
            self.connect_url = url.parse().unwrap();
        }
        let token = self.token.lock().await;
        let transport = eventsub::Transport::websocket(data.id.clone());
        for id in &self.chats {
            let user_id = token.user_id().unwrap().to_owned();

            let mut subs = Vec::new();
            let subscription_stream = self.client.get_eventsub_subscriptions(
                Some(eventsub::Status::Enabled),
                None,
                None,
                &*token,
            );

            futures::pin_mut!(subscription_stream);
            while let Some(result) = subscription_stream.next().await {
                if let Ok(response) = result {
                    subs.extend(response.subscriptions.into_iter().filter(|s| {
                        s.transport
                            .as_websocket()
                            .is_some_and(|t| t.session_id == data.id)
                    }));
                }
            }

            if !subs.is_empty() {
                continue;
            }

            let message =
                eventsub::channel::chat::ChannelChatMessageV1::new(id.clone(), user_id.clone());
            self.client
                .create_eventsub_subscription(message, transport.clone(), &*token)
                .await
                .unwrap();
            self.client
                .create_eventsub_subscription(
                    eventsub::channel::chat::ChannelChatNotificationV1::new(id.clone(), user_id),
                    transport.clone(),
                    &*token,
                )
                .await
                .unwrap();
        }
    }
}
