//! Focused coder-agent transcript checks. GPU frame time is not measured here.
use std::time::Instant;

use gpui::{App, Entity, prelude::*, px, size};
use halo::ToolEditors;
use halo_bridge::{
    Bridge, HaloAgentMessage, HaloAgentPart, HaloAgentRole, HaloAgentSource, StubBackend, WorkData,
    WorkKind, WorkTab,
};
use markdown::mdast::Node;

fn code_nodes(source: &str) -> Vec<(Option<String>, String)> {
    let ast = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).expect("markdown");
    let mut found = Vec::new();
    fn walk(node: &Node, found: &mut Vec<(Option<String>, String)>) {
        if let Node::Code(code) = node {
            found.push((code.lang.clone(), code.value.clone()));
        }
        if let Some(children) = node.children() {
            for child in children {
                walk(child, found);
            }
        }
    }
    walk(&ast, &mut found);
    found
}

#[test]
fn markdown_fences_keep_source_and_row_identity() {
    let inline = code_nodes("before\n\n```rust\nfn answer() -> i32 { 42 }\n```\n\nafter");
    assert_eq!(inline.len(), 1);
    assert_eq!(inline[0].0.as_deref(), Some("rust"));
    assert!(inline[0].1.contains("fn answer"));

    let tilde = code_nodes("~~~\nplain\n~~~");
    assert_eq!(tilde.len(), 1);
    assert_eq!(tilde[0].1, "plain");

    let mut long = String::from("```\n");
    for line in 0..300 {
        long.push_str(&format!("line {line}\n"));
    }
    long.push_str("```\n");
    let parsed = code_nodes(&long);
    assert_eq!(parsed.len(), 1);
    assert!(parsed[0].1.lines().count() >= 300);
    let preview = halo::preview_end(&parsed[0].1);
    assert!(
        preview < parsed[0].1.len(),
        "preview {preview} source {}",
        parsed[0].1.len()
    );
    assert!(
        parsed[0].1.lines().nth(299).is_some(),
        "copy source keeps every line"
    );

    let unclosed = code_nodes("```js\nlet still = true;\n");
    assert_eq!(
        unclosed.len(),
        1,
        "an unclosed fence stays one code node: {unclosed:?}"
    );
    assert!(unclosed[0].1.contains("let still"));

    let old = vec!["a".into(), "b".into(), "c".into()];
    let appended = vec!["a".into(), "b".into(), "c".into(), "d".into()];
    assert_eq!(halo::row_splice_parts(&old, &appended), Some((3, 3, 1)));
    let replaced = vec!["a".into(), "B".into(), "c".into()];
    assert_eq!(halo::row_splice_parts(&old, &replaced), Some((1, 2, 1)));
    assert_eq!(halo::row_splice_parts(&old, &old), None);
}

struct Probe {
    editors: ToolEditors,
}

impl Probe {
    fn new(window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            editors: ToolEditors::new(window, cx),
        }
    }

    fn transcript(&self) -> Entity<halo::AgentTranscript> {
        self.editors.halo_agent_transcript.clone()
    }
}

impl gpui::Render for Probe {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let transcript = self.transcript();
        transcript.update(cx, |view, cx| view.mount(cx));
        gpui::div().size_full().child(transcript)
    }
}

fn transcript<R>(
    probe: &Entity<Probe>,
    cx: &mut gpui::VisualTestContext,
    read: impl FnOnce(&halo::AgentTranscript) -> R,
) -> R {
    cx.update(|_, app| read(probe.read(app).transcript().read(app)))
}

fn install(cx: &mut App) {
    celestia_ui::init(cx);
    halo::install_coder_services(cx);
    Bridge::install(StubBackend::new(), cx);
}

