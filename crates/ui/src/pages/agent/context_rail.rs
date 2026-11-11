//! Right-side Context rail: session facts, subagents, per-chat Skills & MCP,
//! artifacts, and the sources that will go into the next prompt.
//!
//! One rail replaces the old Artifacts rail; the subagent drawer still takes
//! this slot when a child transcript is open.

use std::path::Path;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::switch::Switch;
use celestia_ui::theme::ActiveTheme as _;
use chrono::{Local, TimeZone};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{
    Bridge, BridgeState, HaloAgentSession, HaloAgentSubagentStatus, McpTransport, SettingsTab,
    SkillInfo,
};

use crate::pages::editors::ToolEditors;

use super::artifacts::artifact_row;
use super::subagent::{spawn_cards, status_label};
use super::{CARD_TITLE_LINE, META_LINE_HEIGHT};

/// Width of the Context rail when open.
pub(super) const CONTEXT_RAIL_WIDTH: Pixels = px(300.);

pub(super) fn render_context_rail(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> AnyElement {
    if !state.coder.agent_context_open {
        return Empty.into_any_element();
    }

    let theme = cx.theme();
    let border = theme.border;
    let surface = theme.background;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;

    let session = state
        .coder
        .agent_sessions
        .iter()
        .find(|session| session.id == state.coder.agent_active_session_id);

    let mut body = div()
        .id("agent-context-rail-body")
        .w_full()
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .flex()
        .flex_col()
        .gap(px(16.))
        .px(px(12.))
        .py(px(12.));

    body = body.child(session_section(session, state, cx));
    body = body.child(subagents_section(session, state, cx));
    body = body.child(skills_mcp_section(state, cx));
    body = body.child(artifacts_section(state, editors, cx));
    body = body.child(sources_section(session, state, cx));

    celestia_ui::motion::fade_in(
        "coder-agent-context-rail-in",
        div()
            .id("coder-agent-context-rail")
            .h_full()
            .w(CONTEXT_RAIL_WIDTH)
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(border)
            .bg(surface)
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .h(px(46.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(border)
                    .child(
                        Phosphor::new(PhosphorIcon::SquaresFour)
                            .size(px(14.))
                            .color(muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .line_height(CARD_TITLE_LINE)
                            .font_weight(FontWeight::NORMAL)
                            .text_color(foreground)
                            .child("Context"),
                    )
                    .child(
                        Button::icon("coder-agent-context-refresh", PhosphorIcon::ArrowsClockwise)
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconSm)
                            .accessibility_label("Refresh skills")
                            .tooltip("Refresh skills")
                            .on_click(|_, _, cx| {
                                let _ = Bridge::halo_agent_refresh_skills(cx);
                            }),
                    )
                    .child(
                        Button::icon("coder-agent-context-close", PhosphorIcon::X)
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::IconSm)
                            .accessibility_label("Hide context")
                            .tooltip("Hide context")
                            .on_click(|_, _, cx| {
                                let _ = Bridge::halo_agent_toggle_context(cx);
                            }),
                    ),
            )
            .child(body),
    )
    .into_any_element()
}

/// Clickable section header with a caret — used by foldable Context rail groups.
fn foldable_section_label(
    id: &'static str,
    section: &'static str,
    label: &'static str,
    expanded: bool,
    trailing: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let section_key = section.to_string();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .id(id)
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let _ = Bridge::halo_agent_toggle_context_section(&section_key, cx);
                })
                .child(
                    Phosphor::new(if expanded {
                        PhosphorIcon::CaretDown
                    } else {
                        PhosphorIcon::CaretRight
                    })
                    .size(px(10.))
                    .color(muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .font_weight(FontWeight::NORMAL)
                        .text_color(muted)
                        .child(label),
                ),
        )
        .children(trailing)
        .into_any_element()
}

fn muted_note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .w_full()
        .px(px(2.))
        .py(px(2.))
        .text_xs()
        .line_height(META_LINE_HEIGHT)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

fn meta_row(label: &'static str, value: impl Into<SharedString>, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(muted)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_right()
                .text_xs()
                .line_height(META_LINE_HEIGHT)
                .text_color(foreground)
                .child(value.into()),
        )
        .into_any_element()
}

fn chip_label(text: impl Into<SharedString>, color: Hsla) -> AnyElement {
    div()
        .flex_none()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(4.))
        .bg(color.opacity(0.14))
        .text_xs()
        .font_weight(FontWeight::NORMAL)
        .text_color(color)
        .child(text.into())
        .into_any_element()
}

