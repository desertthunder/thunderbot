use crate::jetstream::event::JetstreamEvent;
use tokio::sync::mpsc;
use tracing::{info, error};

#[derive(Clone)]
pub struct EventProcessor {
    event_tx: mpsc::Sender<JetstreamEvent>,
}

impl EventProcessor {
    pub fn new(buffer_size: usize) -> Self {
        let (event_tx, mut event_rx) = mpsc::channel(buffer_size);

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Err(e) = Self::process_event(event).await {
                    error!("Error processing event: {}", e);
                }
            }
        });

        Self { event_tx }
    }

    pub async fn send(&self, event: JetstreamEvent) -> anyhow::Result<()> {
        self.event_tx.send(event).await?;
        Ok(())
    }

    async fn process_event(event: JetstreamEvent) -> anyhow::Result<()> {
        info!("Processing event: {:?}", event);
        Ok(())
    }
}