fn fill(count: usize, cx: &mut App) -> String {
    Bridge::state(cx).update(cx, |state| {
        state.coder.agent_ensure_session();
        state.coder.agent_messages.clear();
        for index in 0..count {
            let text = if index % 17 == 0 {
                format!("row {index}\n\n```rust\nfn row_{index}() {{}}\n```\n")
            } else {
                format!("row {index}")
            };
            state.coder.agent_messages.push(HaloAgentMessage {
                id: format!("m{index}"),
                role: HaloAgentRole::Assistant,
                parts: vec![HaloAgentPart::Text(text)],
                usage: None,
            });
        }
        let last = count - 1;
        state.coder.agent_messages.get_mut(last).unwrap().parts = vec![
            HaloAgentPart::Reasoning {
                text: "earlier thought".into(),
                expanded: false,
            },
            HaloAgentPart::Reasoning {
                text: "live thought".into(),
                expanded: true,
            },
            HaloAgentPart::Text("streaming".into()),
        ];
        state.coder.agent_running = true;
        state.coder.agent_streaming_part = Some((format!("m{last}"), 1));
        state.coder.agent_flush_active_session();
        let chat = state.coder.agent_active_session_id.clone();
        state.active_work_tab_id = Some("agent-tab".into());
        state.work_tabs = vec![WorkTab {
            id: "agent-tab".into(),
            title: "Agent".into(),
            kind: WorkKind::Agent,
            session_id: String::new(),
            data: WorkData::Agent(Default::default()),
        }];
        chat
    })
}

fn seed_expandable_trace(cx: &mut App) -> (String, String) {
    Bridge::state(cx).update(cx, |state| {
        state.coder.agent_ensure_session();
        state.coder.agent_messages.clear();
        state.coder.agent_messages.push(HaloAgentMessage {
            id: "m-expand".into(),
            role: HaloAgentRole::Assistant,
            parts: vec![
                HaloAgentPart::Reasoning {
                    text: "hidden thought".into(),
                    expanded: false,
                },
                HaloAgentPart::ToolCall {
                    id: "tool-1".into(),
                    label: "Run curl …".into(),
                    detail: Some("ok".into()),
                    expanded: false,
                    is_error: false,
                    done: true,
                    sources: Vec::new(),
                },
                HaloAgentPart::Text("done".into()),
            ],
            usage: None,
        });
        state.coder.agent_running = false;
        state.coder.agent_streaming_part = None;
        state.coder.agent_flush_active_session();
        state.active_work_tab_id = Some("agent-tab".into());
        state.work_tabs = vec![WorkTab {
            id: "agent-tab".into(),
            title: "Agent".into(),
            kind: WorkKind::Agent,
            session_id: String::new(),
            data: WorkData::Agent(Default::default()),
        }];
        ("m-expand".into(), "tool-1".into())
    })
}

#[gpui::test]
fn thought_row_expands_when_clicked(cx: &mut gpui::TestAppContext) {
    use gpui::{Modifiers, point};

    cx.update(install);
    let (message_id, _) = cx.update(|cx| seed_expandable_trace(cx));
    let (probe, cx) = cx.add_window_view(Probe::new);
    cx.simulate_resize(size(px(480.), px(320.)));
    cx.run_until_parked();

    let bounds = cx
        .debug_bounds("agent-thought-row")
        .expect("thought row was not laid out");
    eprintln!("thought row bounds={bounds:?}");
    assert!(
        cx.debug_bounds("agent-thought-body").is_none(),
        "collapsed accordion must not mount its panel"
    );
    // The trigger spans the whole row; click near its trailing edge.
    let pos = bounds.origin + point(bounds.size.width - px(6.), bounds.size.height / 2.);
    eprintln!("click={pos:?}");
    cx.simulate_click(pos, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 0)),
        Some(true),
        "clicking the thought row did not expand it"
    );
    let body = cx
        .debug_bounds("agent-thought-body")
        .expect("expanded accordion panel was not painted");
    eprintln!("thought body bounds={body:?}");
    assert!(
        body.size.height > px(8.),
        "expanded thought body has no height: {body:?}"
    );
}

