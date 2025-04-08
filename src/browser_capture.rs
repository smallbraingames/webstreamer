use chromiumoxide::{Browser, BrowserConfig, Page, cdp::browser_protocol::log::EventEntryAdded};
use futures_util::StreamExt;
use std::path::Path;
use tokio::{spawn, task::JoinHandle};
use tracing::{debug, info, warn};

pub struct CapturedBrowser {
    browser: Browser,
    handle: JoinHandle<()>,
}

impl CapturedBrowser {
    pub async fn new(window_size: (u32, u32)) -> Self {
        let extension_path = Path::new("./extension").canonicalize().unwrap();
        let extension_id = include_str!("../extension/id.txt").trim();
        let (browser, mut handler) = Browser::launch(
            BrowserConfig::builder()
                .with_head()
                .extension(extension_path.to_str().unwrap())
                .arg("--autoplay-policy=no-user-gesture-required")
                .arg("--auto-accept-this-tab-capture")
                .arg("--no-sandbox")
                .arg("--disable-setuid-sandbox")
                .arg(format!(
                    "--disable-extensions-except={}",
                    extension_path.to_str().unwrap()
                ))
                //.arg("--headless=new")
                .arg(format!("--allowlisted-extension-id={}", extension_id))
                .disable_default_args()
                .window_size(window_size.0, window_size.1)
                .viewport(None)
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
        CapturedBrowser { browser, handle }
    }

    pub async fn start_capture(&mut self, url: &str, ws_port: u16) -> Page {
        let page = self.browser.new_page(url).await.unwrap();
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
            ws_port
        ))
        .await
        .unwrap();
        info!("sent start message");

        page
    }
}

impl Drop for CapturedBrowser {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
