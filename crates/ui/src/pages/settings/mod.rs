//! Settings page — sidebar plus General / Appearance / Keyboard, the Coder
//! (IDE) sections, CA certificate, R2 credentials, and the Automation
//! placeholder.

mod appearance_tab;
mod ca_tab;
mod coder;
mod constants;
mod general_tab;
mod keyboard_tab;
mod placeholder_tab;
mod proxy_controls;
mod r2_tab;
mod sidebar;
mod source_control;
mod widgets;

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, BridgeState, SettingsTab};

use crate::pages::chrome::PAGE_GAP;
use crate::pages::editors::ToolEditors;

use appearance_tab::render_appearance_tab;
use ca_tab::render_ca_tab;
use coder::{
    render_agent_tab, render_editor_tab, render_language_servers_tab, render_terminal_tab,
    render_workspace_tab,
};
use general_tab::render_general_tab;
use keyboard_tab::render_keyboard_tab;
use placeholder_tab::render_placeholder_tab;
use r2_tab::render_r2_tab;
use sidebar::render_settings_sidebar;
use source_control::render as render_source_control_tab;

/// Reading width for a settings pane.
///
/// Rows put a label at the leading edge and its control at the trailing edge, so
/// an unbounded pane drifts them hundreds of pixels apart and runs descriptions
/// past a comfortable measure. The pane keeps the full width — only the column
/// of content stops growing.
const CONTENT_MAX_W: f32 = 760.;

pub fn render_settings(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let tab = state.settings_tab;

    div()
        .id("settings-page")
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(
            div().w_full().flex_1().min_h_0().overflow_hidden().child(
                ResizablePanelGroup::horizontal("settings-layout")
                    .with_handle(true)
                    .child(
                        resizable_panel()
                            .size(px(200.))
                            .size_range(px(160.)..px(320.))
                            .child(
                                div()
                                    .size_full()
                                    .overflow_hidden()
                                    .child(render_settings_sidebar(tab, cx)),
                            ),
                    )
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .min_w_0()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .when(tab.is_coder(), |panel| panel.child(reset_bar(cx)))
                                .child(
                                    div()
                                        .id("settings-content")
                                        .w_full()
                                        .flex_1()
                                        .min_h_0()
                                        // 24 inline matches the sidebar's text
                                        // inset (12 panel + 12 item padding), so
                                        // both columns share one edge.
                                        .px_6()
                                        .pt_5()
                                        .pb_8()
                                        .overflow_y_scrollbar()
                                        .child(
                                            div().w_full().max_w(px(CONTENT_MAX_W)).child(
                                                match tab {
                                                    SettingsTab::General => {
                                                        render_general_tab(state, editors, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::Appearance => {
                                                        render_appearance_tab(state, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::Keyboard => {
                                                        render_keyboard_tab(cx).into_any_element()
                                                    }
                                                    SettingsTab::CoderEditor => {
                                                        render_editor_tab(state, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::CoderTerminal => {
                                                        render_terminal_tab(state, editors, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::CoderLanguageServers => {
                                                        render_language_servers_tab(state, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::CoderSourceControl => {
                                                        render_source_control_tab(state, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::CoderWorkspace => {
                                                        render_workspace_tab(state, editors, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::Agent => {
                                                        render_agent_tab(state, editors, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::CaCert => {
                                                        render_ca_tab(state, cx).into_any_element()
                                                    }
                                                    SettingsTab::R2 => {
                                                        render_r2_tab(state, editors, cx)
                                                            .into_any_element()
                                                    }
                                                    SettingsTab::Automation => {
                                                        render_placeholder_tab(tab, cx)
                                                            .into_any_element()
                                                    }
                                                },
                                            ),
                                        ),
                                ),
                        ),
                    ),
            ),
        )
}

/// Toolbar above the Coder / Agent tabs.
///
/// Those six tabs all edit one shared `CoderSettings` blob, so a single reset
/// covers every one of them. It sits outside the scroll area so it stays put
/// while a long section scrolls underneath.
fn reset_bar(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let border = theme.border;

    div()
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        // Same 24px inline inset as the content below it, so the toolbar text
        // and the first section title share one left edge.
        .px_6()
        .pt_3()
        .pb_3()
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Shared by every Coder tab. Changes apply immediately."),
        )
        .child(
            Button::new("coder-settings-reset")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::ArrowCounterClockwise)
                .label("Reset to defaults")
                .on_click(|_, _, cx| Bridge::reset_coder_settings(cx)),
        )
}
