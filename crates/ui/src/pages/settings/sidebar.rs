//! Categorized settings navigation (Application / Coder / Agent / Network /
//! Integrations / Automation).

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement;
use halo_bridge::{Bridge, SettingsTab};

struct NavSection {
    label: &'static str,
    items: &'static [SettingsTab],
}

const SECTIONS: &[NavSection] = &[
    NavSection {
        label: "Application",
        items: &[
            SettingsTab::General,
            SettingsTab::Appearance,
            SettingsTab::Keyboard,
        ],
    },
    NavSection {
        label: "Coder",
        items: &[
            SettingsTab::CoderEditor,
            SettingsTab::CoderTerminal,
            SettingsTab::CoderLanguageServers,
            SettingsTab::CoderSourceControl,
            SettingsTab::CoderWorkspace,
        ],
    },
    NavSection {
        label: "Agent",
        items: &[SettingsTab::Agent],
    },
    NavSection {
        label: "Network",
        items: &[SettingsTab::CaCert],
    },
    NavSection {
        label: "Integrations",
        items: &[SettingsTab::R2],
    },
    NavSection {
        label: "Automation",
        items: &[SettingsTab::Automation],
    },
];

pub fn render_settings_sidebar(active: SettingsTab, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    let mut nav = div()
        .id("settings-sidebar")
        .role(Role::Navigation)
        .aria_label("Settings sections")
        .size_full()
        .overflow_y_scrollbar()
        .flex()
        .flex_col()
        .gap_4()
        .p_3();

    for section in SECTIONS {
        let mut group = div().flex().flex_col().gap_0p5().child(
            div()
                .px_3()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_color(muted.opacity(0.7))
                .child(section.label.to_uppercase()),
        );

        for &tab in section.items {
            let is_active = tab == active;
            group = group.child(
                div()
                    .id(SharedString::from(format!("settings-nav-{}", tab.id())))
                    .role(Role::Button)
                    .tab_index(if is_active { 0 } else { -1 })
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded(theme.radius_sm())
                    .cursor_pointer()
                    .bg(if is_active {
                        theme.accent.opacity(0.12)
                    } else {
                        theme.background.opacity(0.)
                    })
                    .hover(|s| {
                        if is_active {
                            s
                        } else {
                            s.bg(theme.accent.opacity(0.06))
                        }
                    })
                    .on_click(move |_, _, cx| {
                        Bridge::set_settings_tab(tab, cx);
                    })
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(if is_active { theme.foreground } else { muted })
                            .child(tab.label()),
                    ),
            );
        }

        nav = nav.child(group);
    }

    nav
}
