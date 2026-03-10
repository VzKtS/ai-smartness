//! Anthropic Messages API client with streaming + tool_use loop.
//!
//! Uses ureq (blocking HTTP) with SSE streaming for real-time text output.
//! Handles the agentic tool_use loop: send → tool_use → execute → tool_result → send.

use crate::mcp::tools::ToolContext;
use crate::runtime::tool_executor;
use ai_smartness::AiResult;

use std::io::{BufRead, BufReader, Write};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 8192;
const MAX_TOOL_ROUNDS: usize = 25; // Safety limit on tool_use iterations

/// A single content block from the API response.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Parsed API response.
#[derive(Debug)]
pub struct ApiResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: String,
}

/// Run the agentic loop: send messages, handle tool_use, repeat until end_turn.
///
/// Returns the final concatenated text response.
pub fn agentic_loop(
    api_key: &str,
    model: &str,
    system: &str,
    initial_messages: Vec<serde_json::Value>,
    tools: &[serde_json::Value],
    ctx: &ToolContext,
) -> AiResult<String> {
    let mut messages = initial_messages;
    let mut full_text = String::new();

    for round in 0..MAX_TOOL_ROUNDS {
        tracing::debug!(round, messages_count = messages.len(), "API call");

        let response = call_streaming(api_key, model, system, &messages, tools)?;

        // Collect text from this response
        let mut round_text = String::new();
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();

        for block in &response.content {
            match block {
                ContentBlock::Text(t) => {
                    round_text.push_str(t);
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_uses.push((id.clone(), name.clone(), input.clone()));
                }
            }
        }

        full_text.push_str(&round_text);

        // If no tool_use → we're done
        if response.stop_reason != "tool_use" || tool_uses.is_empty() {
            break;
        }

        // Append assistant message with all content blocks
        let assistant_content: Vec<serde_json::Value> = response
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text(t) => serde_json::json!({
                    "type": "text",
                    "text": t,
                }),
                ContentBlock::ToolUse { id, name, input } => serde_json::json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input,
                }),
            })
            .collect();

        messages.push(serde_json::json!({
            "role": "assistant",
            "content": assistant_content,
        }));

        // Execute tools and build tool_result messages
        let mut tool_results: Vec<serde_json::Value> = Vec::new();
        for (tool_id, tool_name, tool_input) in &tool_uses {
            tracing::info!(tool = %tool_name, round, "Executing tool");

            let result = tool_executor::execute_tool(tool_name, tool_input, ctx);

            let (content, is_error) = match result {
                Ok(output) => (output.to_string(), false),
                Err(e) => (format!("Error: {}", e), true),
            };

            tool_results.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_id,
                "content": content,
                "is_error": is_error,
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": tool_results,
        }));
    }

    Ok(full_text)
}

/// Call the Anthropic Messages API with SSE streaming.
/// Streams text to stdout in real-time while accumulating the full response.
fn call_streaming(
    api_key: &str,
    model: &str,
    system: &str,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
) -> AiResult<ApiResponse> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .new_agent();

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system,
        "messages": messages,
        "stream": true,
    });

    // Only include tools if non-empty
    if !tools.is_empty() {
        body["tools"] = serde_json::json!(tools);
    }

    let resp = agent
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| ai_smartness::AiError::Provider(format!("Anthropic API error: {}", e)))?;

    parse_sse_stream(resp.into_body())
}