fn session_section(
    session: Option<&HaloAgentSession>,
    state: &BridgeState,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let card_bg = theme.popover;
    let border = theme.border;
    let muted = theme.muted_foreground;
    let radius = theme.radius_sm();
    let harness = state.coder.agent_harness.clone();
    let model = state.coder.agent_model.clone();

    let mut card = div()
        .w_full()
        .p(px(10.))
        .rounded(radius)
        .border_1()
        .border_color(border)
        .bg(card_bg)
        .flex()
        .flex_col()
        .gap(px(6.));

    if let Some(session) = session {
        let updated = session.updated_at.to_string();
        let stamp = Local
            .timestamp_millis_opt(session.updated_at as i64)
            .single()
            .map(|time| time.format("%b %-d, %H:%M").to_string())
            .unwrap_or(updated);
        let (status_label, status_color) = match session.status {
            halo_bridge::HaloAgentSessionStatus::Working => ("Working", theme.warning),
            halo_bridge::HaloAgentSessionStatus::Errored => ("Errored", theme.danger),
            halo_bridge::HaloAgentSessionStatus::Idle => ("Idle", theme.success),
        };
        let preview = session.preview.trim().to_string();
        let context_percent = session
            .context_usage
            .as_ref()
            .map(|usage| format!("{}%", usage.percent()));

        card = card
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_none()
                            .size(px(6.))
                            .rounded_full()
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .line_height(CARD_TITLE_LINE)
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.foreground)
                            .child(session.title_or_default().to_string()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(muted)
                            .child(status_label),
                    ),
            )
            .when(!preview.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(muted)
                        .line_clamp(2)
                        .child(preview),
                )
            })
            .child(meta_row("Updated", stamp, cx))
            .child(meta_row(
                "Harness",
                match harness.as_str() {
                    "opencode" => "OpenCode",
                    "antigravity" => "Antigravity",
                    "cursor" => "Cursor",
                    _ => "Pi",
                },
                cx,
            ));
        if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
            card = card.child(meta_row("Model", model, cx));
        }
        if let Some(percent) = context_percent {
            card = card.child(meta_row("Context", percent, cx));
        }
        if session.plan_mode {
            card = card.child(meta_row("Mode", "Plan", cx));
        }
    } else {
        card = card.child(muted_note("No session yet", cx));
    }

    let expanded = state.coder.agent_context_section_open("session");
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(foldable_section_label(
            "agent-context-session-toggle",
            "session",
            "SESSION",
            expanded,
            None,
            cx,
        ))
        .when(expanded, |el| el.child(card))
        .into_any_element()
}

