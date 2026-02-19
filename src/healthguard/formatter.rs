//! Formatter — converts HealthFindings into injectable text.
//!
//! Two output modes:
//!   - injection: system-reminder block for stdin injection (imposed on agent)
//!   - suggestion: structured JSON for ai_suggestions MCP tool (optional)

use super::{HealthFinding, HealthGuardPrompts, HealthPriority};

/// Format findings as a system-reminder injection block.
///
/// High/Critical findings become actionable instructions the agent must execute.
/// Low/Medium findings are informational context.
pub fn format_injection(findings: &[HealthFinding]) -> String {
    format_injection_with_prompts(findings, &HealthGuardPrompts::default())
}

/// Format findings using custom prompts (if set via config).
pub fn format_injection_with_prompts(
    findings: &[HealthFinding],
    prompts: &HealthGuardPrompts,
) -> String {
    let header = if prompts.header.is_empty() {
        "Memory maintenance required:"
    } else {
        &prompts.header
    };

    let mut out = format!("{}\n", header);

    for f in findings {
        out.push_str(&format!(
            "- [{}] {}: {} -> {}\n",
            f.priority, f.category, f.message, f.action
        ));
    }

    out
}

/// Format findings as JSON for the ai_suggestions MCP tool.
/// Only includes Low/Medium priority findings (informational suggestions).
pub fn format_suggestions(findings: &[HealthFinding]) -> Vec<serde_json::Value> {
    findings
        .iter()
        .filter(|f| f.priority == HealthPriority::Low || f.priority == HealthPriority::Medium)
        .map(|f| {
            serde_json::json!({
                "type": f.category,
                "priority": format!("{}", f.priority),
                "message": f.message,
                "action": f.action,
            })
        })
        .collect()
}
