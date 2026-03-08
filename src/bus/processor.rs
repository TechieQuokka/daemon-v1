use super::subscriber::SubscriptionRegistry;
use super::types::{BusConfig, BusMessage};
use crate::error::{DaemonError, Result};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Message bus with FIFO guarantees
///
/// Architecture:
/// - Single mpsc channel for all messages (FIFO ordering)
/// - Sequential processing task
/// - Subscription registry for routing
pub struct MessageBus {
    sender: mpsc::UnboundedSender<BusMessage>,
    registry: Arc<Mutex<SubscriptionRegistry>>,
    config: BusConfig,
}

impl MessageBus {
    pub fn new(config: BusConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let registry = Arc::new(Mutex::new(SubscriptionRegistry::new()));

        // Spawn processor task
        let processor_registry = registry.clone();
        tokio::spawn(async move {
            Self::process_messages(rx, processor_registry).await;
        });

        Self {
            sender: tx,
            registry,
            config,
        }
    }

    /// Publish a message to the bus
    pub async fn publish(&self, message: BusMessage) -> Result<()> {
        self.sender
            .send(message)
            .map_err(|e| DaemonError::Bus(format!("Failed to send message: {}", e)))
    }

    /// Subscribe to a topic pattern
    pub async fn subscribe(
        &self,
        subscriber_id: String,
        pattern: String,
    ) -> Result<mpsc::UnboundedReceiver<BusMessage>> {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut registry = self.registry.lock().await;
        registry.subscribe(subscriber_id, pattern, tx);

        Ok(rx)
    }

    /// Unsubscribe from a topic pattern
    pub async fn unsubscribe(&self, subscriber_id: &str, pattern: &str) -> Result<()> {
        let mut registry = self.registry.lock().await;
        registry.unsubscribe(subscriber_id, pattern);
        Ok(())
    }

    /// Unsubscribe from all topics
    pub async fn unsubscribe_all(&self, subscriber_id: &str) -> Result<()> {
        let mut registry = self.registry.lock().await;
        registry.unsubscribe_all(subscriber_id);
        Ok(())
    }

    /// Get subscriber count
    pub async fn subscriber_count(&self) -> usize {
        let registry = self.registry.lock().await;
        registry.subscriber_count()
    }

    /// Sequential message processing (FIFO guarantee)
    async fn process_messages(
        mut receiver: mpsc::UnboundedReceiver<BusMessage>,
        registry: Arc<Mutex<SubscriptionRegistry>>,
    ) {
        while let Some(message) = receiver.recv().await {
            // Route message to all matching subscribers
            let senders = {
                let reg = registry.lock().await;
                reg.route(&message)
            };

            // Send to all subscribers (failures are logged but don't stop processing)
            for sender in senders {
                if let Err(e) = sender.send(message.clone()) {
                    tracing::warn!("Failed to deliver message to subscriber: {}", e);
                }
            }
        }
    }
}

impl Clone for MessageBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            registry: self.registry.clone(),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::types::MessageSource;
    use serde_json::json;

    #[tokio::test]
    async fn test_publish_and_receive() {
        let bus = MessageBus::new(BusConfig::default());

        let mut rx = bus
            .subscribe("sub1".to_string(), "test.message".to_string())
            .await
            .unwrap();

        let msg = BusMessage::new("test.message".to_string(), json!({"value": 42}), MessageSource::System);
        bus.publish(msg.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.topic, "test.message");
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let bus = MessageBus::new(BusConfig::default());

        let mut rx = bus
            .subscribe("sub1".to_string(), "test.*".to_string())
            .await
            .unwrap();

        let msg1 = BusMessage::new("test.foo".to_string(), json!({}), MessageSource::System);
        let msg2 = BusMessage::new("test.bar".to_string(), json!({}), MessageSource::System);

        bus.publish(msg1).await.unwrap();
        bus.publish(msg2).await.unwrap();

        let r1 = rx.recv().await.unwrap();
        let r2 = rx.recv().await.unwrap();

        assert_eq!(r1.topic, "test.foo");
        assert_eq!(r2.topic, "test.bar");
    }

    #[tokio::test]
    async fn test_fifo_ordering() {
        let bus = MessageBus::new(BusConfig::default());

        let mut rx = bus
            .subscribe("sub1".to_string(), "#".to_string())
            .await
            .unwrap();

        // Publish multiple messages
        for i in 0..10 {
            let msg = BusMessage::new(
                format!("test.{}", i),
                json!({"value": i}),
                MessageSource::System,
            );
            bus.publish(msg).await.unwrap();
        }

        // Verify FIFO order
        for i in 0..10 {
            let received = rx.recv().await.unwrap();
            assert_eq!(received.topic, format!("test.{}", i));
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = MessageBus::new(BusConfig::default());

        let mut rx1 = bus
            .subscribe("sub1".to_string(), "test.#".to_string())
            .await
            .unwrap();
        let mut rx2 = bus
            .subscribe("sub2".to_string(), "test.#".to_string())
            .await
            .unwrap();

        let msg = BusMessage::new("test.message".to_string(), json!({}), MessageSource::System);
        bus.publish(msg).await.unwrap();

        // Both should receive
        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();

        assert_eq!(r1.topic, "test.message");
        assert_eq!(r2.topic, "test.message");
    }
}
