//! Enforcer -- evaluates all rules against content.

use super::rules::{Rule, RuleResult};

pub struct Enforcer {
    rules: Vec<Box<dyn Rule>>,
}

impl Enforcer {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
    }

    /// Check all rules against content. Returns results for each rule.
    pub fn check_all(&self, content: &str) -> Vec<RuleResult> {
        let results: Vec<RuleResult> = self.rules.iter().map(|r| r.check(content)).collect();
        tracing::debug!(rules_count = self.rules.len(), content_len = content.len(), "GuardCode check_all");
        for result in &results {
            match result {
                RuleResult::Block(reason) => tracing::warn!(reason = %reason, "GuardCode: BLOCKED"),
                RuleResult::Warn(reason) => tracing::warn!(reason = %reason, "GuardCode: WARNING"),
                RuleResult::Pass => {}
            }
        }
        results
    }

    /// Check if any rule blocks the content.
    pub fn is_blocked(&self, content: &str) -> bool {
        let blocked = self.check_all(content)
            .iter()
            .any(|r| matches!(r, RuleResult::Block(_)));
        if blocked {
            tracing::warn!(content_len = content.len(), "GuardCode: content blocked");
        }
        blocked
    }
}

impl Default for Enforcer {
    fn default() -> Self {
        Self::new()
    }
}
