//! Provider-neutral conversation types. Each adapter maps these to its own
//! wire format, so the rest of Helpy never depends on a specific vendor.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Part {
    Text(String),
    /// Base64 image data.
    Image {
        media_type: String,
        data: String,
    },
    /// `signature` carries Gemini's thought signature, which must be sent back.
    ToolUse {
        id: String,
        name: String,
        input: Value,
        signature: Option<String>,
    },
    ToolResult {
        id: String,
        name: String,
        parts: Vec<Part>,
        is_error: bool,
    },
    /// An Anthropic block (thinking, redacted thinking) that must go back to
    /// Anthropic unchanged. Other providers never see it.
    AnthropicBlock(Value),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text(text.into())],
        }
    }

    pub fn has_images(&self) -> bool {
        self.parts.iter().any(|p| match p {
            Part::Image { .. } => true,
            Part::ToolResult { parts, .. } => parts.iter().any(|p| matches!(p, Part::Image { .. })),
            _ => false,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the input object.
    pub schema: Value,
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
    pub temperature: f64,
}

impl ChatRequest {
    /// A rough, deliberately generous token count for budget checks before a
    /// call: about 4 characters per token, a fixed cost per image, plus the
    /// full response allowance.
    pub fn estimated_tokens(&self) -> u64 {
        let mut chars = self.system.len();
        let mut images = 0u64;
        fn walk(parts: &[Part], chars: &mut usize, images: &mut u64) {
            for p in parts {
                match p {
                    Part::Text(t) => *chars += t.len(),
                    Part::Image { .. } => *images += 1,
                    Part::ToolUse { input, .. } => *chars += input.to_string().len(),
                    Part::ToolResult { parts, .. } => walk(parts, chars, images),
                    Part::AnthropicBlock(block) => *chars += block.to_string().len() / 4,
                }
            }
        }
        for m in &self.messages {
            walk(&m.parts, &mut chars, &mut images);
        }
        for t in &self.tools {
            chars += t.description.len() + t.schema.to_string().len();
        }
        chars as u64 / 4 + images * IMAGE_TOKEN_ESTIMATE + self.max_tokens as u64
    }
}

/// Screenshots are downscaled to at most 1568 px on the long edge, which is
/// roughly 1,800 tokens at one token per 28x28 patch; round up for safety.
pub const IMAGE_TOKEN_ESTIMATE: u64 = 2000;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other(String),
}

/// What one model call produced.
#[derive(Clone, Debug)]
pub struct Completion {
    /// The assistant turn, ready to append to the conversation.
    pub parts: Vec<Part>,
    pub usage: Usage,
    pub stop: StopReason,
    /// The model that actually answered (may differ after a server-side fallback).
    pub model: String,
}

impl Completion {
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| {
                if let Part::Text(t) = p {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn tool_uses(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.parts.iter().filter_map(|p| match p {
            Part::ToolUse {
                id, name, input, ..
            } => Some((id.as_str(), name.as_str(), input)),
            _ => None,
        })
    }
}

/// A model as a provider reports it.
#[derive(Clone, Debug, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelInfo {
    pub id: String,
    /// None when the provider doesn't say.
    pub vision: Option<bool>,
    pub tools: Option<bool>,
}
