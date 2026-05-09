use super::*;
use reqwest::Client;
use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

pub struct OpenAIAdapter {
    base_url: String,
    api_key: String,
    models: Vec<ModelInfo>,
    client: Client,
}

impl OpenAIAdapter {
    pub fn new(base_url: String, api_key: String, models: Vec<ModelInfo>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            models,
            client: Client::new(),
        }
    }

    /// 构建请求体 JSON（公开以便测试）
    pub fn build_chat_request(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> String {
        // 将消息转换为 OpenAI JSON 格式：ContentPart::ToolUse → tool_calls
        let openai_messages: Vec<serde_json::Value> = messages.iter().map(|m| {
            let mut msg_json = serde_json::json!({
                "role": m.role,
                "content": m.content,
            });
            if let (Some(n), Some(tc_id)) = (&m.name, &m.tool_call_id) {
                msg_json["name"] = serde_json::json!(n);
                msg_json["tool_call_id"] = serde_json::json!(tc_id);
            }
            // Convert ContentPart::ToolUse to tool_calls
            if let Content::Parts(parts) = &m.content {
                let tool_uses: Vec<&crate::llm::ContentPart> = parts.iter()
                    .filter(|p| matches!(p, crate::llm::ContentPart::ToolUse { .. }))
                    .collect();
                if !tool_uses.is_empty() {
                    // Extract Text parts as reasoning_content (DeepSeek thinking mode)
                    let reasoning: String = parts.iter().filter_map(|p| {
                        if let crate::llm::ContentPart::Text { text } = p { Some(text.as_str()) } else { None }
                    }).collect::<Vec<_>>().join("");
                    if !reasoning.is_empty() {
                        msg_json["reasoning_content"] = serde_json::json!(reasoning);
                    }
                    msg_json["content"] = serde_json::Value::Null;
                    let tool_calls: Vec<serde_json::Value> = tool_uses.iter().map(|tu| {
                        if let crate::llm::ContentPart::ToolUse { id, name, input } = tu {
                            serde_json::json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(input).unwrap_or_default(),
                                }
                            })
                        } else {
                            serde_json::json!({})
                        }
                    }).collect();
                    msg_json["tool_calls"] = serde_json::json!(tool_calls);
                }
            }
            msg_json
        }).collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": openai_messages,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "top_p": config.top_p,
            "stream": true,
        });

        if !tools.is_empty() {
            let openai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::json!(openai_tools);
        }

        if !config.stop_sequences.is_empty() {
            body["stop"] = serde_json::json!(config.stop_sequences);
        }

        if let Some(thinking_tokens) = config.thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": thinking_tokens,
            });
        }

        body.to_string()
    }

    /// 解析 SSE 行，提取 StreamEvent
    fn parse_sse_line(line: &str) -> Option<anyhow::Result<StreamEvent>> {
        if !line.starts_with("data: ") {
            return None;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            return Some(Ok(StreamEvent::Finish {
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
            }));
        }

        let parsed: serde_json::Value = serde_json::from_str(data).ok()?;
        let choices = parsed["choices"].as_array()?;
        let choice = choices.first()?;
        let delta = &choice["delta"];

        // 优先检查 tool_calls
        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            let tc = &tool_calls[0];
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let func = &tc["function"];
            let name = func["name"].as_str().unwrap_or("").to_string();
            let args = func["arguments"].as_str().unwrap_or("").to_string();
            Some(Ok(StreamEvent::ToolCallDelta { id, name, arguments_json_fragment: args }))
        } else if let Some(content) = delta["content"].as_str() {
            if let Some(finish) = choice["finish_reason"].as_str() {
                let stop_reason = match finish {
                    "tool_calls" => StopReason::ToolUse,
                    "stop" => StopReason::EndTurn,
                    "length" => StopReason::MaxTokens,
                    _ => StopReason::EndTurn,
                };
                let usage = parsed.get("usage").map(|u| Usage {
                    input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                    output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                }).unwrap_or_default();
                Some(Ok(StreamEvent::Finish { stop_reason, usage }))
            } else {
                Some(Ok(StreamEvent::TextDelta(content.to_string())))
            }
        } else if let Some(thinking) = delta["reasoning_content"].as_str() {
            Some(Ok(StreamEvent::ThinkingDelta(thinking.to_string())))
        } else if choice["finish_reason"].as_str().is_some() {
            let usage = parsed.get("usage").map(|u| Usage {
                input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            }).unwrap_or_default();
            Some(Ok(StreamEvent::Finish {
                stop_reason: StopReason::EndTurn,
                usage,
            }))
        } else {
            None
        }
    }
}

