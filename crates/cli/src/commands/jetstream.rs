use owo_colors::OwoColorize;
use std::time::Duration;
use tnbot_core::jetstream::{
    EventFilter, EventPipeline, EventProcessor, FilteredEvent, JetstreamClient, JetstreamConfig, JetstreamEvent,
    PipelineConfig, ProcessedEvent, SharedFilter,
};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Simple processor that just logs and acknowledges events
struct LoggingProcessor;

#[async_trait::async_trait]
impl EventProcessor for LoggingProcessor {
    async fn process(
        &self, mut event: FilteredEvent,
    ) -> Result<ProcessedEvent, Box<dyn std::error::Error + Send + Sync>> {
        if let JetstreamEvent::Commit { did, time_us, commit } = &event.event {
            tracing::info!(
                time_us = time_us,
                author_did = %did,
                rkey = %commit.rkey,
                "🎯 Matched mention - processing"
            );
        }

        event.acknowledge();

        Ok(ProcessedEvent { event, success: true, error: None })
    }
}

/// Listen to Jetstream with full filtering and pipeline
pub async fn listen(filter_did: Option<String>, duration: Option<u64>) {
    tracing::info!("Starting Jetstream listener with filtering pipeline...");

    let bot_did = filter_did.unwrap_or_else(|| "did:plc:placeholder".to_string());
    let filter = SharedFilter::new(EventFilter::new(bot_did));
    let pipeline_config = PipelineConfig { num_workers: 4, channel_buffer_size: 1000, max_in_flight: 100 };

    let pipeline = EventPipeline::new(pipeline_config, filter.clone(), LoggingProcessor);
    pipeline.start().await;

    let (tx, mut rx) = mpsc::channel(1000);

    let config = JetstreamConfig {
        host: "wss://jetstream2.us-east.bsky.network".to_string(),
        wanted_collections: vec!["app.bsky.feed.post".to_string()],
        wanted_dids: vec![],
        compress: true,
        cursor: None,
        max_message_size_bytes: None,
    };

    let client = JetstreamClient::new(config, tx);

    let client_handle = tokio::spawn(async move {
        client.run().await;
    });

    let pipeline_sender = pipeline.event_sender();
    let bridge_handle = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = pipeline_sender.send(event).await {
                tracing::error!("Failed to send event to pipeline: {}", e);
                break;
            }
        }
    });

    let start_time = tokio::time::Instant::now();

    loop {
        if let Some(dur) = duration
            && start_time.elapsed().as_secs() >= dur
        {
            println!("\n{}", format!("Duration limit reached ({}s)", dur).yellow());
            break;
        }

        if start_time.elapsed().as_secs().is_multiple_of(5) {
            let stats = pipeline.stats();
            print_stats(&stats, start_time.elapsed().as_secs());
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    println!("\n{}", "Shutting down...".yellow());
    pipeline.shutdown(10).await;
    client_handle.abort();
    bridge_handle.abort();

    let final_stats = pipeline.stats();
    println!("\n{}", "=== Final Statistics ===".green().bold());
    print_stats(&final_stats, start_time.elapsed().as_secs());
}

pub async fn replay(cursor: u64, filter_did: Option<String>) {
    tracing::info!("Replaying Jetstream from cursor {}...", cursor);

    let bot_did = filter_did.unwrap_or_else(|| "did:plc:placeholder".to_string());

    let (tx, mut rx) = mpsc::channel(100);

    let config = JetstreamConfig {
        host: "wss://jetstream2.us-east.bsky.network".to_string(),
        wanted_collections: vec!["app.bsky.feed.post".to_string()],
        wanted_dids: vec![],
        compress: true,
        cursor: Some(cursor as i64),
        max_message_size_bytes: None,
    };

    let client = JetstreamClient::new(config, tx);

    let client_handle = tokio::spawn(async move {
        client.run().await;
    });

    let filter = EventFilter::new(bot_did);
    let start_time = tokio::time::Instant::now();
    let mut total_events = 0u64;
    let mut filtered_events = 0u64;

    while start_time.elapsed() < Duration::from_secs(30) {
        match timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Some(event)) => {
                total_events += 1;

                if let Some(filtered) = filter.filter(event) {
                    filtered_events += 1;
                    print_filtered_event(&filtered);
                }
            }
            Ok(None) => break,
            Err(_) => continue,
        }

        if total_events.is_multiple_of(10) {
            print!("\rEvents: {} total, {} matched mentions", total_events, filtered_events);
        }
    }

    client_handle.abort();

    println!(
        "\n{}",
        format!(
            "Replay complete: {} total events, {} matched mentions",
            total_events, filtered_events
        )
        .green()
    );
}

fn print_stats(stats: &tnbot_core::jetstream::pipeline::PipelineStatsSnapshot, elapsed_secs: u64) {
    let rate = if elapsed_secs > 0 { stats.events_received as f64 / elapsed_secs as f64 } else { 0.0 };

    println!(
        "[{}s] Received: {} | Filtered: {} | Processed: {} | Failed: {} | In-flight: {} | Rate: {:.1}/s",
        elapsed_secs,
        stats.events_received,
        stats.events_filtered,
        stats.events_processed,
        stats.events_failed,
        stats.events_in_flight,
        rate
    );
}

fn print_filtered_event(filtered: &FilteredEvent) {
    if let JetstreamEvent::Commit { did, time_us, commit } = &filtered.event {
        tracing::trace!(
            time_us = time_us,
            author_did = %did,
            rkey = %commit.rkey,
            "Matched mention in replay"
        );

        println!(
            "[{}] 🎯 {} mentioned bot in {} (rkey: {})",
            time_us,
            did.cyan(),
            commit.collection,
            commit.rkey
        );
    }
}
