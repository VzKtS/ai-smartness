use serde::{Deserialize, Serialize};

/// Payload provider-agnostic pour l'injection de contexte
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionPayload {
    pub threads: Vec<super::thread::Thread>,
    pub bridges: Vec<super::bridge::ThinkBridge>,
    pub reminders: Vec<String>,
    pub cognitive_messages: Vec<super::message::Message>,
    pub health_status: Option<super::HealthStatus>,
}

/// Evenements cognitifs parses depuis la sortie AI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CognitiveEvent {
    NewThread { title: String, content: String },
    UpdateThread { thread_id: String, content: String },
    Decision { description: String },
    ToolUse { tool: String, args: serde_json::Value },
}

/// Mecanisme de hook supporte par le provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookMechanism {
    ClaudeCodeHooks,
    CustomStdio,
    McpOnly,
    None,
}

/// Abstraction du fournisseur AI
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn format_injection(&self, context: &InjectionPayload) -> String;
    fn parse_output(&self, raw: &str) -> Vec<CognitiveEvent>;
    fn hook_mechanism(&self) -> HookMechanism;
}
