pub mod claude;
pub mod generic;

use ai_smartness::provider::AiProvider;

/// Detect provider from provider ID string.
pub fn detect_provider(provider_id: &str) -> Box<dyn AiProvider> {
    match provider_id {
        "claude" | "anthropic" => Box::new(claude::ClaudeProvider),
        _ => Box::new(generic::GenericProvider),
    }
}
