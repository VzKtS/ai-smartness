use ai_smartness::provider::{AiProvider, CognitiveEvent, HookMechanism, InjectionPayload};

pub struct ClaudeProvider;

impl AiProvider for ClaudeProvider {
    fn id(&self) -> &str {
        "claude"
    }

    fn format_injection(&self, context: &InjectionPayload) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Format reminders
        for reminder in &context.reminders {
            parts.push(format!(
                "<system-reminder>\n{}\n</system-reminder>",
                reminder
            ));
        }

        // Format threads as memory context
        if !context.threads.is_empty() {
            let mut ctx = String::from("AI Smartness Memory Context:\n");
            for thread in context.threads.iter().take(5) {
                ctx.push_str(&format!("- \"{}\"\n", thread.title));
            }
            parts.push(format!(
                "<system-reminder>\n{}\n</system-reminder>",
                ctx
            ));
        }

        // Format cognitive messages
        for msg in context.cognitive_messages.iter().take(3) {
            parts.push(format!(
                "<system-reminder>\nMessage from {}: {}\n</system-reminder>",
                msg.from_agent, msg.content
            ));
        }

        parts.join("\n")
    }

    fn parse_output(&self, _raw: &str) -> Vec<CognitiveEvent> {
        // Claude output parsing is handled by the extraction pipeline
        Vec::new()
    }

    fn hook_mechanism(&self) -> HookMechanism {
        HookMechanism::ClaudeCodeHooks
    }
}
