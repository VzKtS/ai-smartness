//! GuardCode rules -- trait + built-in rules.

/// Result of a rule check.
#[derive(Debug, Clone)]
pub enum RuleResult {
    Pass,
    Warn(String),
    Block(String),
}

/// Trait for content validation rules.
pub trait Rule: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self, content: &str) -> RuleResult;
}

/// Built-in: maximum content length.
pub struct MaxLengthRule {
    pub max_bytes: usize,
}

impl Rule for MaxLengthRule {
    fn name(&self) -> &str {
        "max_length"
    }
    fn check(&self, content: &str) -> RuleResult {
        if content.len() > self.max_bytes {
            tracing::debug!(size = content.len(), max = self.max_bytes, "MaxLength: exceeded");
            RuleResult::Block(format!(
                "Content exceeds max length: {} > {}",
                content.len(),
                self.max_bytes
            ))
        } else {
            RuleResult::Pass
        }
    }
}

/// Built-in: blocked patterns (substring match).
pub struct BlockedPatternRule {
    pub patterns: Vec<String>,
}

impl Rule for BlockedPatternRule {
    fn name(&self) -> &str {
        "blocked_pattern"
    }
    fn check(&self, content: &str) -> RuleResult {
        for pattern in &self.patterns {
            if content.contains(pattern.as_str()) {
                tracing::warn!(pattern = %pattern, "BlockedPattern: match found");
                return RuleResult::Block(format!("Blocked pattern found: {}", pattern));
            }
        }
        RuleResult::Pass
    }
}