#[gpui::test]
fn thought_and_tool_rows_expand_on_toggle(cx: &mut gpui::TestAppContext) {
    cx.update(install);
    let (message_id, tool_id) = cx.update(|cx| seed_expandable_trace(cx));
    let (probe, cx) = cx.add_window_view(Probe::new);
    cx.simulate_resize(size(px(480.), px(320.)));
    cx.run_until_parked();

    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 0)),
        Some(false)
    );
    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 1)),
        Some(false)
    );

    probe.update(cx, |probe, cx| {
        probe
            .transcript()
            .update(cx, |view, _| view.remeasured_rows = 0);
    });

    cx.update(|_, app| {
        Bridge::halo_agent_toggle_reasoning(&message_id, 0, app).unwrap();
    });
    cx.run_until_parked();

    let reasoning = cx.update(|_, app| {
        let state = Bridge::state(app).read(app);
        match &state.coder.agent_messages[0].parts[0] {
            HaloAgentPart::Reasoning { expanded, .. } => *expanded,
            other => panic!("expected reasoning, got {other:?}"),
        }
    });
    assert!(reasoning, "bridge reasoning expanded");
    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 0)),
        Some(true),
        "transcript kept collapsed after reasoning toggle"
    );
    let remeasured = transcript(&probe, cx, |view| view.remeasured_rows);
    assert!(
        remeasured >= 1,
        "reasoning toggle should remeasure the row, got {remeasured}"
    );

    probe.update(cx, |probe, cx| {
        probe
            .transcript()
            .update(cx, |view, _| view.remeasured_rows = 0);
    });
    cx.update(|_, app| {
        Bridge::halo_agent_toggle_tool_chip(&message_id, &tool_id, app).unwrap();
    });
    cx.run_until_parked();

    let tool = cx.update(|_, app| {
        let state = Bridge::state(app).read(app);
        match &state.coder.agent_messages[0].parts[1] {
            HaloAgentPart::ToolCall { expanded, .. } => *expanded,
            other => panic!("expected tool, got {other:?}"),
        }
    });
    assert!(tool, "bridge tool expanded");
    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 1)),
        Some(true),
        "transcript kept collapsed after tool toggle"
    );
    let remeasured = transcript(&probe, cx, |view| view.remeasured_rows);
    assert!(
        remeasured >= 1,
        "tool toggle should remeasure the row, got {remeasured}"
    );

    cx.update(|_, app| {
        Bridge::halo_agent_toggle_reasoning(&message_id, 0, app).unwrap();
        Bridge::halo_agent_toggle_tool_chip(&message_id, &tool_id, app).unwrap();
    });
    cx.run_until_parked();
    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 0)),
        Some(false)
    );
    assert_eq!(
        transcript(&probe, cx, |view| view.test_part_expanded(&message_id, 1)),
        Some(false)
    );
}

