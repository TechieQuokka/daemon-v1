use daemon_v1::{
    bus::{BusMessage, MessageBus, MessageSource},
    config::DaemonConfig,
    storage::DataLayer,
};
use serde_json::json;

#[tokio::test]
async fn test_bus_publish_subscribe() {
    let config = DaemonConfig::default();
    let bus = MessageBus::new(config.bus);

    // Subscribe to test topic
    let mut rx = bus
        .subscribe("test_sub".to_string(), "test.*".to_string())
        .await
        .unwrap();

    // Publish message
    let msg = BusMessage::new(
        "test.message".to_string(),
        json!({"value": 42}),
        MessageSource::System,
    );
    bus.publish(msg).await.unwrap();

    // Receive message
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        rx.recv()
    ).await.unwrap().unwrap();

    assert_eq!(received.topic, "test.message");
    assert_eq!(received.payload["value"], 42);
}

#[tokio::test]
async fn test_data_layer_operations() {
    let config = DaemonConfig::default();
    let data_layer = DataLayer::new(config.storage);

    // Set value
    data_layer
        .set("test_key".to_string(), json!(123))
        .unwrap();

    // Get value
    let entry = data_layer.get("test_key").unwrap().unwrap();
    match entry {
        daemon_v1::storage::DataEntry::Inline(val) => {
            assert_eq!(val, json!(123));
        }
        _ => panic!("Expected inline value"),
    }

    // Delete value
    let deleted = data_layer.delete("test_key").unwrap();
    assert!(deleted.is_some());

    // Verify deleted
    let entry = data_layer.get("test_key").unwrap();
    assert!(entry.is_none());
}

#[tokio::test]
async fn test_sieve_eviction() {
    use daemon_v1::storage::StorageConfig;

    let config = StorageConfig {
        max_keys: 3,
        data_layer_path: "/data".to_string(),
    };
    let data_layer = DataLayer::new(config);

    // Fill cache
    data_layer.set("a".to_string(), json!(1)).unwrap();
    data_layer.set("b".to_string(), json!(2)).unwrap();
    data_layer.set("c".to_string(), json!(3)).unwrap();

    // Access 'a' to mark as visited
    data_layer.get("a").unwrap();

    // Insert 'd' - should evict 'b' or 'c' (not 'a' which was visited)
    data_layer.set("d".to_string(), json!(4)).unwrap();

    assert_eq!(data_layer.len().unwrap(), 3);
    assert!(data_layer.get("a").unwrap().is_some());
    assert!(data_layer.get("d").unwrap().is_some());
}
