use ai_smartness::provider::{AiProvider, CognitiveEvent, HookMechanism, InjectionPayload};

pub struct GenericProvider;

impl AiProvider for GenericProvider {
    fn id(&self) -> &str {
        "generic"
    }

    fn format_injection(&self, context: &InjectionPayload) -> String {
        let mut parts: Vec<String> = Vec::new();

        for reminder in &context.reminders {
            parts.push(reminder.clone());
        }

        if !context.threads.is_empty() {
            let mut ctx = String::from("Memory Context:\n");
            for thread in context.threads.iter().take(5) {
                ctx.push_str(&format!("- {}\n", thread.title));
            }
            parts.push(ctx);
        }

        parts.join("\n---\n")
    }

    fn parse_output(&self, _raw: &str) -> Vec<CognitiveEvent> {
        Vec::new()
    }

    fn hook_mechanism(&self) -> HookMechanism {
        HookMechanism::McpOnly
    }
}
