use crate::jetstream::{client::JetstreamClient, filter::JetstreamFilter};
use crate::processor::EventProcessor;

pub async fn listen(filter_did: Option<String>, duration: Option<u64>) -> anyhow::Result<()> {
    let client = JetstreamClient::new();
    let filter = JetstreamFilter::new(filter_did);
    let processor = EventProcessor::new(100);
    let stream = client.connect().await?;
    let mut stream = stream;

    let start_time = std::time::Instant::now();

    loop {
        if let Some(d) = duration
            && start_time.elapsed().as_secs() >= d
        {
            tracing::info!("Duration limit reached, exiting");
            break;
        }

        match stream.next_event().await {
            Ok(Some(event)) => {
                if filter.should_process(&event) {
                    processor.send(event).await?;
                }
            }
            Ok(None) => tracing::warn!("No event received, continuing"),
            Err(e) => {
                tracing::error!("Error receiving event: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}

pub async fn replay(cursor: i64) -> anyhow::Result<()> {
    let client = JetstreamClient::new();
    let filter = JetstreamFilter::new(None);
    let processor = EventProcessor::new(100);
    let stream = client.connect_with_cursor(cursor).await?;
    let mut stream = stream;

    loop {
        match stream.next_event().await {
            Ok(Some(event)) => {
                if filter.should_process(&event) {
                    processor.send(event).await?;
                }
            }
            Ok(None) => tracing::warn!("No event received, continuing"),
            Err(e) => {
                tracing::error!("Error receiving event: {}", e);
                return Err(e);
            }
        }
    }
}
