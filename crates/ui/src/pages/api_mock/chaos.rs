use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, MockChaosDto};

use crate::pages::editors::ToolEditors;

pub fn render_simulate_bar(state: &BridgeState, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let path_input = editors.mock_simulate_path.clone();
    let output = state
        .mock
        .simulate_output
        .clone()
        .unwrap_or_else(|| "Simulate a request path to preview the rendered template.".into());

    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Simulate:"),
        )
        .child(div().w(px(220.)).child(path_input.clone()))
        .child(
            Button::new("mock-simulate-btn")
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Small)
                .label("Preview")
                .on_click({
                    let input = path_input.clone();
                    move |_, _, cx| {
                        let path = input.read(cx).value().to_string();
                        Bridge::mock_simulate(&path, cx);
                    }
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px_2()
                .py_1()
                .rounded(theme.radius_sm())
                .bg(theme.muted.opacity(0.3))
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .font_family("Menlo")
                .truncate()
                .text_color(theme.foreground.opacity(0.85))
                .child(output),
        )
        .into_any_element()
}

pub fn render_chaos_section(state: &BridgeState, editors: &ToolEditors, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let chaos = state.mock.route_chaos();
    let mode = if chaos.latency_mode.is_empty() {
        "none".to_string()
    } else {
        chaos.latency_mode.clone()
    };

    let fixed = editors.mock_chaos_fixed.clone();
    let min = editors.mock_chaos_min.clone();
    let max = editors.mock_chaos_max.clone();
    let rate = editors.mock_chaos_rate.clone();
    let status = editors.mock_chaos_status.clone();

    fn mode_button(
        label: &'static str,
        value: &'static str,
        mode: &str,
        fixed: &Entity<gpui_component::input::InputState>,
        min: &Entity<gpui_component::input::InputState>,
        max: &Entity<gpui_component::input::InputState>,
        rate: &Entity<gpui_component::input::InputState>,
        status: &Entity<gpui_component::input::InputState>,
    ) -> Button {
        let selected = mode == value;
        Button::new(SharedString::from(format!("mock-chaos-{value}")))
            .variant(if selected {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            })
            .size(ButtonSize::Small)
            .label(label)
            .on_click({
                let fixed = fixed.clone();
                let min = min.clone();
                let max = max.clone();
                let rate = rate.clone();
                let status = status.clone();
                move |_, _, cx| {
                    let patch = chaos_from_inputs(&fixed, &min, &max, &rate, &status, cx);
                    Bridge::mock_set_chaos(
                        MockChaosDto {
                            latency_mode: value.to_string(),
                            ..patch
                        },
                        cx,
                    );
                }
            })
    }

    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.15))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Chaos:"),
        )
        .child(mode_button(
            "None", "none", &mode, &fixed, &min, &max, &rate, &status,
        ))
        .child(mode_button(
            "Fixed", "fixed", &mode, &fixed, &min, &max, &rate, &status,
        ))
        .child(mode_button(
            "Random", "random", &mode, &fixed, &min, &max, &rate, &status,
        ))
        .children(if mode == "fixed" {
            vec![labeled_input(
                "Latency ms:",
                editors.mock_chaos_fixed.clone(),
            )]
        } else {
            vec![]
        })
        .children(if mode == "random" {
            vec![
                labeled_input("Min ms:", editors.mock_chaos_min.clone()),
                labeled_input("Max ms:", editors.mock_chaos_max.clone()),
            ]
        } else {
            vec![]
        })
        .child(labeled_input(
            "Error rate:",
            editors.mock_chaos_rate.clone(),
        ))
        .child(labeled_input(
            "Error status:",
            editors.mock_chaos_status.clone(),
        ))
        .into_any_element()
}

pub fn labeled_input(
    label: &'static str,
    input: Entity<gpui_component::input::InputState>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .child(div().text_xs().child(label))
        .child(div().w(px(64.)).child(input))
        .into_any_element()
}

pub fn chaos_from_inputs(
    fixed: &Entity<gpui_component::input::InputState>,
    min: &Entity<gpui_component::input::InputState>,
    max: &Entity<gpui_component::input::InputState>,
    rate: &Entity<gpui_component::input::InputState>,
    status: &Entity<gpui_component::input::InputState>,
    cx: &App,
) -> MockChaosDto {
    MockChaosDto {
        latency_mode: String::new(),
        latency_fixed: fixed.read(cx).value().parse().ok(),
        latency_min: min.read(cx).value().parse().ok(),
        latency_max: max.read(cx).value().parse().ok(),
        error_rate_bp: rate
            .read(cx)
            .value()
            .parse::<f64>()
            .ok()
            .map(|rate| (rate * 10_000.0).round() as u32),
        error_status: status.read(cx).value().parse().ok(),
    }
}
