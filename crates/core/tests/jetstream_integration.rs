use tnbot_core::jetstream::{JetstreamClient, JetstreamConfig, JetstreamEvent};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

/// Integration test that connects to Jetstream and verifies it can receive events
///
/// Run with: cargo test --test jetstream_integration -- --ignored
#[tokio::test]
#[ignore]
async fn test_jetstream_connection_and_receive_events() {
    let (tx, mut rx) = mpsc::channel(100);

    let config = JetstreamConfig {
        host: "wss://jetstream2.us-east.bsky.network".to_string(),
        wanted_collections: vec!["app.bsky.feed.post".to_string()],
        wanted_dids: vec![],
        compress: true,
        cursor: None,
        max_message_size_bytes: None,
    };

    let client = JetstreamClient::new(config, tx);

    let client_handle = tokio::spawn(async move { tokio::time::timeout(Duration::from_secs(10), client.run()).await });

    let event = timeout(Duration::from_secs(8), rx.recv()).await;

    assert!(event.is_ok(), "Should receive at least one event within timeout");

    let event = event.unwrap();
    assert!(event.is_some(), "Event should not be None");

    let event = event.unwrap();

    match event {
        JetstreamEvent::Commit { did, time_us, commit } => {
            assert!(!did.is_empty(), "DID should not be empty");
            assert!(time_us > 0, "time_us should be positive");
            assert!(!commit.collection.is_empty(), "Collection should not be empty");
            println!("✓ Received commit event: {} - {} ({})", did, commit.collection, time_us);
        }
        JetstreamEvent::Identity { did, time_us, .. } => {
            assert!(!did.is_empty(), "DID should not be empty");
            assert!(time_us > 0, "time_us should be positive");
            println!("✓ Received identity event: {} ({})", did, time_us);
        }
        JetstreamEvent::Account { did, time_us, .. } => {
            assert!(!did.is_empty(), "DID should not be empty");
            assert!(time_us > 0, "time_us should be positive");
            println!("✓ Received account event: {} ({})", did, time_us);
        }
    }

    client_handle.abort();

    println!("✓ Jetstream integration test passed!");
}

/// Test that verifies Jetstream can connect without compression
#[tokio::test]
#[ignore]
async fn test_jetstream_without_compression() {
    let (tx, mut rx) = mpsc::channel(100);

    let config = JetstreamConfig {
        host: "wss://jetstream2.us-east.bsky.network".to_string(),
        wanted_collections: vec!["app.bsky.feed.post".to_string()],
        wanted_dids: vec![],
        compress: false,
        cursor: None,
        max_message_size_bytes: None,
    };

    let client = JetstreamClient::new(config, tx);

    let client_handle = tokio::spawn(async move { tokio::time::timeout(Duration::from_secs(10), client.run()).await });

    let event = timeout(Duration::from_secs(8), rx.recv()).await;

    assert!(event.is_ok(), "Should receive event without compression");
    assert!(event.unwrap().is_some(), "Event should not be None");

    client_handle.abort();

    println!("✓ Jetstream without compression test passed!");
}