fn subagents_section(
    session: Option<&HaloAgentSession>,
    state: &BridgeState,
    cx: &App,
) -> AnyElement {
    let children = session
        .map(|session| session.subagents.as_slice())
        .unwrap_or(&[]);
    let cards = spawn_cards(children, None);
    let theme = cx.theme();
    let accent = theme.accent;
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let success = theme.success;
    let warning = theme.warning;
    let expanded = state.coder.agent_context_section_open("subagents");

    let mut list = div().w_full().flex().flex_col().gap(px(2.));
    if cards.is_empty() {
        list = list.child(muted_note("No delegated agents", cx));
    } else {
        for card in &cards {
            let session_id = card.session_id.clone();
            let status_color = match card.status {
                HaloAgentSubagentStatus::Running => warning,
                HaloAgentSubagentStatus::Completed => success,
                HaloAgentSubagentStatus::Interrupted => muted,
                HaloAgentSubagentStatus::Errored => theme.danger,
            };
            list = list.child(
                div()
                    .id(SharedString::from(format!(
                        "agent-context-subagent-{}",
                        card.session_id
                    )))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(6.))
                    .py(px(5.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(accent))
                    .on_click(move |_, _, cx| {
                        let _ = Bridge::halo_agent_open_subagent(&session_id, cx);
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(px(6.))
                            .rounded_full()
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .line_height(META_LINE_HEIGHT)
                            .text_color(foreground)
                            .child(card.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(status_color)
                            .child(status_label(card.status)),
                    ),
            );
        }
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(foldable_section_label(
            "agent-context-subagents-toggle",
            "subagents",
            "SUBAGENTS",
            expanded,
            None,
            cx,
        ))
        .when(expanded, |el| el.child(list))
        .into_any_element()
}

fn skills_mcp_section(state: &BridgeState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let is_opencode = state.coder.agent_harness == "opencode";
    let enabled = state.coder.agent_enabled_skills.clone();
    let catalog: Vec<SkillInfo> = state.coder.agent_skills.clone();
    let loading = state.coder.agent_skills_loading;
    let error = state.coder.agent_skills_error.clone();

    let mut list = div().w_full().flex().flex_col().gap(px(2.)).pt(px(2.));
    if catalog.is_empty() {
        list = list.child(muted_note(
            if loading {
                "Loading skills…"
            } else {
                "No skills found for this project"
            },
            cx,
        ));
    } else {
        for skill in &catalog {
            let skill_id = skill.id.clone();
            let name = skill.name.clone();
            let scope = skill.scope.clone();
            let checked = enabled.iter().any(|id| id == &skill.id);
            let scope_color = match scope.as_str() {
                "project" => theme.success,
                "user" => theme.warning,
                _ => muted,
            };
            let tooltip = if skill.description.trim().is_empty() {
                skill.path.clone()
            } else {
                format!("{}\n{}", skill.description.trim(), skill.path)
            };
            list = list.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(2.))
                    .py(px(3.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_sm()
                                    .line_height(CARD_TITLE_LINE)
                                    .text_color(foreground)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_xs()
                                    .line_height(px(14.))
                                    .text_color(muted)
                                    .child(if skill.description.trim().is_empty() {
                                        skill.path.clone()
                                    } else {
                                        skill.description.clone()
                                    }),
                            ),
                    )
                    .child(chip_label(scope.to_uppercase(), scope_color))
                    .child(
                        Switch::new(ElementId::Name(SharedString::from(format!(
                            "agent-skill-{skill_id}"
                        ))))
                        .checked(checked)
                        .accessibility_label(format!("Enable skill {skill_id}"))
                        .tooltip(tooltip)
                        .on_change(move |next, _, cx| {
                            if *next != checked {
                                let _ = Bridge::halo_agent_toggle_skill(&skill_id, cx);
                            }
                        }),
                    ),
            );
        }
    }

    if let Some(error) = error {
        list = list.child(
            div()
                .w_full()
                .text_xs()
                .line_height(px(14.))
                .text_color(theme.danger)
                .child(error),
        );
    }

    let skills_expanded = state.coder.agent_context_section_open("skills");
    let mut section = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(foldable_section_label(
            "agent-context-skills-toggle",
            "skills",
            "SKILLS",
            skills_expanded,
            None,
            cx,
        ))
        .when(skills_expanded, |el| el.child(list));

    // MCP is an OpenCode capability; other harnesses run without it by design.
    let servers = state
        .coder_settings
        .mcp_servers
        .iter()
        .filter(|server| server.enabled && server.is_configured())
        .cloned()
        .collect::<Vec<_>>();
    let mut mcp_list = div().w_full().flex().flex_col().gap(px(2.));
    if !is_opencode {
        mcp_list = mcp_list.child(muted_note("MCP servers are available on OpenCode only", cx));
    } else if servers.is_empty() {
        mcp_list = mcp_list.child(muted_note("No MCP servers configured", cx));
    } else {
        let enabled_mcp = state.coder.agent_enabled_mcp.clone();
        for server in &servers {
            let name = server.name.clone();
            let checked = enabled_mcp.iter().any(|enabled| enabled == &name);
            let summary = server.summary();
            let transport = match server.transport {
                McpTransport::Stdio => "LOCAL",
                McpTransport::Remote => "REMOTE",
            };
            let transport_color = match server.transport {
                McpTransport::Stdio => theme.warning,
                McpTransport::Remote => theme.success,
            };
            let switch_name = name.clone();
            mcp_list = mcp_list.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(2.))
                    .py(px(3.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_sm()
                                    .line_height(CARD_TITLE_LINE)
                                    .text_color(foreground)
                                    .child(name.clone()),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_xs()
                                    .line_height(px(14.))
                                    .text_color(muted)
                                    .child(summary),
                            ),
                    )
                    .child(chip_label(transport, transport_color))
                    .child(
                        Switch::new(ElementId::Name(SharedString::from(format!(
                            "agent-mcp-{switch_name}"
                        ))))
                        .checked(checked)
                        .accessibility_label(format!("Enable MCP server {switch_name}"))
                        .on_change(move |next, _, cx| {
                            if *next != checked {
                                let _ = Bridge::halo_agent_toggle_mcp(&switch_name, cx);
                            }
                        }),
                    ),
            );
        }
    }

    let mcp_expanded = state.coder.agent_context_section_open("mcp");
    section = section
        .child(foldable_section_label(
            "agent-context-mcp-toggle",
            "mcp",
            "MCP",
            mcp_expanded,
            Some(
                Button::new("agent-mcp-manage")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .label("Manage")
                    .on_click(|_, _, cx| {
                        crate::windows::open_settings_window(SettingsTab::Agent, cx);
                    })
                    .into_any_element(),
            ),
            cx,
        ))
        .when(mcp_expanded, |el| el.child(mcp_list));

    section.into_any_element()
}

