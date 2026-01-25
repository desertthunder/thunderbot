use crate::jetstream::event::JetstreamEvent;

use anyhow::Result;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::{net::TcpStream, time::sleep};
use tokio_tungstenite::{MaybeTlsStream, connect_async, tungstenite::Message};

pub struct JetstreamClient {
    url: String,
}

impl JetstreamClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn connect(&self) -> Result<WebSocketStream> {
        tracing::info!("Connecting to Jetstream: {}", self.url);
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (_, read) = ws_stream.split();
        Ok(WebSocketStream { read })
    }

    pub async fn connect_with_cursor(&self, cursor: i64) -> Result<WebSocketStream> {
        let url = format!("{}&cursor={}", self.url, cursor);
        tracing::info!("Connecting to Jetstream with cursor: {}", cursor);
        let (ws_stream, _) = connect_async(&url).await?;
        let (_, read) = ws_stream.split();
        Ok(WebSocketStream { read })
    }
}

impl Default for JetstreamClient {
    fn default() -> Self {
        let url = "wss://jetstream2.us-east.bsky.network/subscribe?wantedCollections=app.bsky.feed.post&compress=true"
            .to_string();
        Self { url }
    }
}

pub struct WebSocketStream {
    read: SplitStream,
}

pub type SplitStream = futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<MaybeTlsStream<TcpStream>>>;

impl WebSocketStream {
    pub async fn next_event(&mut self) -> Result<Option<JetstreamEvent>> {
        match self.read.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<JetstreamEvent>(&text) {
                Ok(event) => Ok(Some(event)),
                Err(e) => {
                    tracing::error!("Failed to parse JSON: {}", e);
                    Ok(None)
                }
            },
            Some(Ok(Message::Binary(data))) => {
                let decompressed = zstd::decode_all(&data[..])?;
                let text = String::from_utf8(decompressed)?;
                match serde_json::from_str::<JetstreamEvent>(&text) {
                    Ok(event) => Ok(Some(event)),
                    Err(e) => {
                        tracing::error!("Failed to parse JSON: {}", e);
                        Ok(None)
                    }
                }
            }
            Some(Ok(Message::Close(_))) => {
                tracing::warn!("WebSocket connection closed");
                Ok(None)
            }
            Some(Err(e)) => {
                tracing::error!("WebSocket error: {}", e);
                Err(anyhow::anyhow!("WebSocket error: {}", e))
            }
            None => Ok(None),
            _ => Ok(None),
        }
    }
}

pub struct BackoffStrategy {
    initial: Duration,
    max: Duration,
    factor: f64,
    current: Duration,
}

impl BackoffStrategy {
    pub fn standard() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            factor: 2.0,
            current: Duration::from_secs(1),
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;

        let jitter = rand::random::<u64>() % (delay.as_millis() / 4).max(1) as u64;
        let with_jitter = delay + Duration::from_millis(jitter);

        self.current = Duration::from_secs_f64((self.current.as_secs_f64() * self.factor).min(self.max.as_secs_f64()));

        with_jitter
    }

    pub fn reset(&mut self) {
        self.current = self.initial;
    }
}

pub async fn run_with_reconnect<F, Fut, T>(mut connect: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut backoff = BackoffStrategy::standard();

    loop {
        match connect().await {
            Ok(_) => {
                tracing::info!("Connection completed successfully");
                backoff.reset();
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("Connection error: {}", e);
            }
        }

        let delay = backoff.next_delay();
        tracing::info!("Reconnecting in {:?}", delay);
        sleep(delay).await;
    }
}
