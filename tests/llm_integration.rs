use emergence::llm::*;

// ── Message roundtrip through public API ──

#[test]
fn test_message_serialize_deserialize_roundtrip() {
    let original = ChatMessage {
        role: Role::User,
        content: Content::Text("hello world".into()),
        name: None,
        tool_call_id: None,
    };
    let json = serde_json::to_string(&original).unwrap();
    let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn test_message_with_tool_call_id_roundtrip() {
    let original = ChatMessage {
        role: Role::Tool,
        content: Content::Text("file contents".into()),
        name: Some("read_file".into()),
        tool_call_id: Some("tc_abc123".into()),
    };
    let json = serde_json::to_string(&original).unwrap();
    let parsed: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.role, Role::Tool);
    assert_eq!(parsed.name.as_deref(), Some("read_file"));
    assert_eq!(parsed.tool_call_id.as_deref(), Some("tc_abc123"));
}

// ── OpenAIAdapter through public API ──

fn make_adapter() -> OpenAIAdapter {
    OpenAIAdapter::new(
        "https://api.example.com/v1".into(),
        "sk-test-key".into(),
        vec![
            ModelInfo { id: "gpt-4".into(), name: "GPT-4".into(), max_tokens: 8192 },
            ModelInfo { id: "gpt-3.5".into(), name: "GPT-3.5".into(), max_tokens: 4096 },
        ],
    )
}

#[test]
fn test_openai_adapter_models() {
    let adapter = make_adapter();
    let models = adapter.models();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gpt-4");
    assert_eq!(models[1].id, "gpt-3.5");
}

#[test]
fn test_openai_adapter_build_request_includes_all_fields() {
    let adapter = make_adapter();
    let messages = vec![ChatMessage {
        role: Role::User,
        content: Content::Text("hi".into()),
        name: None,
        tool_call_id: None,
    }];
    let tools = vec![ToolDefinition {
        name: "read".into(),
        description: "read files".into(),
        parameters: serde_json::json!({"type": "object"}),
    }];
    let config = GenerationConfig {
        max_tokens: 2048,
        temperature: 0.5,
        top_p: 0.9,
        stop_sequences: vec!["END".into()],
        thinking: Some(8000),
        tools: None,
    };

    let body = adapter.build_chat_request("gpt-4", &messages, &tools, &config);
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(parsed["model"], "gpt-4");
    assert_eq!(parsed["max_tokens"], 2048);
    assert_eq!(parsed["temperature"], 0.5);
    assert_eq!(parsed["top_p"], 0.9);
    assert_eq!(parsed["stream"], true);
    assert_eq!(parsed["messages"][0]["role"], "user");
    assert_eq!(parsed["messages"][0]["content"], "hi");
    assert_eq!(parsed["stop"][0], "END");
    assert_eq!(parsed["thinking"]["type"], "enabled");
    assert_eq!(parsed["thinking"]["budget_tokens"], 8000);
    let tool_arr = parsed["tools"].as_array().unwrap();
    assert_eq!(tool_arr.len(), 1);
    assert_eq!(tool_arr[0]["function"]["name"], "read");
}

// ── ProviderRegistry with real adapter ──

#[test]
fn test_registry_with_real_adapter() {
    let mut registry = ProviderRegistry::new();
    registry.register("openai".into(), Box::new(make_adapter()));

    let provider = registry.get("openai").unwrap();
    assert_eq!(provider.models().len(), 2);
    assert_eq!(provider.models()[0].id, "gpt-4");
}

#[test]
fn test_registry_multiple_adapters() {
    let mut registry = ProviderRegistry::new();
    let adapter_a = OpenAIAdapter::new(
        "https://api.a.com/v1".into(), "key-a".into(),
        vec![ModelInfo { id: "model-a".into(), name: "A".into(), max_tokens: 100 }],
    );
    let adapter_b = OpenAIAdapter::new(
        "https://api.b.com/v1".into(), "key-b".into(),
        vec![ModelInfo { id: "model-b".into(), name: "B".into(), max_tokens: 200 }],
    );
    registry.register("provider-a".into(), Box::new(adapter_a));
    registry.register("provider-b".into(), Box::new(adapter_b));

    assert_eq!(registry.list_providers(), vec!["provider-a", "provider-b"]);
    assert_eq!(registry.get("provider-a").unwrap().models()[0].id, "model-a");
    assert_eq!(registry.get("provider-b").unwrap().models()[0].id, "model-b");
}

// ── StreamEvent enumeration ──

#[test]
fn test_stream_event_variants() {
    let text = StreamEvent::TextDelta("hello".into());
    let thinking = StreamEvent::ThinkingDelta("reasoning".into());
    let tool = StreamEvent::ToolCallDelta {
        id: "t1".into(),
        name: "read".into(),
        arguments_json_fragment: "{\"path".into(),
    };
    let finish = StreamEvent::Finish {
        stop_reason: StopReason::EndTurn,
        usage: Usage { input_tokens: 10, output_tokens: 5 },
    };

    // Verify Debug formatting doesn't panic
    assert!(format!("{:?}", text).contains("TextDelta"));
    assert!(format!("{:?}", thinking).contains("ThinkingDelta"));
    assert!(format!("{:?}", tool).contains("ToolCallDelta"));
    assert!(format!("{:?}", finish).contains("Finish"));
}

// ── GenerationConfig defaults via serde ──

#[test]
fn test_generation_config_partial_deserialize() {
    let json = r#"{"max_tokens":1000,"temperature":0.7}"#;
    let config: GenerationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.max_tokens, 1000);
    assert_eq!(config.temperature, 0.7);
    assert_eq!(config.top_p, 1.0); // default
    assert!(config.thinking.is_none());
    assert!(config.tools.is_none());
    assert!(config.stop_sequences.is_empty());
}
