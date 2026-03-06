//! Prompt Loader — loads LLM prompt templates from .toml files.
//!
//! Each model tier has its own prompts/ subdirectory with optimized templates.
//! Templates use `{placeholder}` syntax for runtime substitution.

use crate::config::LocalModelSize;
use crate::AiResult;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::RwLock;

/// Parsed prompt template from a .toml file.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptTemplate {
    pub meta: PromptMeta,
    pub template: PromptBody,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptMeta {
    pub version: u32,
    pub max_tokens: u32,
    pub description: String,
    /// Max chars for agent context injection (Step 2 importance scoring).
    /// Larger models can handle more context for better importance judgment.
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
}

fn default_max_context_chars() -> usize {
    500
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptBody {
    pub prompt: String,
}

/// Prompt names matching the .toml filenames (without extension).
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum PromptName {
    Extractor,
    ToolExtractor,
    MergeEvaluator,
    RelevanceGate,
    Coherence,
}

impl PromptName {
    fn filename(&self) -> &'static str {
        match self {
            Self::Extractor => "extractor.toml",
            Self::ToolExtractor => "toolextractor.toml",
            Self::MergeEvaluator => "merge_evaluator.toml",
            Self::RelevanceGate => "relevance_gate.toml",
            Self::Coherence => "coherence.toml",
        }
    }
}

/// Map model size to prompts directory name.
fn model_dir_name(model: &LocalModelSize) -> &'static str {
    match model {
        LocalModelSize::Phi4Mini => "Phi-4-Mini",
        LocalModelSize::SevenB => "Qwen-7B",
        LocalModelSize::Gemma12B => "Gemma-12B",
        LocalModelSize::Qwen14B => "Qwen-14B",
        LocalModelSize::Qwen32B => "Qwen-32B",
    }
}

/// Resolve the prompts base directory.
/// Searches: 1) next to executable, 2) project root (dev mode).
fn prompts_base_dir() -> Option<PathBuf> {
    // Dev mode: look relative to CARGO_MANIFEST_DIR or current dir
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest).join("prompts");
        if p.is_dir() {
            return Some(p);
        }
    }

    // Next to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("prompts");
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    // Current working directory
    let p = PathBuf::from("prompts");
    if p.is_dir() {
        return Some(p);
    }

    None
}

