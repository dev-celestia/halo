//! Harness tab strip, model chips, and hints shared by every harness panel.

use super::*;

pub(super) fn harness_tabs(
    harnesses: impl IntoIterator<Item = (String, String, bool)>,
    current: &str,
) -> impl IntoElement {
    Tabs::new("agent-harness-tabs").child(
        TabsList::new("agent-harness-tab-list")
            .size(TabsSize::Small)
            .children(harnesses.into_iter().map(|(id, label, available)| {
                let selected = id == current;
                let set_id = id.clone();
                TabsTrigger::new(SharedString::from(format!("agent-harness-tab-{id}")))
                    .label(label)
                    .selected(selected)
                    .disabled(!available)
                    .on_click(move |_, _, cx| {
                        let _ = Bridge::halo_agent_set_harness(&set_id, cx);
                    })
            })),
    )
}

pub(super) fn model_chips(
    models: impl IntoIterator<Item = (String, String)>,
    current_model: Option<&str>,
) -> impl IntoElement {
    let mut chips = div().flex().flex_wrap().gap_2();
    for (id, display_name) in models {
        let selected = current_model == Some(id.as_str());
        chips = chips.child(chip(
            SharedString::from(format!("coder-model-{id}")),
            display_name,
            selected,
            move |cx| {
                let _ = Bridge::halo_agent_set_model(&id, cx);
            },
        ));
    }
    chips
}

pub(super) fn harness_hint(harness: &str, installed: bool, setup_hint: Option<&str>) -> String {
    if installed {
        return match harness {
            "opencode" => {
                "OpenCode is installed and ready (connected via local OpenCode server).".into()
            }
            "antigravity" => "Antigravity ACP server is installed and ready.".into(),
            "cursor" => "Cursor SDK runtime is installed and ready.".into(),
            _ => "Pi 1.4.0 is installed and ready.".into(),
        };
    }
    setup_hint
        .map(str::to_string)
        .unwrap_or_else(|| match harness {
            "opencode" => {
                "OpenCode 2.2.0 or newer is required. Click Install below, or set a custom binary path."
                    .into()
            }
            "antigravity" => {
                "Antigravity ACP server is required. Click Install below, or set ANTIGRAVITY_ACP_EXECUTABLE."
                    .into()
            }
            "cursor" => {
                "Cursor requires Node.js on PATH and a Cursor API key."
                    .into()
            }
            _ => {
                "Pi 1.4.0 is required. Click Install below, or set a custom binary path."
                    .into()
            }
        })
}

pub(super) fn harness_tab_body(
    harness: &str,
    settings: &CoderSettings,
    editors: &ToolEditors,
    model_chips: impl IntoElement,
    current_model: Option<&str>,
    installed: bool,
    installing: Option<&str>,
    needs_upgrade: bool,
    setup_hint: Option<&str>,
    cx: &App,
) -> impl IntoElement {
    let hint = harness_hint(harness, installed, setup_hint);
    let body = match harness {
        "opencode" => {
            opencode_setup(settings, editors, installing, needs_upgrade, cx).into_any_element()
        }
        "antigravity" => antigravity_setup(settings, editors, installing, cx).into_any_element(),
        "cursor" => div().into_any_element(),
        _ => pi_setup(
            settings,
            editors,
            current_model,
            installing,
            needs_upgrade,
            cx,
        )
        .into_any_element(),
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(choice_block("Model", model_chips, Some(hint), cx))
        .child(body)
}
