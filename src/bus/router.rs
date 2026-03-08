/// Topic pattern matching with wildcard support
///
/// Patterns:
/// - "*" matches exactly one segment
/// - "#" matches zero or more segments
/// - Exact strings match literally
///
/// Examples:
/// - "user.*" matches "user.created" but not "user.profile.updated"
/// - "user.#" matches "user.created" and "user.profile.updated"
/// - "*.created" matches "user.created" and "post.created"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicPattern {
    segments: Vec<PatternSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PatternSegment {
    Exact(String),
    SingleWildcard,  // *
    MultiWildcard,   // #
}

impl TopicPattern {
    pub fn new(pattern: &str) -> Self {
        let segments = pattern
            .split('.')
            .map(|s| match s {
                "*" => PatternSegment::SingleWildcard,
                "#" => PatternSegment::MultiWildcard,
                _ => PatternSegment::Exact(s.to_string()),
            })
            .collect();

        Self { segments }
    }

    pub fn matches(&self, topic: &str) -> bool {
        let topic_segments: Vec<&str> = topic.split('.').collect();
        self.matches_segments(&topic_segments, 0, 0)
    }

    fn matches_segments(&self, topic: &[&str], pattern_idx: usize, topic_idx: usize) -> bool {
        // Both exhausted - match
        if pattern_idx >= self.segments.len() && topic_idx >= topic.len() {
            return true;
        }

        // Pattern exhausted but topic remains - no match
        if pattern_idx >= self.segments.len() {
            return false;
        }

        match &self.segments[pattern_idx] {
            PatternSegment::Exact(s) => {
                if topic_idx >= topic.len() || topic[topic_idx] != s {
                    false
                } else {
                    self.matches_segments(topic, pattern_idx + 1, topic_idx + 1)
                }
            }
            PatternSegment::SingleWildcard => {
                if topic_idx >= topic.len() {
                    false
                } else {
                    self.matches_segments(topic, pattern_idx + 1, topic_idx + 1)
                }
            }
            PatternSegment::MultiWildcard => {
                // Multi-wildcard matches zero or more segments
                // Try matching from current position onwards
                for i in topic_idx..=topic.len() {
                    if self.matches_segments(topic, pattern_idx + 1, i) {
                        return true;
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let pattern = TopicPattern::new("user.created");
        assert!(pattern.matches("user.created"));
        assert!(!pattern.matches("user.updated"));
        assert!(!pattern.matches("user.created.now"));
    }

    #[test]
    fn test_single_wildcard() {
        let pattern = TopicPattern::new("user.*");
        assert!(pattern.matches("user.created"));
        assert!(pattern.matches("user.updated"));
        assert!(!pattern.matches("user.profile.updated"));
        assert!(!pattern.matches("user"));

        let pattern2 = TopicPattern::new("*.created");
        assert!(pattern2.matches("user.created"));
        assert!(pattern2.matches("post.created"));
        assert!(!pattern2.matches("user.updated"));
    }

    #[test]
    fn test_multi_wildcard() {
        let pattern = TopicPattern::new("user.#");
        assert!(pattern.matches("user.created"));
        assert!(pattern.matches("user.profile.updated"));
        assert!(pattern.matches("user.profile.avatar.changed"));

        let pattern2 = TopicPattern::new("#.created");
        assert!(pattern2.matches("created"));
        assert!(pattern2.matches("user.created"));
        assert!(pattern2.matches("user.profile.created"));
    }

    #[test]
    fn test_match_all() {
        let pattern = TopicPattern::new("#");
        assert!(pattern.matches("anything"));
        assert!(pattern.matches("user.created"));
        assert!(pattern.matches("a.b.c.d.e"));
    }

    #[test]
    fn test_mixed_wildcards() {
        let pattern = TopicPattern::new("user.*.#");
        assert!(pattern.matches("user.profile.updated"));
        assert!(pattern.matches("user.profile.avatar.changed"));
        assert!(pattern.matches("user.created")); // # matches 0 segments
        assert!(!pattern.matches("user")); // * requires 1 segment
    }
}