/// Global prompt cache: (model_dir, prompt_name) -> PromptTemplate
static PROMPT_CACHE: OnceLock<RwLock<HashMap<(String, PromptName), PromptTemplate>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<(String, PromptName), PromptTemplate>> {
    PROMPT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Load a prompt template for the given model and prompt name.
/// Caches the result for subsequent calls.
pub fn load_prompt(model: &LocalModelSize, name: PromptName) -> AiResult<PromptTemplate> {
    let dir_name = model_dir_name(model).to_string();
    let key = (dir_name.clone(), name);

    // Check cache
    if let Ok(cache) = cache().read() {
        if let Some(tmpl) = cache.get(&key) {
            return Ok(tmpl.clone());
        }
    }

    // Load from disk
    let base = prompts_base_dir().ok_or_else(|| {
        crate::AiError::InvalidInput("prompts/ directory not found".into())
    })?;

    let path = base.join(&dir_name).join(name.filename());
    let content = std::fs::read_to_string(&path).map_err(|e| {
        crate::AiError::InvalidInput(format!(
            "Failed to read prompt template {}: {}",
            path.display(),
            e
        ))
    })?;

    let tmpl: PromptTemplate = toml::from_str(&content).map_err(|e| {
        crate::AiError::InvalidInput(format!(
            "Failed to parse prompt template {}: {}",
            path.display(),
            e
        ))
    })?;

    tracing::info!(
        model = dir_name,
        prompt = name.filename(),
        version = tmpl.meta.version,
        "Prompt template loaded"
    );

    // Store in cache
    if let Ok(mut cache) = cache().write() {
        cache.insert(key, tmpl.clone());
    }

    Ok(tmpl)
}

/// Get the raw template string for a given model and prompt name.
/// Convenience wrapper around `load_prompt`.
pub fn get_template(model: &LocalModelSize, name: PromptName) -> AiResult<String> {
    Ok(load_prompt(model, name)?.template.prompt)
}

/// Get max_tokens for a given model and prompt name.
pub fn get_max_tokens(model: &LocalModelSize, name: PromptName) -> AiResult<u32> {
    Ok(load_prompt(model, name)?.meta.max_tokens)
}

/// Get max_context_chars for a given model and prompt name.
pub fn get_max_context_chars(model: &LocalModelSize, name: PromptName) -> AiResult<usize> {
    Ok(load_prompt(model, name)?.meta.max_context_chars)
}

/// Clear the prompt cache (useful when switching models at runtime).
pub fn clear_cache() {
    if let Ok(mut cache) = cache().write() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_dir_names() {
        assert_eq!(model_dir_name(&LocalModelSize::Phi4Mini), "Phi-4-Mini");
        assert_eq!(model_dir_name(&LocalModelSize::SevenB), "Qwen-7B");
        assert_eq!(model_dir_name(&LocalModelSize::Gemma12B), "Gemma-12B");
        assert_eq!(model_dir_name(&LocalModelSize::Qwen14B), "Qwen-14B");
        assert_eq!(model_dir_name(&LocalModelSize::Qwen32B), "Qwen-32B");
    }

    #[test]
    fn test_prompt_filenames() {
        assert_eq!(PromptName::Extractor.filename(), "extractor.toml");
        assert_eq!(PromptName::ToolExtractor.filename(), "toolextractor.toml");
        assert_eq!(PromptName::MergeEvaluator.filename(), "merge_evaluator.toml");
        assert_eq!(PromptName::RelevanceGate.filename(), "relevance_gate.toml");
    }

    #[test]
    fn test_load_phi4mini_extractor() {
        // This test works when run from the project root (cargo test)
        if let Ok(tmpl) = load_prompt(&LocalModelSize::Phi4Mini, PromptName::Extractor) {
            assert_eq!(tmpl.meta.version, 1);
            assert!(tmpl.template.prompt.contains("{content}"));
            assert!(tmpl.template.prompt.contains("{source_type}"));
        }
    }

    #[test]
    fn test_load_phi4mini_toolextractor() {
        if let Ok(tmpl) = load_prompt(&LocalModelSize::Phi4Mini, PromptName::ToolExtractor) {
            assert_eq!(tmpl.meta.version, 1);
            assert!(tmpl.template.prompt.contains("{content}"));
            assert!(tmpl.template.prompt.contains("{context_block}"));
        }
    }

    #[test]
    fn test_load_phi4mini_merge_evaluator() {
        if let Ok(tmpl) = load_prompt(&LocalModelSize::Phi4Mini, PromptName::MergeEvaluator) {
            assert_eq!(tmpl.meta.version, 1);
            assert!(tmpl.template.prompt.contains("{title_a}"));
            assert!(tmpl.template.prompt.contains("{title_b}"));
        }
    }

    #[test]
    fn test_load_phi4mini_relevance_gate() {
        if let Ok(tmpl) = load_prompt(&LocalModelSize::Phi4Mini, PromptName::RelevanceGate) {
            assert_eq!(tmpl.meta.version, 1);
            assert!(tmpl.template.prompt.contains("{message}"));
        }
    }

    #[test]
    fn test_cache_returns_same_instance() {
        clear_cache();
        if let Ok(t1) = load_prompt(&LocalModelSize::Phi4Mini, PromptName::Extractor) {
            let t2 = load_prompt(&LocalModelSize::Phi4Mini, PromptName::Extractor).unwrap();
            assert_eq!(t1.meta.version, t2.meta.version);
            assert_eq!(t1.template.prompt, t2.template.prompt);
        }
    }
}
