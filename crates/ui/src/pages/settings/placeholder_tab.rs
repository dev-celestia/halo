//! Placeholder panels for integrations not yet wired in the GPUI rewrite.

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::SettingsTab;

use crate::pages::chrome::section;

pub fn render_placeholder_tab(tab: SettingsTab, cx: &App) -> impl IntoElement {
    let (title, description, body) = match tab {
        SettingsTab::Automation => (
            "Automation Runtime",
            "Crawler runtime preferences.",
            "Open the Crawler tool tab to configure and run crawls. Advanced runtime prefs land here later.",
        ),
        _ => (
            tab.label(),
            "Additional settings.",
            "This section is not available yet.",
        ),
    };

    section(
        title,
        Some(description.into()),
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(body),
        cx,
    )
}
