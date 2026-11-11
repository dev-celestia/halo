//! Top control toolbar: Start/Stop, progress gauge, clear results.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, IntruderDraft, IntruderPayloadType};

use crate::components::confirm_destructive;
use crate::pages::editors::ToolEditors;

pub fn render_intruder_toolbar(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let running = state.intruder.running;
    let result_count = state.intruder.results.len();
    let current = state.intruder.progress_current;
    let total = state.intruder.progress_total;
    let pct = if total > 0 {
        ((current as f32 / total as f32) * 100.0).round() as u32
    } else {
        0
    };
    let progress_label = if total > 0 {
        format!("{current} / {total} ({pct}%)")
    } else if running {
        "Starting…".into()
    } else {
        "Idle".into()
    };

    let method = editors.intruder_method.clone();
    let url = editors.intruder_url.clone();
    let headers = editors.intruder_headers.clone();
    let body = editors.intruder_body.clone();
    let payloads = editors.intruder_payloads.clone();
    let grep_keyword = editors.intruder_grep_keyword.clone();
    let grep_regex = editors.intruder_grep_regex.clone();
    let session_token = editors.intruder_session_token.clone();
    let session_header = editors.intruder_session_header.clone();

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("intruder-start")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Play)
                        .label("Start attack")
                        .disabled(running)
                        .on_click(move |_, _, cx| {
                            let cfg = Bridge::state(cx).read(cx).intruder.config.clone();
                            let draft = IntruderDraft {
                                method: method.read(cx).value().to_string(),
                                url: url.read(cx).value().to_string(),
                                headers_text: headers.read(cx).value().to_string(),
                                body: body.read(cx).value().to_string(),
                                payloads_text: payloads.read(cx).value().to_string(),
                                mode: cfg.mode,
                                payload_type: cfg.payload_type,
                                number_start: cfg.number_start,
                                number_end: cfg.number_end,
                                number_step: cfg.number_step,
                                number_format: cfg.number_format,
                                processing: cfg.processing,
                                concurrency: cfg.concurrency,
                                delay_ms: cfg.delay_ms,
                                delay_max_ms: cfg.delay_max_ms,
                                retries: cfg.retries,
                                grep_match_enabled: cfg.grep_match_enabled,
                                grep_keyword: grep_keyword.read(cx).value().to_string(),
                                grep_case_sensitive: cfg.grep_case_sensitive,
                                grep_extract_enabled: cfg.grep_extract_enabled,
                                grep_extract_regex: grep_regex.read(cx).value().to_string(),
                                session_enabled: cfg.session_enabled,
                                session_token_name: session_token.read(cx).value().to_string(),
                                session_header_name: session_header.read(cx).value().to_string(),
                            };
                            let _ = Bridge::start_intruder(draft, cx);
                        }),
                )
                .child(
                    Button::new("intruder-stop")
                        .variant(ButtonVariant::Secondary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Stop)
                        .label("Stop")
                        .disabled(!running)
                        .on_click(|_, _, cx| {
                            let _ = Bridge::stop_intruder(cx);
                        }),
                )
                .child(
                    Button::icon("intruder-clear", PhosphorIcon::Trash)
                        .variant(ButtonVariant::Destructive)
                        .size(ButtonSize::Small)
                        .accessibility_label("Clear results")
                        .tooltip("Remove all attack results from this run")
                        .disabled(running || result_count == 0)
                        .on_click(move |_, window, cx| {
                            confirm_destructive(
                                window,
                                cx,
                                "Clear results?",
                                format!(
                                    "{result_count} attack result(s) from this run will be removed. This cannot be undone."
                                ),
                                "Clear",
                                |_, cx| {
                                    let _ = Bridge::clear_intruder_results(cx);
                                },
                            );
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(muted)
                        .child(progress_label.clone()),
                )
                .child(div().text_xs().text_color(muted).child(mode_hint(
                    &state.intruder.config.mode,
                    state.intruder.config.payload_type,
                ))),
        )
        .child({
            let fill_w = if total > 0 {
                ((current as f32 / total as f32).clamp(0.0, 1.0) * 240.0) as f32
            } else if running {
                12.0
            } else {
                0.0
            };
            div()
                .id("intruder-progress")
                .role(Role::ProgressIndicator)
                .aria_label(progress_label.clone())
                .w_full()
                .h(px(4.))
                .rounded_full()
                .bg(theme.border)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(if running {
                            theme.accent
                        } else if total > 0 && current >= total {
                            theme.success
                        } else {
                            theme.border
                        })
                        .w(px(fill_w)),
                )
        })
}

fn mode_hint(mode: &halo_bridge::IntruderAttackMode, payload_type: IntruderPayloadType) -> String {
    format!("{} · {}", mode.label(), payload_type.label())
}