#[gpui::test]
fn virtualized_rows_stay_bounded_and_stable(cx: &mut gpui::TestAppContext) {
    cx.update(install);
    let chat = cx.update(|cx| fill(1_000, cx));
    let (probe, cx) = cx.add_window_view(Probe::new);
    cx.simulate_resize(size(px(480.), px(320.)));
    cx.run_until_parked();

    probe.update(cx, |probe, cx| {
        probe
            .transcript()
            .update(cx, |view, _| view.rendered_rows = 0);
    });
    cx.simulate_resize(size(px(500.), px(300.)));
    cx.run_until_parked();
    let rendered_1k = transcript(&probe, cx, |view| view.rendered_rows);
    eprintln!("agent panel 1_000 messages rendered_rows={rendered_1k}");
    assert!(rendered_1k > 0, "the viewport rendered no rows");
    assert!(
        rendered_1k < 400,
        "rendered {rendered_1k} rows for 1_000 messages"
    );

    let (entries, created, updated, _) = transcript(&probe, cx, |view| view.cache_stats());
    eprintln!("cache after first paint entries={entries} created={created} updated={updated}");
    assert!(entries <= 128);
    assert_eq!(created, entries);
    assert_eq!(updated, 0);
    probe.update(cx, |probe, cx| {
        probe.transcript().update(cx, |_, cx| cx.notify())
    });
    cx.run_until_parked();
    let (_, created_again, updated_again, _) = transcript(&probe, cx, |view| view.cache_stats());
    assert_eq!(created_again, created, "unchanged history reparsed");
    assert_eq!(updated_again, 0);

    let scroller = transcript(&probe, cx, |view| view.test_scroller());
    scroller.update(cx, |state, cx| assert!(state.scroll_to_item(40, cx)));
    cx.run_until_parked();
    let top = cx.update(|_, app| scroller.read(app).list_state().logical_scroll_top().item_ix);
    assert_eq!(top, 40);
    probe.update(cx, |probe, cx| {
        probe
            .transcript()
            .update(cx, |view, _| view.remeasured_rows = 0);
    });
    cx.update(|_, app| {
        Bridge::state(app).update(app, |state| {
            let text = match &mut state.coder.agent_messages.get_mut(40).unwrap().parts[0] {
                HaloAgentPart::Text(text) => text,
                other => panic!("expected text, got {other:?}"),
            };
            text.push_str(" token");
        });
    });
    cx.run_until_parked();
    let top = cx.update(|_, app| scroller.read(app).list_state().logical_scroll_top().item_ix);
    assert_eq!(top, 40);
    let remeasured = transcript(&probe, cx, |view| view.remeasured_rows);
    eprintln!("token update remeasured_rows={remeasured}");
    assert!(
        remeasured >= 1 && remeasured < 10,
        "remeasured {remeasured}"
    );
    let (_, _, updated_token, _) = transcript(&probe, cx, |view| view.cache_stats());
    assert_eq!(updated_token, 1, "one authoritative row updated");

    assert_eq!(
        transcript(&probe, cx, |view| view.active_part()),
        Some((format!("m999"), 1))
    );

    scroller.update(cx, |state, cx| state.scroll_to_end(cx));
    let (end, count) = cx.update(|_, app| {
        let list = scroller.read(app).list_state();
        (list.logical_scroll_top().item_ix, list.item_count())
    });
    eprintln!("jump-to-latest item={end} count={count}");
    assert!(count >= 1_000, "list count {count}");
    assert!(end + 20 >= count, "jump-to-latest at {end} of {count}");

    let started = Instant::now();
    cx.update(|_, cx| fill(10_000, cx));
    cx.run_until_parked();
    let elapsed = started.elapsed();
    probe.update(cx, |probe, cx| {
        probe
            .transcript()
            .update(cx, |view, _| view.rendered_rows = 0);
    });
    cx.simulate_resize(size(px(640.), px(360.)));
    cx.run_until_parked();
    let rendered_10k = transcript(&probe, cx, |view| view.rendered_rows);
    let (entries, _, _, evicted) = transcript(&probe, cx, |view| view.cache_stats());
    eprintln!(
        "agent panel 10_000 messages rendered_rows={rendered_10k} entries={entries} evicted={evicted} elapsed_ms={}",
        elapsed.as_millis()
    );
    assert!(
        rendered_10k > 0 && rendered_10k < 500,
        "rendered {rendered_10k} rows for 10_000 messages"
    );
    assert!(entries <= 128, "cache entries {entries}");
    assert!(
        elapsed.as_secs() < 5,
        "10_000-message sync took {elapsed:?}"
    );

    let pinned = probe.update(cx, |probe, cx| {
        probe
            .transcript()
            .update(cx, |view, cx| view.warm_text_cache(200, true, cx))
    });
    assert!(pinned);
    let (entries, _, _, evicted) = transcript(&probe, cx, |view| view.cache_stats());
    eprintln!("cache after warm entries={entries} evicted={evicted}");
    assert!(entries <= 128);
    assert!(evicted > 0);

    let source = "```rust\nfn copied() {}\n```\n".repeat(20);
    let range = probe.update(cx, |probe, cx| {
        probe.transcript().update(cx, |view, cx| {
            view.selection_covers_and_survives(&source, cx)
        })
    });
    assert_eq!(range, Some(0..source.len()));

    let other = cx.update(|_, app| {
        Bridge::state(app).update(app, |state| {
            state.coder.agent_new_chat();
            state.coder.agent_messages.push(HaloAgentMessage {
                id: "only".into(),
                role: HaloAgentRole::User,
                parts: vec![HaloAgentPart::Text("other chat".into())],
                usage: None,
            });
            state.coder.agent_flush_active_session();
            state.coder.agent_active_session_id.clone()
        })
    });
    cx.run_until_parked();
    let second = transcript(&probe, cx, |view| view.test_scroller());
    assert_ne!(scroller.entity_id(), second.entity_id());
    assert_eq!(transcript(&probe, cx, |view| view.test_chat_count()), 2);
    cx.update(|_, app| {
        Bridge::state(app).update(app, |state| {
            assert!(state.coder.agent_select_session(&chat));
        });
    });
    cx.run_until_parked();
    let restored = transcript(&probe, cx, |view| view.test_scroller());
    assert_eq!(restored.entity_id(), scroller.entity_id());
    assert_ne!(other, chat);
    let _ = other;
}

