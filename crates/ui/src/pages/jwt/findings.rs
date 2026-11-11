use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{BridgeState, JwtSeverity, JwtVulnerability};

use crate::pages::chrome::panel;

pub fn render_findings(state: &BridgeState, cx: &App) -> impl IntoElement {
    let findings = &state.jwt.vulnerabilities;
    if findings.is_empty() {
        return div().into_any_element();
    }

    panel(
        format!(
            "Vulnerability Findings — {} issue(s) detected",
            findings.len()
        ),
        None,
        div()
            .id("jwt-findings")
            .w_full()
            .max_h(px(150.))
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_1p5()
            .children(
                findings
                    .iter()
                    .map(|finding| finding_card(finding, cx))
                    .collect::<Vec<_>>(),
            ),
        cx,
    )
    .into_any_element()
}

pub fn finding_card(finding: &JwtVulnerability, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (border, icon, label) = match finding.severity {
        JwtSeverity::Critical => (hsla(0., 0.7, 0.55, 1.), PhosphorIcon::Warning, "CRITICAL"),
        JwtSeverity::High => (
            hsla(25. / 360., 0.9, 0.55, 1.),
            PhosphorIcon::WarningCircle,
            "HIGH",
        ),
        JwtSeverity::Medium => (
            hsla(45. / 360., 0.9, 0.5, 1.),
            PhosphorIcon::WarningCircle,
            "MEDIUM",
        ),
        JwtSeverity::Low => (hsla(215. / 360., 0.7, 0.55, 1.), PhosphorIcon::Info, "LOW"),
        JwtSeverity::Info => (theme.muted_foreground, PhosphorIcon::Info, "INFO"),
    };
    div()
        .w_full()
        .flex()
        .items_start()
        .gap_2()
        .p_2()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(border.opacity(0.35))
        .child(div().mt_0p5().text_color(border).child(icon))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .px_1()
                                .rounded(theme.radius_sm())
                                .bg(border)
                                .text_color(hsla(0., 0., 1., 1.))
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .child(label),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::NORMAL)
                                .child(finding.title.clone()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(finding.detail.clone()),
                ),
        )
        .into_any_element()
}