/// Parse SSE event stream from Anthropic API into content blocks.
fn parse_sse_stream(resp: ureq::Body) -> AiResult<ApiResponse> {
    let reader = BufReader::new(resp.into_reader());
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    let mut stop_reason = String::from("end_turn");

    // Track current block being built
    let mut current_block_index: Option<usize> = None;
    #[allow(unused_assignments)]
    let mut current_tool_id = String::new();
    #[allow(unused_assignments)]
    let mut current_tool_name = String::new();
    let mut current_tool_input_json = String::new();

    for line in reader.lines() {
        let line = line.map_err(|e| {
            ai_smartness::AiError::Provider(format!("SSE read error: {}", e))
        })?;

        // SSE format: "data: {json}" or empty lines
        let data = match line.strip_prefix("data: ") {
            Some(d) => d.trim(),
            None => continue,
        };

        if data == "[DONE]" {
            break;
        }

        let event: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = event["type"].as_str().unwrap_or("");

        match event_type {
            "content_block_start" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                current_block_index = Some(index);

                let block = &event["content_block"];
                let block_type = block["type"].as_str().unwrap_or("");

                match block_type {
                    "text" => {
                        content_blocks.push(ContentBlock::Text(String::new()));
                    }
                    "tool_use" => {
                        current_tool_id = block["id"].as_str().unwrap_or("").to_string();
                        current_tool_name = block["name"].as_str().unwrap_or("").to_string();
                        current_tool_input_json.clear();
                        content_blocks.push(ContentBlock::ToolUse {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            input: serde_json::json!({}),
                        });
                    }
                    _ => {}
                }
            }

            "content_block_delta" => {
                let delta = &event["delta"];
                let delta_type = delta["type"].as_str().unwrap_or("");

                match delta_type {
                    "text_delta" => {
                        if let Some(text) = delta["text"].as_str() {
                            // Stream to terminal in real-time
                            print!("{}", text);
                            std::io::stdout().flush().ok();

                            // Accumulate in content block
                            if let Some(idx) = current_block_index {
                                if let Some(ContentBlock::Text(ref mut t)) =
                                    content_blocks.get_mut(idx)
                                {
                                    t.push_str(text);
                                }
                            }
                        }
                    }
                    "input_json_delta" => {
                        if let Some(json_chunk) = delta["partial_json"].as_str() {
                            current_tool_input_json.push_str(json_chunk);
                        }
                    }
                    _ => {}
                }
            }

            "content_block_stop" => {
                // Finalize tool_use input JSON
                if let Some(idx) = current_block_index {
                    if let Some(ContentBlock::ToolUse { ref mut input, .. }) =
                        content_blocks.get_mut(idx)
                    {
                        if !current_tool_input_json.is_empty() {
                            if let Ok(parsed) =
                                serde_json::from_str(&current_tool_input_json)
                            {
                                *input = parsed;
                            }
                        }
                        current_tool_input_json.clear();
                    }
                }
                current_block_index = None;
            }

            "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    stop_reason = reason.to_string();
                }
            }

            "message_stop" => {
                break;
            }

            // Ignore: message_start, ping, etc.
            _ => {}
        }
    }

    Ok(ApiResponse {
        content: content_blocks,
        stop_reason,
    })
}

/// Non-streaming fallback (for testing or when streaming not desired).
#[allow(dead_code)]
pub fn call_blocking(
    api_key: &str,
    model: &str,
    system: &str,
    messages: &[serde_json::Value],
    tools: &[serde_json::Value],
) -> AiResult<ApiResponse> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .new_agent();

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system,
        "messages": messages,
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::json!(tools);
    }

    let mut resp = agent
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| ai_smartness::AiError::Provider(format!("Anthropic API: {}", e)))?;

    let json: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| ai_smartness::AiError::Provider(format!("Parse error: {}", e)))?;

    let mut content_blocks = Vec::new();
    if let Some(content) = json["content"].as_array() {
        for block in content {
            match block["type"].as_str() {
                Some("text") => {
                    let text = block["text"].as_str().unwrap_or("").to_string();
                    content_blocks.push(ContentBlock::Text(text));
                }
                Some("tool_use") => {
                    content_blocks.push(ContentBlock::ToolUse {
                        id: block["id"].as_str().unwrap_or("").to_string(),
                        name: block["name"].as_str().unwrap_or("").to_string(),
                        input: block["input"].clone(),
                    });
                }
                _ => {}
            }
        }
    }

    let stop_reason = json["stop_reason"]
        .as_str()
        .unwrap_or("end_turn")
        .to_string();

    Ok(ApiResponse {
        content: content_blocks,
        stop_reason,
    })
}