/// Overflowing transcript whose active turn ends in tool calls, so the work
/// group row folds them and the scrollbar hit listeners are live.
fn seed_work_group(running: bool, cx: &mut App) -> (String, String) {
    Bridge::state(cx).update(cx, |state| {
        state.coder.agent_ensure_session();
        state.coder.agent_messages.clear();
        for index in 0..40 {
            state.coder.agent_messages.push(HaloAgentMessage {
                id: format!("fill-u{index}"),
                role: HaloAgentRole::User,
                parts: vec![HaloAgentPart::Text(format!("prompt {index}"))],
                usage: None,
            });
            state.coder.agent_messages.push(HaloAgentMessage {
                id: format!("fill-a{index}"),
                role: HaloAgentRole::Assistant,
                parts: vec![HaloAgentPart::Text(format!("answer {index}"))],
                usage: None,
            });
        }
        state.coder.agent_messages.push(HaloAgentMessage {
            id: "m-work".into(),
            role: HaloAgentRole::Assistant,
            parts: vec![
                HaloAgentPart::ToolCall {
                    id: "tool-w".into(),
                    label: "Run cargo test".into(),
                    detail: if running { None } else { Some("ok".into()) },
                    expanded: false,
                    is_error: false,
                    done: !running,
                    sources: Vec::new(),
                },
                HaloAgentPart::Text("done".into()),
            ],
            usage: None,
        });
        state.coder.agent_running = running;
        state.coder.agent_streaming_part = None;
        state.coder.agent_flush_active_session();
        let session_id = state.coder.agent_active_session_id.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let session = state
            .coder
            .agent_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .expect("active session");
        if running {
            session.run_started_at = Some(now.saturating_sub(6_000));
            session.last_worked_secs = None;
        } else {
            session.run_started_at = None;
            session.last_worked_secs = Some(6);
        }
        state.active_work_tab_id = Some("agent-tab".into());
        state.work_tabs = vec![WorkTab {
            id: "agent-tab".into(),
            title: "Agent".into(),
            kind: WorkKind::Agent,
            session_id: String::new(),
            data: WorkData::Agent(Default::default()),
        }];
        ("m-work".into(), "tool-w".into())
    })
}

/// Click the work-group header (trailing edge, scrollbar listeners live) and
/// expect the folded process row to toggle. A live run opens the group itself;
/// a settled turn starts folded.
fn work_group_caret_toggles(running: bool, cx: &mut gpui::TestAppContext) {
    use gpui::{Modifiers, point};

    cx.update(install);
    let (message_id, _) = cx.update(|cx| seed_work_group(running, cx));
    let (probe, cx) = cx.add_window_view(Probe::new);
    cx.simulate_resize(size(px(480.), px(320.)));
    cx.run_until_parked();

    assert_eq!(
        transcript(&probe, cx, |view| view.test_fold_id()),
        Some(message_id.clone()),
        "active turn process did not fold under the work group"
    );

    // Bring the trailing group row into the viewport.
    let scroller = transcript(&probe, cx, |view| view.test_scroller());
    scroller.update(cx, |state, cx| state.scroll_to_end(cx));
    cx.run_until_parked();

    assert_eq!(
        cx.debug_bounds("coder-agent-work-row-0").is_some(),
        running,
        "work group expansion does not track the run state"
    );

    let click_header = |cx: &mut gpui::VisualTestContext| {
        let bounds = cx
            .debug_bounds("coder-agent-work-timer-header")
            .expect("work group header was not laid out");
        let pos = bounds.origin + point(bounds.size.width - px(6.), bounds.size.height / 2.);
        cx.simulate_click(pos, Modifiers::none());
        cx.run_until_parked();
    };

    click_header(cx);
    assert_eq!(
        cx.debug_bounds("coder-agent-work-row-0").is_some(),
        !running,
        "first header click did not toggle the process rows"
    );
    if !running {
        let chip = cx
            .debug_bounds("coder-agent-work-row-0")
            .expect("revealed process row has no bounds");
        assert!(
            chip.size.height > px(8.),
            "revealed process row has no height"
        );
    }

    click_header(cx);
    assert_eq!(
        cx.debug_bounds("coder-agent-work-row-0").is_some(),
        running,
        "second header click did not toggle the process rows back"
    );
}

#[gpui::test]
fn work_group_caret_toggles_settled_turn(cx: &mut gpui::TestAppContext) {
    work_group_caret_toggles(false, cx);
}

#[gpui::test]
fn work_group_caret_toggles_running_turn(cx: &mut gpui::TestAppContext) {
    work_group_caret_toggles(true, cx);
}

