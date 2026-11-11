use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_destructive;
use crate::pages::chrome::panel;
use crate::pages::colors::{method_color, status_color_u16};

pub fn render_logs_strip(state: &BridgeState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let logs: Vec<_> = state.mock.logs.iter().take(8).cloned().collect();
    let total_logs = state.mock.logs.len();

    panel(
        "Recent Requests",
        None,
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children(if logs.is_empty() {
                vec![
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("No requests logged yet. Start the server and hit an endpoint.")
                        .into_any_element(),
                ]
            } else {
                logs.iter()
                    .map(|log| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(theme.radius_sm())
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(method_color(&log.method))
                                    .child(log.method.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .font_family("Menlo")
                                    .truncate()
                                    .child(log.path.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(status_color_u16(log.status_code))
                                    .child(log.status_code.to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{}ms", log.latency_ms)),
                            )
                            .into_any_element()
                    })
                    .collect()
            })
            .child(
                div().flex().justify_end().child(
                    Button::icon("mock-clear-logs-btn", PhosphorIcon::Trash)
                        .variant(ButtonVariant::Destructive)
                        .size(ButtonSize::Small)
                        .accessibility_label("Clear request logs")
                        .tooltip("Clear logs")
                        .disabled(total_logs == 0)
                        .on_click(move |_, window, cx| {
                            confirm_destructive(
                                window,
                                cx,
                                "Clear request logs?",
                                format!(
                                    "{total_logs} logged request(s) will be removed. This cannot be undone."
                                ),
                                "Clear",
                                |_, cx| {
                                    let _ = Bridge::mock_clear_logs(cx);
                                },
                            );
                        }),
                ),
            ),
        cx,
    )
}
