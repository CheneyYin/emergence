use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Content,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// OpenAI-compatible: tool 消息需要 tool_call_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub max_tokens: u32,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

fn default_top_p() -> f64 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Role ──

    /// Verifies that serializing all Role variants produces the correct lowercase JSON string representation.
    #[test]
    fn test_role_serialize_lowercase() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), r#""system""#);
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), r#""user""#);
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            r#""assistant""#
        );
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), r#""tool""#);
    }

    /// Verifies that JSON deserialization of lowercase role strings correctly produces each Role variant.
    #[test]
    fn test_role_deserialize() {
        assert_eq!(
            serde_json::from_str::<Role>(r#""system""#).unwrap(),
            Role::System
        );
        assert_eq!(
            serde_json::from_str::<Role>(r#""user""#).unwrap(),
            Role::User
        );
        assert_eq!(
            serde_json::from_str::<Role>(r#""assistant""#).unwrap(),
            Role::Assistant
        );
        assert_eq!(
            serde_json::from_str::<Role>(r#""tool""#).unwrap(),
            Role::Tool
        );
    }

    // ── ChatMessage ──

    /// Verifies that a minimal ChatMessage serializes without optional fields `name` and `tool_call_id`.
    #[test]
    fn test_chat_message_serialize_minimal() {
        let msg = ChatMessage {
            role: Role::User,
            content: Content::Text("hello".into()),
            name: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["role"], "user");
        assert_eq!(parsed["content"], "hello");
        assert!(!parsed.as_object().unwrap().contains_key("name"));
        assert!(!parsed.as_object().unwrap().contains_key("tool_call_id"));
    }

    /// Verifies that ChatMessage serializes `name` and `tool_call_id` when present.
    #[test]
    fn test_chat_message_serialize_with_optional_fields() {
        let msg = ChatMessage {
            role: Role::Tool,
            content: Content::Text("result".into()),
            name: Some("read".into()),
            tool_call_id: Some("tc_1".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["role"], "tool");
        assert_eq!(parsed["content"], "result");
        assert_eq!(parsed["name"], "read");
        assert_eq!(parsed["tool_call_id"], "tc_1");
    }

    // ── Content (untagged) ──

    /// Verifies that Content::Text serializes as a plain JSON string rather than an object.
    #[test]
    fn test_content_text_serialize_as_plain_string() {
        let content = Content::Text("hello world".into());
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(json, r#""hello world""#);
    }

    /// Verifies that Content::Parts serializes as a JSON array with typed elements.
    #[test]
    fn test_content_parts_serialize_as_array() {
        let content = Content::Parts(vec![ContentPart::Text { text: "hi".into() }]);
        let json = serde_json::to_string(&content).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["type"], "text");
        assert_eq!(parsed[0]["text"], "hi");
    }

    /// Verifies that a plain JSON string deserializes into Content::Text.
    #[test]
    fn test_content_text_deserialize() {
        let content: Content = serde_json::from_str(r#""hello""#).unwrap();
        assert_eq!(content, Content::Text("hello".into()));
    }

    /// Verifies that a JSON array deserializes into Content::Parts with correct inner structure.
    #[test]
    fn test_content_parts_deserialize() {
        let json = r#"[{"type":"text","text":"hi"}]"#;
        let content: Content = serde_json::from_str(json).unwrap();
        match content {
            Content::Parts(parts) => {
                assert_eq!(parts.len(), 1);
            }
            other => panic!("expected Parts, got {:?}", other),
        }
    }

    // ── ContentPart ──

    /// Verifies that ContentPart::ToolUse serializes with the correct structure and snake_case type tag.
    #[test]
    fn test_content_part_tool_use_serialize() {
        let part = ContentPart::ToolUse {
            id: "t1".into(),
            name: "read".into(),
            input: serde_json::json!({"path": "/x"}),
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "tool_use");
        assert_eq!(parsed["id"], "t1");
        assert_eq!(parsed["name"], "read");
        assert_eq!(parsed["input"]["path"], "/x");
    }

    /// Verifies that ContentPart::ToolResult serializes `is_error` when set to Some(true).
    #[test]
    fn test_content_part_tool_result_with_is_error() {
        let part = ContentPart::ToolResult {
            tool_use_id: "t1".into(),
            content: "error msg".into(),
            is_error: Some(true),
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "tool_result");
        assert_eq!(parsed["tool_use_id"], "t1");
        assert_eq!(parsed["is_error"], true);
    }

    /// Verifies that `is_error` is omitted from serialization when None.
    #[test]
    fn test_content_part_tool_result_without_is_error() {
        let part = ContentPart::ToolResult {
            tool_use_id: "t1".into(),
            content: "ok".into(),
            is_error: None,
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(!json.contains("is_error"));
    }

    // ── StopReason ──

    /// Verifies that all StopReason variants serialize to their expected snake_case representation.
    #[test]
    fn test_stop_reason_snake_case() {
        assert_eq!(
            serde_json::to_string(&StopReason::EndTurn).unwrap(),
            r#""end_turn""#
        );
        assert_eq!(
            serde_json::to_string(&StopReason::MaxTokens).unwrap(),
            r#""max_tokens""#
        );
        assert_eq!(
            serde_json::to_string(&StopReason::ToolUse).unwrap(),
            r#""tool_use""#
        );
        assert_eq!(
            serde_json::to_string(&StopReason::StopSequence).unwrap(),
            r#""stop_sequence""#
        );
    }

    /// Verifies that snake_case JSON strings deserialize into the correct StopReason variant.
    #[test]
    fn test_stop_reason_deserialize() {
        assert_eq!(
            serde_json::from_str::<StopReason>(r#""end_turn""#).unwrap(),
            StopReason::EndTurn
        );
        assert_eq!(
            serde_json::from_str::<StopReason>(r#""max_tokens""#).unwrap(),
            StopReason::MaxTokens
        );
        assert_eq!(
            serde_json::from_str::<StopReason>(r#""tool_use""#).unwrap(),
            StopReason::ToolUse
        );
        assert_eq!(
            serde_json::from_str::<StopReason>(r#""stop_sequence""#).unwrap(),
            StopReason::StopSequence
        );
    }

    // ── Usage ──

    /// Verifies that Usage::default() initializes both input and output token counters to zero.
    #[test]
    fn test_usage_default_is_zero() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    /// Verifies that Usage serializes `input_tokens` and `output_tokens` correctly.
    #[test]
    fn test_usage_serialize() {
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 5,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["input_tokens"], 10);
        assert_eq!(parsed["output_tokens"], 5);
    }

    // ── GenerationConfig ──

    fn config() -> GenerationConfig {
        GenerationConfig {
            max_tokens: 100,
            temperature: 0.0,
            top_p: 1.0,
            stop_sequences: vec![],
            thinking: None,
            tools: None,
        }
    }

    /// Verifies that optional fields `thinking` and `tools` are omitted when set to None.
    #[test]
    fn test_generation_config_serialize_minimal() {
        let config = config();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["max_tokens"], 100);
        assert_eq!(parsed["temperature"], 0.0);
        assert!(!parsed.as_object().unwrap().contains_key("thinking"));
        assert!(!parsed.as_object().unwrap().contains_key("tools"));
    }

    /// Verifies that the thinking budget is serialized when `thinking` is set to Some.
    #[test]
    fn test_generation_config_with_thinking() {
        let config = GenerationConfig {
            thinking: Some(16000),
            ..config()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["thinking"], 16000);
    }

    /// Verifies that multiple stop sequences are serialized as a JSON array.
    #[test]
    fn test_generation_config_with_stop_sequences() {
        let config = GenerationConfig {
            stop_sequences: vec!["```".into(), "STOP".into()],
            ..config()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stops = parsed["stop_sequences"].as_array().unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0], "```");
    }

    /// Verifies that missing optional fields deserialize to their Rust defaults (temperature=0.0, top_p=1.0, thinking=None).
    #[test]
    fn test_generation_config_deserialize_defaults() {
        let json = r#"{"max_tokens":100}"#;
        let config: GenerationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_tokens, 100);
        assert_eq!(config.temperature, 0.0);
        assert_eq!(config.top_p, 1.0);
        assert!(config.thinking.is_none());
    }

    // ── ToolDefinition ──

    /// Verifies that ToolDefinition serializes name, description, and parameters correctly.
    #[test]
    fn test_tool_definition_serialize() {
        let tool = ToolDefinition {
            name: "read".into(),
            description: "read a file".into(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "read");
        assert_eq!(parsed["description"], "read a file");
        assert_eq!(parsed["parameters"]["type"], "object");
    }

    /// Verifies that ToolDefinition deserializes from JSON with all fields intact.
    #[test]
    fn test_tool_definition_deserialize() {
        let json = r#"{"name":"read","description":"read a file","parameters":{"type":"object"}}"#;
        let tool: ToolDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "read");
        assert_eq!(tool.description, "read a file");
        assert_eq!(tool.parameters["type"], "object");
    }

    // ── Deserialization roundtrips ──

    /// Verifies that a minimal ChatMessage JSON deserializes correctly with optional fields defaulting to None.
    #[test]
    fn test_chat_message_deserialize() {
        let json = r#"{"role":"assistant","content":"response"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content, Content::Text("response".into()));
        assert_eq!(msg.name, None);
        assert_eq!(msg.tool_call_id, None);
    }

    /// Verifies that ChatMessage deserializes all fields including `name` and `tool_call_id`.
    #[test]
    fn test_chat_message_deserialize_with_optional_fields() {
        let json = r#"{"role":"tool","content":"result","name":"read","tool_call_id":"tc_1"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg,
            ChatMessage {
                role: Role::Tool,
                content: Content::Text("result".into()),
                name: Some("read".into()),
                tool_call_id: Some("tc_1".into()),
            }
        );
    }

    /// Verifies that a ChatMessage with Content::Parts survives a full serialize-deserialize roundtrip unchanged.
    #[test]
    fn test_chat_message_full_roundtrip() {
        let original = ChatMessage {
            role: Role::Assistant,
            content: Content::Parts(vec![
                ContentPart::Text { text: "hi".into() },
                ContentPart::ToolUse {
                    id: "t1".into(),
                    name: "read".into(),
                    input: serde_json::json!({}),
                },
            ]),
            name: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let roundtripped: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(original, roundtripped);
    }

    /// Verifies that a tool_use JSON object deserializes into ContentPart::ToolUse with correct fields.
    #[test]
    fn test_content_part_tool_use_deserialize() {
        let json = r#"{"type":"tool_use","id":"t1","name":"read","input":{"path":"/x"}}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        assert_eq!(
            part,
            ContentPart::ToolUse {
                id: "t1".into(),
                name: "read".into(),
                input: serde_json::json!({"path": "/x"}),
            }
        );
    }

    /// Verifies that a tool_result JSON object deserializes into ContentPart::ToolResult with `is_error` defaulting to None.
    #[test]
    fn test_content_part_tool_result_deserialize() {
        let json = r#"{"type":"tool_result","tool_use_id":"t1","content":"done"}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        assert_eq!(
            part,
            ContentPart::ToolResult {
                tool_use_id: "t1".into(),
                content: "done".into(),
                is_error: None,
            }
        );
    }

    /// Verifies that a text-type JSON content part correctly deserializes into ContentPart::Text.
    #[test]
    fn test_content_part_text_deserialize() {
        let json = r#"{"type":"text","text":"hello"}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        assert_eq!(
            part,
            ContentPart::Text {
                text: "hello".into()
            }
        );
    }

    /// Verifies that ModelInfo serializes `id`, `name`, and `max_tokens` correctly.
    #[test]
    fn test_model_info_serialize() {
        let model = ModelInfo {
            id: "gpt-4".into(),
            name: "GPT-4".into(),
            max_tokens: 8192,
        };
        let json = serde_json::to_string(&model).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["id"], "gpt-4");
        assert_eq!(parsed["name"], "GPT-4");
        assert_eq!(parsed["max_tokens"], 8192);
    }

    /// Verifies that ModelInfo deserializes from JSON with all fields intact.
    #[test]
    fn test_model_info_deserialize() {
        let json = r#"{"id":"gpt-4","name":"GPT-4","max_tokens":8192}"#;
        let model: ModelInfo = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "gpt-4");
        assert_eq!(model.name, "GPT-4");
        assert_eq!(model.max_tokens, 8192);
    }

    /// Verifies that Usage deserializes `input_tokens` and `output_tokens` from JSON.
    #[test]
    fn test_usage_deserialize() {
        let json = r#"{"input_tokens":100,"output_tokens":50}"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    /// Verifies that GenerationConfig deserializes with tool definitions present in the JSON.
    #[test]
    fn test_generation_config_deserialize_with_tools() {
        let json =
            r#"{"max_tokens":200,"tools":[{"name":"read","description":"desc","parameters":{}}]}"#;
        let config: GenerationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_tokens, 200);
        assert!(config.tools.is_some());
        let tools = config.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read");
    }
}
