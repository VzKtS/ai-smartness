//! User profile — persistent preferences and behavioral patterns.
//!
//! Persisted as `{agent_data_dir}/user_profile.json`.
//! Auto-detected from user messages in inject hook.
//! Editable from GUI Settings > Profile tab.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub created_at: String,
    pub updated_at: String,
    pub identity: Identity,
    pub preferences: Preferences,
    pub context_rules: Vec<String>,
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
            context_rules: Vec::new(),
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
        self.context_rules.push(rule);
        if self.context_rules.len() > MAX_CONTEXT_RULES {
            self.context_rules.remove(0);
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

    /// Detect user rules from message (e.g. "always use TypeScript").
    /// Returns the detected rule text if found.
    pub fn detect_rules(&mut self, message: &str) -> Option<String> {
        let patterns = [
            "rappelle-toi:", "n'oublie pas:", "toujours ", "jamais ",
            "rule:", "remember:", "always ", "never ",
            "regla:", "siempre ", "nunca ",
        ];
        let msg_lower = message.to_lowercase();
        for pattern in &patterns {
            if let Some(pos) = msg_lower.find(pattern) {
                // Extract the rule text (from pattern to end of sentence)
                let rule_start = pos;
                let rule_text = &message[rule_start..];
                // Take up to end of line or 200 chars
                let rule = match rule_text.find('\n') {
                    Some(nl) => &rule_text[..nl],
                    None => &rule_text[..rule_text.len().min(200)],
                };
                let rule = rule.trim().to_string();
                if rule.len() >= 10 {
                    if self.add_rule(rule.clone()) {
                        return Some(rule);
                    }
                }
            }
        }
        None
    }

    /// Build injection text for Layer 5.5.
    pub fn build_injection(&self) -> Option<String> {
        let mut parts = Vec::new();

        // Identity line
        let mut id_parts = Vec::new();
        id_parts.push(self.identity.role.clone());
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

        // Context rules
        if !self.context_rules.is_empty() {
            parts.push("User rules:".to_string());
            for rule in self.context_rules.iter().take(10) {
                parts.push(format!("- {}", rule));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}
