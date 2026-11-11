//! Transcript row tests.

use super::*;

use halo_bridge::HaloAgentMessage;

fn message(role: HaloAgentRole, parts: Vec<HaloAgentPart>) -> HaloAgentMessage {
    HaloAgentMessage {
        id: "m1".into(),
        role,
        parts,
        usage: None,
    }
}

/// The clipboard gets the answer, not the harness log: reasoning, tool
/// calls and attachments stay out, and separate text parts keep their
/// paragraph break.
#[test]
fn message_source_joins_text_parts_only() {
    let msg = message(
        HaloAgentRole::Assistant,
        vec![
            HaloAgentPart::Reasoning {
                text: "thinking".into(),
                expanded: false,
            },
            HaloAgentPart::Text("first  \n".into()),
            HaloAgentPart::ToolCall {
                id: "t1".into(),
                label: "Run tests".into(),
                detail: Some("ok".into()),
                expanded: false,
                is_error: false,
                done: true,
                sources: Vec::new(),
            },
            HaloAgentPart::Text("\n\nsecond".into()),
        ],
    );
    assert_eq!(message_source(&msg).as_deref(), Some("first\n\nsecond"));
}

/// Nothing to copy means no footer at all, so the row stays bare.
#[test]
fn message_source_is_none_without_visible_text() {
    let empty = message(HaloAgentRole::Assistant, vec![]);
    assert_eq!(message_source(&empty), None);

    let blank = message(
        HaloAgentRole::Assistant,
        vec![HaloAgentPart::Text("   ".into())],
    );
    assert_eq!(message_source(&blank), None);

    let system = message(
        HaloAgentRole::System,
        vec![HaloAgentPart::Text("hi".into())],
    );
    assert_eq!(message_source(&system), None);
}

/// A user turn copies its prompt, attachments excluded.
#[test]
fn message_source_covers_user_prompts() {
    let msg = message(
        HaloAgentRole::User,
        vec![
            HaloAgentPart::Attachments(vec!["/tmp/a.png".into()]),
            HaloAgentPart::Text("do the thing".into()),
        ],
    );
    assert_eq!(message_source(&msg).as_deref(), Some("do the thing"));
}