fn artifacts_section(state: &BridgeState, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let changes = &state.coder.agent_file_changes;
    let count = changes.len();
    let editors = editors.coder_editors.clone();
    let expanded = state.coder.agent_context_section_open("artifacts");
    let mut list = div().w_full().flex().flex_col().gap(px(2.));
    if changes.is_empty() {
        list = list.child(muted_note("No file changes yet", cx));
    } else {
        for change in changes {
            list = list.child(artifact_row(
                change,
                &editors,
                theme.foreground,
                theme.muted_foreground,
                theme.accent,
                theme.success,
                theme.warning,
                theme.radius_sm(),
            ));
        }
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(foldable_section_label(
            "agent-context-artifacts-toggle",
            "artifacts",
            "ARTIFACTS",
            expanded,
            (!changes.is_empty()).then(|| {
                div()
                    .flex_none()
                    .text_xs()
                    .line_height(META_LINE_HEIGHT)
                    .text_color(theme.muted_foreground)
                    .child(format!("{count}"))
                    .into_any_element()
            }),
            cx,
        ))
        .when(expanded, |el| el.child(list))
        .into_any_element()
}

fn sources_section(
    session: Option<&HaloAgentSession>,
    state: &BridgeState,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let foreground = theme.foreground;
    let muted = theme.muted_foreground;
    let attachments = session
        .map(|session| session.attachments.clone())
        .unwrap_or_default();
    let mentions = session
        .map(|session| mentioned_paths(&session.draft))
        .unwrap_or_default();

    let mut list = div().w_full().flex().flex_col().gap(px(2.));
    if attachments.is_empty() && mentions.is_empty() {
        list = list.child(muted_note("Nothing staged for the next prompt", cx));
    }
    for path in &attachments {
        let remove = path.clone();
        let name = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        list = list.child(
            div()
                .id(SharedString::from(format!("agent-source-{path}")))
                .w_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(2.))
                .py(px(3.))
                .child(
                    Phosphor::new(PhosphorIcon::Image)
                        .size(px(12.))
                        .color(muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(foreground)
                        .child(name),
                )
                .child(
                    Button::icon(
                        ElementId::Name(SharedString::from(format!(
                            "agent-source-remove-{remove}"
                        ))),
                        PhosphorIcon::X,
                    )
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .accessibility_label("Remove image")
                    .tooltip("Remove")
                    .on_click(move |_, _, cx| {
                        let _ = Bridge::halo_agent_remove_attachment(&remove, cx);
                    }),
                ),
        );
    }
    for path in &mentions {
        let name = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        list = list.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(2.))
                .py(px(3.))
                .child(
                    Phosphor::new(PhosphorIcon::FileCode)
                        .size(px(12.))
                        .color(muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .line_height(META_LINE_HEIGHT)
                        .text_color(foreground)
                        .child(name),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(muted)
                        .child(path.clone()),
                ),
        );
    }

    let expanded = state.coder.agent_context_section_open("sources");
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(foldable_section_label(
            "agent-context-sources-toggle",
            "sources",
            "SOURCES",
            expanded,
            None,
            cx,
        ))
        .when(expanded, |el| {
            el.child(list).child(
                Button::new("agent-context-add-images")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Small)
                    .leading_icon(PhosphorIcon::Plus)
                    .label("Add images")
                    .on_click(|_, _, cx| {
                        let _ = Bridge::halo_agent_attach_images(cx);
                    }),
            )
        })
        .into_any_element()
}

/// `@path` tokens in the composer draft, in order, without duplicates.
fn mentioned_paths(draft: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for token in draft.split_whitespace() {
        let Some(path) = token.strip_prefix('@') else {
            continue;
        };
        let path = path.trim_end_matches([',', ';', ')', ']', '}']);
        if path.is_empty() {
            continue;
        }
        if !seen.iter().any(|existing| existing == path) {
            seen.push(path.to_string());
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::mentioned_paths;

    #[test]
    fn mentions_dedupe_and_strip_punctuation() {
        assert_eq!(
            mentioned_paths("look @src/lib.rs and @src/lib.rs, then @docs/a.md)"),
            vec!["src/lib.rs".to_string(), "docs/a.md".to_string()]
        );
    }

    #[test]
    fn mentions_ignore_email_and_bare_at() {
        assert!(mentioned_paths("mail me at a@b.com or @").is_empty());
    }
}