/// A live turn moves every step — thought and tool call — into the group and
/// opens it, so the run reads as one process list with the answer under it.
#[gpui::test]
fn live_work_group_lists_thoughts_and_tools(cx: &mut gpui::TestAppContext) {
    cx.update(install);
    cx.update(|cx| {
        Bridge::state(cx).update(cx, |state| {
            state.coder.agent_ensure_session();
            state.coder.agent_messages.clear();
            state.coder.agent_messages.push(HaloAgentMessage {
                id: "m-live".into(),
                role: HaloAgentRole::Assistant,
                parts: vec![
                    HaloAgentPart::Reasoning {
                        text: "reading the header\nand the body".into(),
                        expanded: false,
                    },
                    HaloAgentPart::ToolCall {
                        id: "tool-live".into(),
                        label: "Read file".into(),
                        detail: Some("trace.rs".into()),
                        expanded: false,
                        is_error: false,
                        done: true,
                        sources: Vec::new(),
                    },
                    HaloAgentPart::Text("answer".into()),
                ],
                usage: None,
            });
            state.coder.agent_running = true;
            state.coder.agent_streaming_part = Some(("m-live".into(), 2));
            state.coder.agent_flush_active_session();
            let session_id = state.coder.agent_active_session_id.clone();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let session = state
                .coder
                .agent_sessions
                .iter_mut()
                .find(|session| session.id == session_id)
                .expect("active session");
            session.run_started_at = Some(now.saturating_sub(6_000));
            session.last_worked_secs = None;
            state.active_work_tab_id = Some("agent-tab".into());
            state.work_tabs = vec![WorkTab {
                id: "agent-tab".into(),
                title: "Agent".into(),
                kind: WorkKind::Agent,
                session_id: String::new(),
                data: WorkData::Agent(Default::default()),
            }];
        });
    });
    let (probe, cx) = cx.add_window_view(Probe::new);
    cx.simulate_resize(size(px(480.), px(320.)));
    cx.run_until_parked();

    let scroller = transcript(&probe, cx, |view| view.test_scroller());
    scroller.update(cx, |state, cx| state.scroll_to_end(cx));
    cx.run_until_parked();

    assert_eq!(
        transcript(&probe, cx, |view| view.test_fold_id()),
        Some("m-live".into()),
        "live turn did not host the work group"
    );
    assert!(
        cx.debug_bounds("agent-thought-row").is_none(),
        "thought stayed inline instead of folding into the group"
    );
    let thought = cx
        .debug_bounds("coder-agent-work-row-0")
        .expect("thought step was not listed in the live group");
    let tool = cx
        .debug_bounds("coder-agent-work-row-1")
        .expect("tool step was not listed in the live group");
    assert!(
        thought.origin.y < tool.origin.y,
        "steps are not in part order: thought={thought:?} tool={tool:?}"
    );
}

#[gpui::test]
fn web_search_part_lists_its_sources(cx: &mut gpui::TestAppContext) {
    cx.update(install);
    cx.update(|cx| {
        Bridge::state(cx).update(cx, |state| {
            state.coder.agent_ensure_session();
            state.coder.agent_messages.clear();
            state.coder.agent_messages.push(HaloAgentMessage {
                id: "m-search".into(),
                role: HaloAgentRole::Assistant,
                parts: vec![
                    HaloAgentPart::ToolCall {
                        id: "tool-s".into(),
                        label: "Web search: tokio TcpStream".into(),
                        detail: Some(
                            "Title: TcpStream\nURL: https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html\n"
                                .into(),
                        ),
                        expanded: true,
                        is_error: false,
                        done: true,
                        sources: vec![
                            HaloAgentSource {
                                title: "TcpStream in tokio::net - Rust".into(),
                                url: "https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html"
                                    .into(),
                            },
                            HaloAgentSource {
                                title: "tokio-rs/tokio: A runtime for reliable applications".into(),
                                url: "https://github.com/tokio-rs/tokio".into(),
                            },
                        ],
                    },
                    HaloAgentPart::Text("done".into()),
                ],
                usage: None,
            });
            state.coder.agent_running = false;
            state.coder.agent_streaming_part = None;
            state.coder.agent_flush_active_session();
            state.active_work_tab_id = Some("agent-tab".into());
            state.work_tabs = vec![WorkTab {
                id: "agent-tab".into(),
                title: "Agent".into(),
                kind: WorkKind::Agent,
                session_id: String::new(),
                data: WorkData::Agent(Default::default()),
            }];
        });
    });
    let (_probe, cx) = cx.add_window_view(Probe::new);
    cx.simulate_resize(size(px(480.), px(320.)));
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("agent-source-row-0").is_some(),
        "first source row was not painted"
    );
    assert!(
        cx.debug_bounds("agent-source-row-1").is_some(),
        "second source row was not painted"
    );
}
