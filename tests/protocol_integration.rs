use emergence::llm::StopReason;
use emergence::permissions::RiskLevel;
use emergence::protocol::{Action, Event};

/// Verifies that Action variants can be sent through a channel.
#[tokio::test]
async fn test_action_channel_roundtrip() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Action>();

    tx.send(Action::Submit("hello".into())).unwrap();
    let received = rx.recv().await.unwrap();
    assert!(matches!(received, Action::Submit(s) if s == "hello"));

    tx.send(Action::Quit).unwrap();
    assert!(matches!(rx.recv().await.unwrap(), Action::Quit));

    tx.send(Action::ApproveOnce).unwrap();
    assert!(matches!(rx.recv().await.unwrap(), Action::ApproveOnce));
}

/// Verifies that Event variants with payloads can be sent through a channel.
#[tokio::test]
async fn test_event_channel_roundtrip() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    tx.send(Event::TextDelta {
        content: "hi".into(),
        finish_reason: None,
    })
    .unwrap();
    assert!(matches!(rx.recv().await.unwrap(), Event::TextDelta { .. }));

    tx.send(Event::Error {
        message: "oops".into(),
    })
    .unwrap();
    assert!(matches!(rx.recv().await.unwrap(), Event::Error { .. }));

    tx.send(Event::AgentDone {
        stop_reason: StopReason::EndTurn,
    })
    .unwrap();
    assert!(matches!(rx.recv().await.unwrap(), Event::AgentDone { .. }));
}

/// Verifies that Action→Event flow simulates a submit→TextDelta→AgentDone lifecycle.
#[tokio::test]
async fn test_basic_lifecycle_channel_flow() {
    let (_action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Simulate: user submits → agent responds with TextDelta → AgentDone
    event_tx
        .send(Event::TextDelta {
            content: "response".into(),
            finish_reason: None,
        })
        .unwrap();
    event_tx
        .send(Event::AgentDone {
            stop_reason: StopReason::EndTurn,
        })
        .unwrap();
    drop(event_tx);

    let first = event_rx.recv().await.unwrap();
    assert!(matches!(first, Event::TextDelta { .. }));

    let second = event_rx.recv().await.unwrap();
    assert!(matches!(second, Event::AgentDone { .. }));

    assert!(event_rx.recv().await.is_none());
    drop(action_rx);
}

/// Verifies that the ToolRequest/ToolResult event pair carries all expected fields.
#[tokio::test]
async fn test_tool_request_result_pair() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    let params = serde_json::json!({"file_path": "/x", "content": "hello"});
    tx.send(Event::ToolRequest {
        id: "tc_1".into(),
        name: "write".into(),
        params: params.clone(),
        risk: RiskLevel::Write,
    })
    .unwrap();

    match rx.recv().await.unwrap() {
        Event::ToolRequest {
            id,
            name,
            params: p,
            risk,
        } => {
            assert_eq!(id, "tc_1");
            assert_eq!(name, "write");
            assert_eq!(p, params);
            assert_eq!(risk, RiskLevel::Write);
        }
        other => panic!("expected ToolRequest, got {:?}", other),
    }
}
