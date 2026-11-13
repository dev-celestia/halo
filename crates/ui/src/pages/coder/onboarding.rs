//! Onboarding surface when Coder has no project folder open.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState, SettingsTab};

use crate::app_icons;
use crate::nav::Page;

use super::project_display_name;

/// Size of the app mark shown above the onboarding headline.
const ONBOARD_MARK: f32 = 64.;

/// Full-page welcome when starting fresh with no selected project.
pub(super) fn render_onboarding(state: &BridgeState, scale: f32, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let recent = &state.coder_projects;
    let mark_px = app_icons::device_px(ONBOARD_MARK, scale);

    div()
        .id("coder-onboarding")
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_5()
        .p_8()
        .child(
            div()
                .size(px(ONBOARD_MARK))
                .rounded_2xl()
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .border_1()
                .border_color(theme.border)
                .child(
                    img(app_icons::icon_at(Page::Coder, mark_px))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .max_w(px(420.))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child("No project open"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_center()
                        .text_color(theme.muted_foreground)
                        .child(
                            "Open a project folder to browse files, edit code, run terminals, and collaborate with Halo Agent.",
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("coder-onboard-open")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::FolderOpen)
                        .label("Open Folder…")
                        .on_click(|_, _, cx| {
                            let _ = Bridge::coder_open_folder(cx);
                        }),
                )
                .child(
                    Button::new("coder-onboard-settings")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .leading_icon(PhosphorIcon::Gear)
                        .label("Settings…")
                        .on_click(|_, _, cx| {
                            crate::windows::open_settings_window(SettingsTab::CoderEditor, cx);
                        }),
                ),
        )
        .child(if recent.is_empty() {
            div()
                .max_w(px(420.))
                .text_xs()
                .text_center()
                .text_color(theme.muted_foreground)
                .child("No recent projects yet.")
                .into_any_element()
        } else {
            recent_projects(recent, cx).into_any_element()
        })
}

fn recent_projects(projects: &[String], cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let mut list = div()
        .id("coder-onboard-recent")
        .w_full()
        .max_w(px(420.))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.muted_foreground)
                .child("Recent projects"),
        );

    for (idx, path) in projects.iter().take(6).enumerate() {
        let label = project_display_name(path);
        let path_owned = path.clone();
        let click_path = path_owned.clone();
        let hover_bg = theme.accent.opacity(0.12);
        let hover_border = theme.accent.opacity(0.35);
        let idle_border = theme.border.opacity(0.6);

        list = list.child(
            div()
                .id(SharedString::from(format!("coder-onboard-project-{idx}")))
                .w_full()
                .flex()
                .flex_col()
                .gap_0p5()
                .px_2()
                .py_1p5()
                .rounded(theme.radius_sm())
                .border_1()
                .border_color(idle_border)
                .bg(theme.background.opacity(0.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg).border_color(hover_border))
                .active(|s| s.bg(theme.accent.opacity(0.18)))
                .on_click(move |_, _, cx| {
                    let _ = Bridge::coder_open_project(&click_path, cx);
                })
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground)
                        .child(label),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(path_owned),
                ),
        );
    }

    list
}
