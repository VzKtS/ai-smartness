//! User profile — persistent preferences and behavioral patterns.
//!
//! Persisted as `{agent_data_dir}/user_profile.json`.
//! Auto-detected from user messages in inject hook.
//! Editable from GUI Settings > Profile tab.

use std::collections::VecDeque;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub created_at: String,
    pub updated_at: String,
    pub identity: Identity,
    pub preferences: Preferences,
    pub context_rules: VecDeque<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// "user", "developer", "owner"
    pub role: String,
    /// "user", "contributor", "owner"
    pub relationship: String,
    /// Optional user name
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    /// "en", "fr", "es", etc.
    pub language: String,
    /// "concise", "normal", "detailed"
    pub verbosity: String,
    pub emoji_usage: bool,
    /// "beginner", "intermediate", "expert"
    pub technical_level: String,
}

const PROFILE_FILE: &str = "user_profile.json";
const MAX_CONTEXT_RULES: usize = 20;

impl Default for UserProfile {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            created_at: now.clone(),
            updated_at: now,
            identity: Identity {
                role: "user".to_string(),
                relationship: "user".to_string(),
                name: None,
            },
            preferences: Preferences {
                language: "en".to_string(),
                verbosity: "normal".to_string(),
                emoji_usage: false,
                technical_level: "intermediate".to_string(),
            },
            context_rules: VecDeque::new(),
        }
    }
}

impl UserProfile {
    /// Load profile from file, or create default if absent/corrupted.
    pub fn load(agent_data_dir: &Path) -> Self {
        let path = agent_data_dir.join(PROFILE_FILE);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save profile to file.
    pub fn save(&mut self, agent_data_dir: &Path) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = std::fs::create_dir_all(agent_data_dir) {
            tracing::warn!(error = %e, "Failed to create agent data dir for profile");
            return;
        }
        let path = agent_data_dir.join(PROFILE_FILE);
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::warn!(error = %e, "Failed to write user_profile.json");
                }
            }
            Err(e) => tracing::warn!(error = %e, "Failed to serialize user profile"),
        }
    }

    /// Add a context rule (dedup, max N).
    pub fn add_rule(&mut self, rule: String) -> bool {
        if self.context_rules.contains(&rule) {
            return false;
        }
        self.context_rules.push_back(rule);
        if self.context_rules.len() > MAX_CONTEXT_RULES {
            self.context_rules.pop_front();
        }
        true
    }

    /// Auto-detect profile traits from a user message.
    pub fn detect_from_message(&mut self, message: &str) {
        let msg = message.to_lowercase();

        // Ownership detection
        if ["mon projet", "my project", "j'ai créé", "i created", "i built", "j'ai construit"]
            .iter()
            .any(|p| msg.contains(p))
        {
            self.identity.relationship = "owner".to_string();
        }

        // Developer role detection
        if ["implement", "debug", "refactor", "implémente", "deploy", "compile", "build"]
            .iter()
            .any(|p| msg.contains(p))
        {
            self.identity.role = "developer".to_string();
        }

        // Technical level detection
        let expert_terms = [
            "api", "async", "hook", "mcp", "daemon", "socket", "embedding",
            "mutex", "pipeline", "container", "kubernetes",
        ];
        if expert_terms.iter().filter(|t| msg.contains(**t)).count() >= 3 {
            self.preferences.technical_level = "expert".to_string();
        }
    }

    /// Remove a rule by index (0-based).
    pub fn remove_rule(&mut self, index: usize) -> Option<String> {
        if index < self.context_rules.len() {
            Some(self.context_rules.remove(index).unwrap())
        } else {
            None
        }
    }

    /// Clear all context rules.
    pub fn clear_rules(&mut self) {
        self.context_rules.clear();
    }

    /// Detect user rules from message.
    /// DISABLED — auto-detection captures garbage fragments.
    /// Use ai_profile set_rule for explicit rule management.
    pub fn detect_rules(&mut self, _message: &str) -> Option<String> {
        None
    }

    /// Build a lean injection string for Layer 5.5.
    /// Format: identity line + pin hint + numbered rules with tool hints.
    pub fn build_injection(&self) -> Option<String> {
        let mut parts = Vec::new();

        // Identity line
        let mut id_parts = vec![self.identity.role.clone()];
        if self.identity.relationship != "user" {
            id_parts.push(format!("({})", self.identity.relationship));
        }
        if let Some(ref name) = self.identity.name {
            id_parts.push(format!("name: {}", name));
        }
        id_parts.push(format!("{} level", self.preferences.technical_level));
        if self.preferences.verbosity != "normal" {
            id_parts.push(format!("prefers {} responses", self.preferences.verbosity));
        }
        parts.push(format!("User profile: {}", id_parts.join(", ")));

        parts.push("Custom sections: use ai_pin for persistent reminders".to_string());

        // Numbered rules with tool hint
        if !self.context_rules.is_empty() {
            parts.push("Rules (ai_profile set_rule / remove_rule):".to_string());
            for (i, rule) in self.context_rules.iter().enumerate() {
                parts.push(format!("{}. {}", i + 1, rule));
            }
        }

        Some(parts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_rule_dedup() {
        let mut profile = UserProfile::default();
        assert!(profile.add_rule("Always bump version".into()));
        assert!(!profile.add_rule("Always bump version".into())); // dedup
        assert_eq!(profile.context_rules.len(), 1);
    }

    #[test]
    fn test_remove_rule_by_index() {
        let mut profile = UserProfile::default();
        profile.add_rule("Rule A".into());
        profile.add_rule("Rule B".into());
        profile.add_rule("Rule C".into());

        let removed = profile.remove_rule(1);
        assert_eq!(removed, Some("Rule B".into()));
        assert_eq!(profile.context_rules.len(), 2);
        assert_eq!(profile.context_rules[0], "Rule A");
        assert_eq!(profile.context_rules[1], "Rule C");

        // Out of bounds
        assert_eq!(profile.remove_rule(99), None);
    }

    #[test]
    fn test_clear_rules() {
        let mut profile = UserProfile::default();
        profile.add_rule("Rule 1".into());
        profile.add_rule("Rule 2".into());
        profile.clear_rules();
        assert!(profile.context_rules.is_empty());
    }

    #[test]
    fn test_build_injection_numbered_format() {
        let mut profile = UserProfile::default();
        profile.identity.role = "developer".into();
        profile.preferences.technical_level = "expert".into();
        profile.add_rule("Never modify LLM prompts".into());
        profile.add_rule("Always bump version BEFORE build".into());

        let injection = profile.build_injection().unwrap();
        assert!(injection.contains("User profile: developer, expert level"));
        assert!(injection.contains("Custom sections: use ai_pin"));
        assert!(injection.contains("Rules (ai_profile set_rule / remove_rule):"));
        assert!(injection.contains("1. Never modify LLM prompts"));
        assert!(injection.contains("2. Always bump version BEFORE build"));
    }

    #[test]
    fn test_build_injection_no_rules() {
        let profile = UserProfile::default();
        let injection = profile.build_injection().unwrap();
        assert!(injection.contains("User profile:"));
        assert!(!injection.contains("Rules"));
    }

    #[test]
    fn test_detect_rules_disabled() {
        let mut profile = UserProfile::default();
        // These would previously trigger auto-detection
        assert_eq!(profile.detect_rules("toujours utiliser bun pour les builds"), None);
        assert_eq!(profile.detect_rules("always use TypeScript for frontend"), None);
        assert_eq!(profile.detect_rules("never commit without tests"), None);
        assert!(profile.context_rules.is_empty());
    }
}
