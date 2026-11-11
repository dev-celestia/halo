use halo_harness::pi::{PiDoneStatus, PiEvent, default_models};
use serde_json::json;

#[test]
fn test_default_models_catalog() {
    let models = default_models();
    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.id == "anthropic/claude-sonnet-5-5"));
    assert!(models.iter().any(|m| m.id == "openai/gpt-4o"));
    assert!(models.iter().any(|m| m.id == "google/gemini-2.5-pro"));
    assert!(!models.iter().any(|m| m.id.starts_with("antigravity/")));
    assert!(models.iter().any(|m| m.id == "deepseek/deepseek-flash"));
    assert!(models.iter().any(|m| m.id == "deepseek/deepseek-v4-pro"));
    assert!(models.iter().any(|m| m.id == "ollama"));
}

#[test]
fn test_migrate_retired_model_ids() {
    use halo_harness::pi::migrate_model_id;
    assert_eq!(
        migrate_model_id("anthropic/claude-3-7-sonnet"),
        "anthropic/claude-sonnet-5-5"
    );
    assert_eq!(
        migrate_model_id("deepseek/deepseek-v4-flash"),
        "deepseek/deepseek-flash"
    );
    assert_eq!(
        migrate_model_id("anthropic/claude-sonnet-5-5"),
        "anthropic/claude-sonnet-5-5"
    );
    assert_eq!(
        migrate_model_id("antigravity/gemini-3.8-flash"),
        "google/gemini-3.5-flash"
    );
    assert_eq!(
        migrate_model_id("antigravity/gemini-3.1-pro"),
        "google/gemini-2.5-pro"
    );
    assert_eq!(migrate_model_id("default"), "auto");
}

#[test]
fn test_pi_event_serialization() {
    let text_event = PiEvent::TextDelta {
        text: "Hello from Pi".into(),
    };
    let json = serde_json::to_string(&text_event).unwrap();
    assert!(json.contains("Hello from Pi"));

    let tool_event = PiEvent::ToolCall {
        id: "call-1".into(),
        name: "read".into(),
        args: json!({ "path": "src/main.rs" }),
    };
    let json_tool = serde_json::to_string(&tool_event).unwrap();
    assert!(json_tool.contains("src/main.rs"));
}

#[test]
fn test_pi_done_status() {
    let done = PiEvent::Done {
        status: PiDoneStatus::Completed,
        session_id: Some("session-123".into()),
        error: None,
    };
    let serialized = serde_json::to_string(&done).unwrap();
    let deserialized: PiEvent = serde_json::from_str(&serialized).unwrap();
    assert_eq!(done, deserialized);
}
