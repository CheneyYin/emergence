use crate::llm::StopReason;
use crate::permissions::RiskLevel;

/// TUI → Agent Loop
#[derive(Debug, Clone)]
pub enum Action {
    Submit(String),
    ApproveOnce,
    ApproveAlways,
    Deny,
    Cancel,
    Quit,
}

/// Agent Loop → TUI
#[derive(Debug, Clone)]
pub enum Event {
    TextDelta {
        content: String,
        finish_reason: Option<String>,
    },
    ToolRequest {
        id: String,
        name: String,
        params: serde_json::Value,
        risk: RiskLevel,
    },
    ToolResult {
        id: String,
        name: String,
        params: serde_json::Value,
        output: String,
        metadata: Option<serde_json::Value>,
    },
    ThinkingDelta {
        content: String,
    },
    StatusUpdate {
        tokens: u32,
        model: String,
    },
    AgentDone {
        stop_reason: StopReason,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that Action variants can be constructed.
    #[test]
    fn test_action_variants() {
        let submit = Action::Submit("hello".into());
        assert!(matches!(submit, Action::Submit(_)));
        assert!(matches!(Action::ApproveOnce, Action::ApproveOnce));
        assert!(matches!(Action::Quit, Action::Quit));
    }

    /// Verifies that Event variants with payloads can be constructed.
    #[test]
    fn test_event_variants() {
        let text = Event::TextDelta {
            content: "hi".into(),
            finish_reason: None,
        };
        assert!(matches!(text, Event::TextDelta { .. }));

        let tool_req = Event::ToolRequest {
            id: "t1".into(),
            name: "read".into(),
            params: serde_json::json!({}),
            risk: crate::permissions::RiskLevel::ReadOnly,
        };
        assert!(matches!(tool_req, Event::ToolRequest { .. }));

        let err = Event::Error {
            message: "oops".into(),
        };
        assert!(matches!(err, Event::Error { .. }));
    }
}