#[async_trait]
impl Provider for OpenAIAdapter {
    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> anyhow::Result<ChatStream> {
        let body = self.build_chat_request(model, messages, tools, config);
        let url = format!("{}/chat/completions", self.base_url);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error ({}): {}", status.as_u16(), error_text);
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<StreamEvent>>(64);
        let byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();
            tokio::pin!(byte_stream);
            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim_end().to_string();
                            buffer = buffer[newline_pos + 1..].to_string();
                            if !line.is_empty() {
                                if let Some(event) = Self::parse_sse_line(&line) {
                                    if tx.send(event).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("stream error: {}", e))).await;
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn models(&self) -> &[ModelInfo] {
        &self.models
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adapter() -> OpenAIAdapter {
        OpenAIAdapter::new(
            "https://api.deepseek.com/v1".to_string(),
            "sk-test".to_string(),
            vec![ModelInfo {
                id: "deepseek-v4-pro".to_string(),
                name: "DeepSeek V4 Pro".to_string(),
                max_tokens: 128000,
            }],
        )
    }

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

    /// Verifies that all chat requests have `stream=true` set by default for SSE streaming.
    #[test]
    fn test_build_chat_request_sets_stream_true() {
        let adapter = make_adapter();
        let body = adapter.build_chat_request("m1", &[], &[], &config());
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["stream"], true);
    }

    /// Verifies that basic request fields (model, max_tokens, temperature, messages) are correctly serialized.
    #[test]
    fn test_build_chat_request_basic() {
        let adapter = make_adapter();
        let messages = vec![ChatMessage {
            role: Role::User,
            content: Content::Text("hello".to_string()),
            name: None, tool_call_id: None,
        }];
        let body = adapter.build_chat_request(
            "deepseek-v4-pro",
            &messages,
            &[],
            &GenerationConfig { max_tokens: 32000, temperature: 0.7, ..config() },
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["model"], "deepseek-v4-pro");
        assert_eq!(parsed["max_tokens"], 32000);
        assert_eq!(parsed["messages"][0]["role"], "user");
        assert_eq!(parsed["messages"][0]["content"], "hello");
    }

    /// Verifies that tool definitions are correctly serialized into OpenAI-compatible function-calling format.
    #[test]
    fn test_build_chat_request_with_tools() {
        let adapter = make_adapter();
        let tools = vec![ToolDefinition {
            name: "read".into(),
            description: "读取文件".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }];
        let body = adapter.build_chat_request(
            "m1", &[], &tools,
            &config(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let tool_arr = parsed["tools"].as_array().unwrap();
        assert_eq!(tool_arr.len(), 1);
        assert_eq!(tool_arr[0]["function"]["name"], "read");
    }

    /// Verifies that a text content delta in an SSE line is parsed into StreamEvent::TextDelta.
    #[test]
    fn test_parse_sse_text_delta() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"delta":{"content":"Hello"},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::TextDelta(text))) => assert_eq!(text, "Hello"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
    }

    /// Verifies that a finish event with usage data is parsed into StreamEvent::Finish with correct stop_reason and token counts.
    #[test]
    fn test_parse_sse_finish() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"finish_reason":"stop","delta":{},"index":0}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#
        );
        match event {
            Some(Ok(StreamEvent::Finish { stop_reason, usage })) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 5);
            }
            other => panic!("expected Finish, got {:?}", other),
        }
    }

    // ── parse_sse_line edge cases ──

    /// Verifies that the SSE [DONE] signal is parsed into StreamEvent::Finish with default EndTurn and zero usage.
    #[test]
    fn test_parse_sse_done_signal() {
        let event = OpenAIAdapter::parse_sse_line("data: [DONE]");
        match event {
            Some(Ok(StreamEvent::Finish { stop_reason, usage })) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 0);
                assert_eq!(usage.output_tokens, 0);
            }
            other => panic!("expected Finish from [DONE], got {:?}", other),
        }
    }

    /// Verifies that a tool_call delta in an SSE line is parsed into StreamEvent::ToolCallDelta with correct fields.
    #[test]
    fn test_parse_sse_tool_call_delta() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"tc_1","function":{"name":"read","arguments":"{\"p"}}]},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::ToolCallDelta { id, name, arguments_json_fragment })) => {
                assert_eq!(id, "tc_1");
                assert_eq!(name, "read");
                assert_eq!(arguments_json_fragment, "{\"p");
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }

    /// Verifies that a reasoning_content delta in an SSE line is parsed into StreamEvent::ThinkingDelta.
    #[test]
    fn test_parse_sse_thinking_delta() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"delta":{"reasoning_content":"Let me think..."},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::ThinkingDelta(text))) => assert_eq!(text, "Let me think..."),
            other => panic!("expected ThinkingDelta, got {:?}", other),
        }
    }

    /// Verifies that non-data SSE lines (e.g., "event: ping") are ignored by parse_sse_line.
    #[test]
    fn test_parse_sse_non_data_line_returns_none() {
        let event = OpenAIAdapter::parse_sse_line("event: ping");
        assert!(event.is_none());
    }

    /// Verifies that SSE data with an empty choices array is ignored (returns None).
    #[test]
    fn test_parse_sse_empty_choices_returns_none() {
        let event = OpenAIAdapter::parse_sse_line(r#"data: {"choices":[]}"#);
        assert!(event.is_none());
    }

    /// Verifies that malformed JSON in SSE data does not cause a panic and returns None.
    #[test]
    fn test_parse_sse_invalid_json_returns_none() {
        let event = OpenAIAdapter::parse_sse_line("data: not-json");
        assert!(event.is_none());
    }

    /// Verifies that finish_reason=tool_calls is mapped to StopReason::ToolUse.
    #[test]
    fn test_parse_sse_finish_reason_tool_calls() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"finish_reason":"tool_calls","delta":{"content":"done"},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::Finish { stop_reason, .. })) => {
                assert_eq!(stop_reason, StopReason::ToolUse);
            }
            other => panic!("expected Finish with ToolUse, got {:?}", other),
        }
    }

    /// Verifies that finish_reason=length is mapped to StopReason::MaxTokens.
    #[test]
    fn test_parse_sse_finish_reason_length() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"finish_reason":"length","delta":{"content":"done"},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::Finish { stop_reason, .. })) => {
                assert_eq!(stop_reason, StopReason::MaxTokens);
            }
            other => panic!("expected Finish with MaxTokens, got {:?}", other),
        }
    }

    // ── build_chat_request edge cases ──

    /// Verifies that stop sequences are serialized into the request body as a "stop" array.
    #[test]
    fn test_build_chat_request_with_stop_sequences() {
        let adapter = make_adapter();
        let body = adapter.build_chat_request(
            "m1", &[], &[],
            &GenerationConfig { stop_sequences: vec!["```".into(), "END".into()], ..config() },
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let stops = parsed["stop"].as_array().unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0], "```");
        assert_eq!(stops[1], "END");
    }

    /// Verifies that the thinking configuration is serialized into the request body with type and budget_tokens.
    #[test]
    fn test_build_chat_request_with_thinking() {
        let adapter = make_adapter();
        let body = adapter.build_chat_request(
            "m1", &[], &[],
            &GenerationConfig { thinking: Some(16000), ..config() },
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let thinking = &parsed["thinking"];
        assert_eq!(thinking["type"], "enabled");
        assert_eq!(thinking["budget_tokens"], 16000);
    }

    /// Verifies that an unrecognized finish_reason defaults to StopReason::EndTurn for graceful fallback.
    #[test]
    fn test_parse_sse_unknown_finish_reason_defaults_to_end_turn() {
        let event = OpenAIAdapter::parse_sse_line(
            r#"data: {"choices":[{"finish_reason":"some_unknown_reason","delta":{"content":"done"},"index":0}]}"#
        );
        match event {
            Some(Ok(StreamEvent::Finish { stop_reason, .. })) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
            }
            other => panic!("expected Finish with EndTurn, got {:?}", other),
        }
    }

    // ── OpenAIAdapter structural ──

    /// Verifies that the adapter returns the list of models it was initialized with.
    #[test]
    fn test_models() {
        let adapter = make_adapter();
        assert_eq!(adapter.models().len(), 1);
        assert_eq!(adapter.models()[0].id, "deepseek-v4-pro");
    }

    /// Verifies that trailing slashes in base_url are stripped during construction.
    #[test]
    fn test_new_trims_trailing_slash() {
        let adapter = OpenAIAdapter::new(
            "https://api.example.com/v1/".to_string(),
            "sk-test".to_string(),
            vec![],
        );
        assert_eq!(adapter.base_url, "https://api.example.com/v1");
    }
}
