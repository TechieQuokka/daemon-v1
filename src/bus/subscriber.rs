use super::router::TopicPattern;
use super::types::BusMessage;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Manages subscriptions to topics
pub struct SubscriptionRegistry {
    subscriptions: HashMap<String, Vec<Subscription>>,
}

struct Subscription {
    subscriber_id: String,
    pattern: TopicPattern,
    sender: mpsc::UnboundedSender<BusMessage>,
}

impl SubscriptionRegistry {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }

    /// Subscribe to a topic pattern
    pub fn subscribe(
        &mut self,
        subscriber_id: String,
        pattern: String,
        sender: mpsc::UnboundedSender<BusMessage>,
    ) {
        let topic_pattern = TopicPattern::new(&pattern);
        let subscription = Subscription {
            subscriber_id: subscriber_id.clone(),
            pattern: topic_pattern,
            sender,
        };

        self.subscriptions
            .entry(pattern)
            .or_insert_with(Vec::new)
            .push(subscription);
    }

    /// Unsubscribe from a topic pattern
    pub fn unsubscribe(&mut self, subscriber_id: &str, pattern: &str) {
        if let Some(subs) = self.subscriptions.get_mut(pattern) {
            subs.retain(|s| s.subscriber_id != subscriber_id);
            if subs.is_empty() {
                self.subscriptions.remove(pattern);
            }
        }
    }

    /// Unsubscribe from all topics
    pub fn unsubscribe_all(&mut self, subscriber_id: &str) {
        for subs in self.subscriptions.values_mut() {
            subs.retain(|s| s.subscriber_id != subscriber_id);
        }
        self.subscriptions.retain(|_, subs| !subs.is_empty());
    }

    /// Route message to matching subscribers
    pub fn route(&self, message: &BusMessage) -> Vec<mpsc::UnboundedSender<BusMessage>> {
        let mut senders = Vec::new();

        for subs in self.subscriptions.values() {
            for sub in subs {
                if sub.pattern.matches(&message.topic) {
                    senders.push(sub.sender.clone());
                }
            }
        }

        senders
    }

    /// Get all subscriptions for a subscriber
    pub fn get_subscriptions(&self, subscriber_id: &str) -> Vec<String> {
        let mut patterns = Vec::new();
        for (pattern, subs) in &self.subscriptions {
            if subs.iter().any(|s| s.subscriber_id == subscriber_id) {
                patterns.push(pattern.clone());
            }
        }
        patterns
    }

    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.subscriptions.values().map(|v| v.len()).sum()
    }
}

impl Default for SubscriptionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::types::MessageSource;
    use serde_json::json;

    #[test]
    fn test_subscribe_and_route() {
        let mut registry = SubscriptionRegistry::new();
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();

        registry.subscribe("sub1".to_string(), "user.*".to_string(), tx1);
        registry.subscribe("sub2".to_string(), "user.created".to_string(), tx2);

        let msg = BusMessage::new(
            "user.created".to_string(),
            json!({}),
            MessageSource::System,
        );

        let senders = registry.route(&msg);
        assert_eq!(senders.len(), 2); // Both should match
    }

    #[test]
    fn test_unsubscribe() {
        let mut registry = SubscriptionRegistry::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        registry.subscribe("sub1".to_string(), "user.*".to_string(), tx);
        assert_eq!(registry.subscriber_count(), 1);

        registry.unsubscribe("sub1", "user.*");
        assert_eq!(registry.subscriber_count(), 0);
    }

    #[test]
    fn test_wildcard_routing() {
        let mut registry = SubscriptionRegistry::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        registry.subscribe("sub1".to_string(), "user.#".to_string(), tx);

        let msg1 = BusMessage::new(
            "user.created".to_string(),
            json!({}),
            MessageSource::System,
        );
        let msg2 = BusMessage::new(
            "user.profile.updated".to_string(),
            json!({}),
            MessageSource::System,
        );

        assert_eq!(registry.route(&msg1).len(), 1);
        assert_eq!(registry.route(&msg2).len(), 1);
    }
}
