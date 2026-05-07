use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

pub mod message;
pub mod registry;

pub use message::{
    ChatMessage, Content, ContentPart, GenerationConfig,
    ModelInfo, Role, StopReason, ToolDefinition, Usage,
};

pub use registry::ProviderRegistry;

/// LLM 流式事件
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallDelta {
        id: String,
        name: String,
        arguments_json_fragment: String,
    },
    Finish {
        stop_reason: StopReason,
        usage: Usage,
    },
}

/// 聊天流：Pin<Box<dyn Stream>>
pub type ChatStream = Pin<Box<dyn Stream<Item = anyhow::Result<StreamEvent>> + Send>>;

/// LLM Provider trait — 所有 provider 实现此接口
#[async_trait]
pub trait Provider: Send + Sync {
    /// 发起聊天请求，返回 SSE 流
    async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &GenerationConfig,
    ) -> anyhow::Result<ChatStream>;

    /// 返回该 provider 支持的模型列表
    fn models(&self) -> &[ModelInfo];
}
