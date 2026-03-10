use crate::jetstream::types::JetstreamEvent;
use futures_util::{SinkExt, StreamExt};
use rand::RngExt;
use rustls::crypto::ring::default_provider;
use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message};

static INIT_CRYPTO: Once = Once::new();
const CURSOR_REWIND_US: i64 = 5_000_000;

fn ensure_crypto_provider() {
    INIT_CRYPTO.call_once(|| {
        default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
    });
}

#[derive(Debug, Clone)]
pub struct JetstreamConfig {
    pub host: String,
    pub wanted_collections: Vec<String>,
    pub wanted_dids: Vec<String>,
    pub compress: bool,
    pub cursor: Option<i64>,
    pub max_message_size_bytes: Option<i64>,
}

impl Default for JetstreamConfig {
    fn default() -> Self {
        Self {
            host: "wss://jetstream2.us-east.bsky.network".to_string(),
            wanted_collections: vec!["app.bsky.feed.post".to_string()],
            wanted_dids: vec![],
            compress: true,
            cursor: None,
            max_message_size_bytes: None,
        }
    }
}

pub struct JetstreamClient {
    config: JetstreamConfig,
    last_cursor: Arc<RwLock<i64>>,
    event_tx: mpsc::Sender<JetstreamEvent>,
}

impl JetstreamClient {
    pub fn new(config: JetstreamConfig, event_tx: mpsc::Sender<JetstreamEvent>) -> Self {
        Self { config, last_cursor: Arc::new(RwLock::new(0)), event_tx }
    }

    async fn build_url(&self) -> String {
        let mut url = format!("{}/subscribe", self.config.host);
        let mut params = vec![];

        if !self.config.wanted_collections.is_empty() {
            for collection in &self.config.wanted_collections {
                params.push(format!("wantedCollections={}", urlencoding::encode(collection)));
            }
        }

        if !self.config.wanted_dids.is_empty() {
            for did in &self.config.wanted_dids {
                params.push(format!("wantedDids={}", urlencoding::encode(did)));
            }
        }

        if self.config.compress {
            params.push("compress=true".to_string());
        }

        if let Some(cursor) = self.get_cursor().await {
            params.push(format!("cursor={}", cursor));
        }

        if let Some(max_size) = self.config.max_message_size_bytes {
            params.push(format!("maxMessageSizeBytes={}", max_size));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        url
    }

    async fn get_cursor(&self) -> Option<i64> {
        let cursor = *self.last_cursor.read().await;
        if cursor > 0 { Some(cursor.saturating_sub(CURSOR_REWIND_US)) } else { self.config.cursor }
    }

    async fn update_cursor(&self, time_us: i64) {
        let mut cursor = self.last_cursor.write().await;
        *cursor = time_us;
    }

    pub async fn run(&self) {
        ensure_crypto_provider();

        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(60);

        loop {
            match self.connect_and_stream().await {
                Ok(()) => {
                    tracing::info!("Jetstream connection closed gracefully");
                    backoff = Duration::from_secs(1);
                }
                Err(e) => {
                    tracing::error!("Jetstream connection error: {}", e);

                    let jitter = rand::rng().random::<f64>() * 0.1;
                    let sleep_duration = backoff.mul_f64(1.0 + jitter);
                    tracing::warn!("Reconnecting in {:?}...", sleep_duration);
                    sleep(sleep_duration).await;

                    backoff = std::cmp::min(backoff * 2, max_backoff);
                }
            }
        }
    }

    async fn connect_and_stream(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = self.build_url().await;
        tracing::info!("Connecting to Jetstream at {}", url);

        let mut request = url.clone().into_client_request()?;
        if self.config.compress {
            request
                .headers_mut()
                .insert("Socket-Encoding", HeaderValue::from_static("zstd"));
        }

        let (ws_stream, _) = connect_async(request).await?;
        tracing::info!("Connected to Jetstream");

        self.handle_stream(ws_stream).await
    }

    async fn handle_stream(
        &self, mut ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Err(e) = self.handle_binary_message(data.to_vec()).await {
                        tracing::warn!("Failed to handle binary message: {}", e);
                    }
                }
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_text_message(text.to_string()).await {
                        tracing::warn!("Failed to handle text message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("Jetstream connection closed by server");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    ws_stream.send(Message::Pong(data)).await?;
                }
                Err(e) => {
                    return Err(e.into());
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn handle_binary_message(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let decompressed = decompress_zstd(&data)?;
        let text = String::from_utf8(decompressed)?;

        tracing::trace!("Decompressed binary message: {}", &text[..text.len().min(500)]);

        self.handle_text_message(text).await
    }

    async fn handle_text_message(&self, text: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!("Received message: {}", &text[..text.len().min(200)]);

        let event: JetstreamEvent = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to parse event: {} | JSON: {}", e, &text[..text.len().min(500)]);
                return Err(e.into());
            }
        };

        if let JetstreamEvent::Commit { time_us, .. } = &event {
            self.update_cursor(*time_us).await;
        }

        self.event_tx.send(event).await?;

        Ok(())
    }
}

include!(concat!(env!("OUT_DIR"), "/zstd_dict.rs"));

fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let mut decoder = zstd::bulk::Decompressor::with_dictionary(ZSTD_DICTIONARY)?;
    let result = decoder.decompress(data, 10 * 1024 * 1024)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jetstream_config_default() {
        let config = JetstreamConfig::default();
        assert_eq!(config.host, "wss://jetstream2.us-east.bsky.network");
        assert!(config.compress);
        assert_eq!(config.wanted_collections, vec!["app.bsky.feed.post"]);
    }

    #[test]
    fn test_build_url_basic() {
        let config = JetstreamConfig::default();
        let (tx, _rx) = mpsc::channel(100);
        let client = JetstreamClient::new(config, tx);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let url = rt.block_on(async { client.build_url().await });

        assert!(url.contains("jetstream2.us-east.bsky.network/subscribe"));
        assert!(url.contains("wantedCollections=app.bsky.feed.post"));
        assert!(url.contains("compress=true"));
    }

    #[test]
    fn test_build_url_rewinds_cursor_after_progress() {
        let config = JetstreamConfig::default();
        let (tx, _rx) = mpsc::channel(100);
        let client = JetstreamClient::new(config, tx);
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async { client.update_cursor(10_000_000).await });
        let url = rt.block_on(async { client.build_url().await });

        assert!(
            url.contains("cursor=5000000"),
            "URL should include rewound cursor: {url}"
        );
    }
}
