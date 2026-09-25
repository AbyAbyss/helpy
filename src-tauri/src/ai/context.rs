//! Keeping a long conversation inside a model's memory. Agents and questions
//! both condense their older messages into a summary with these.

use super::types::{ChatRequest, Message, Part};
use crate::agents::runner::truncate;

/// Longest summary asked for.
pub const SUMMARY_TOKENS: u32 = 2000;

/// The messages as plain lines for summarizing: text, tool calls, and tool
/// results cut short.
pub fn transcript(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages {
        for p in &m.parts {
            let line = match p {
                Part::Text(t) => t.clone(),
                Part::ToolUse { name, input, .. } => format!("[used {name} with {input}]"),
                Part::ToolResult { name, parts, .. } => {
                    let text: Vec<_> = parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Text(t) => Some(truncate(t, 1500)),
                            _ => None,
                        })
                        .collect();
                    format!("[{name} returned: {}]", text.join(" "))
                }
                _ => continue,
            };
            out += &format!("{:?}: {line}\n", m.role);
        }
    }
    out
}

/// A request that condenses `transcript` following `instruction`.
pub fn summary_request(instruction: &str, transcript: String, max_response_tokens: u32) -> ChatRequest {
    ChatRequest {
        model: String::new(),
        system: instruction.into(),
        messages: vec![Message::user_text(transcript)],
        tools: Vec::new(),
        max_tokens: SUMMARY_TOKENS.min(max_response_tokens),
        temperature: 0.2,
    }
}
