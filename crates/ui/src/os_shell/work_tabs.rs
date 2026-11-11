//! Session work-tab strip in the title bar.

use std::sync::Arc;

use celestia_ui::components::composite::tab_bar::{TabBar, TabBarItem};
use celestia_ui::components::composite::window::WINDOW_TITLE_BAR_HEIGHT;
use celestia_ui::components::{Button, ButtonSize, ButtonVariant, PhosphorIcon};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, BridgeState, WorkData, WorkTab};

use crate::app::HaloApp;
use crate::app_icons;
use crate::components::confirm_destructive;
use crate::nav::Page;

const HOME_TAB_ID: &str = "halo-home";
/// Title-bar band height — keep the wrapper at the window chrome size.
const TITLE_BAR_HEIGHT: Pixels = WINDOW_TITLE_BAR_HEIGHT;
/// Shorter chips sit on the band bottom; breathing room stays above.
const TITLE_TAB_HEIGHT: Pixels = px(26.);
/// Icon size scaled to the title-bar tabs.
const TAB_ICON_SIZE: Pixels = px(13.);

pub(crate) fn render_work_tabs(
    state: &BridgeState,
    page: Page,
    weak: &WeakEntity<HaloApp>,
    scale: f32,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let active = if page == Page::Desktop {
        HOME_TAB_ID.to_string()
    } else {
        state
            .active_work_tab_id
            .clone()
            .unwrap_or_else(|| HOME_TAB_ID.to_string())
    };
    let session_tabs: Vec<WorkTab> = state.session_work_tabs().into_iter().cloned().collect();
    let tab_count = session_tabs.len();
    let has_session_tabs = tab_count > 0;
    let weak_close = weak.clone();
    let weak_select = weak.clone();
    let weak_home = weak.clone();

    let mut items = vec![
        TabBarItem::new(HOME_TAB_ID, "Halo")
            .closable(false)
            .leading(tab_icon(
                "tab-icon-home",
                app_icons::brand_at(app_icons::device_px(TAB_ICON_SIZE.into(), scale)),
                cx,
            ))
            .context_menu(move |menu, _, _| {
                let weak = weak_home.clone();
                menu.item(
                    PopupMenuItem::new("Close All Tabs")
                        .disabled(!has_session_tabs)
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::close_all_work_tabs(cx);
                            follow_after_close(&weak, cx);
                        }),
                )
            }),
    ];
    items.extend(session_tabs.iter().enumerate().map(|(ix, tab)| {
        let page = Page::from_work_kind(tab.kind);
        let tab_id = tab.id.clone();
        let weak_menu = weak.clone();
        let has_others = tab_count > 1;
        let has_left = ix > 0;
        let has_right = ix + 1 < tab_count;
        TabBarItem::new(tab.id.clone(), tab.title.clone())
            .closable(true)
            .dirty(work_tab_dirty(tab, state))
            .leading(tab_icon(
                format!("tab-icon-{}", tab.id),
                app_icons::icon_at(page, app_icons::device_px(TAB_ICON_SIZE.into(), scale)),
                cx,
            ))
            .context_menu(move |menu, _, _| {
                let close_id = tab_id.clone();
                let others_id = tab_id.clone();
                let left_id = tab_id.clone();
                let right_id = tab_id.clone();
                let weak_close = weak_menu.clone();
                let weak_others = weak_menu.clone();
                let weak_left = weak_menu.clone();
                let weak_right = weak_menu.clone();
                let weak_all = weak_menu.clone();
                menu.item(PopupMenuItem::new("Close").on_click(move |_, _, cx| {
                    let _ = Bridge::close_work_tab(&close_id, cx);
                    follow_after_close(&weak_close, cx);
                }))
                .item(
                    PopupMenuItem::new("Close Others")
                        .disabled(!has_others)
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::close_other_work_tabs(&others_id, cx);
                            follow_after_close(&weak_others, cx);
                        }),
                )
                .item(
                    PopupMenuItem::new("Close Left")
                        .disabled(!has_left)
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::close_work_tabs_to_left(&left_id, cx);
                            follow_after_close(&weak_left, cx);
                        }),
                )
                .item(
                    PopupMenuItem::new("Close Right")
                        .disabled(!has_right)
                        .on_click(move |_, _, cx| {
                            let _ = Bridge::close_work_tabs_to_right(&right_id, cx);
                            follow_after_close(&weak_right, cx);
                        }),
                )
                .separator()
                .item(PopupMenuItem::new("Close All").on_click(move |_, _, cx| {
                    let _ = Bridge::close_all_work_tabs(cx);
                    follow_after_close(&weak_all, cx);
                }))
                .separator()
                .item(
                    PopupMenuItem::new("Open in New Window").on_click(move |_, _, cx| {
                        HaloApp::open_in_new_window(page, cx);
                    }),
                )
            })
    }));

    div()
        .id("work-tab-strip")
        .aria_label(format!("{} work tabs", page.label()))
        .w_full()
        .h_full()
        .flex()
        .flex_row()
        .items_end()
        .gap_1()
        .pt_0()
        .pb_0()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h(TITLE_BAR_HEIGHT)
                .flex()
                .items_end()
                .overflow_hidden()
                .child(
                    TabBar::new("work-tabs")
                        .h(TITLE_TAB_HEIGHT)
                        .tab_height(TITLE_TAB_HEIGHT)
                        .pt_0()
                        .px_0()
                        .border_0()
                        .border_color(theme.border)
                        .active_border(theme.border)
                        .bg(theme.transparent)
                        .items_end()
                        .items(items)
                        .active(active)
                        .on_select({
                            let weak = weak_select.clone();
                            move |id, _, cx| {
                                if id.as_ref() == HOME_TAB_ID {
                                    if let Some(app) = weak.upgrade() {
                                        app.update(cx, |this, cx| {
                                            this.open_page(Page::Desktop, cx)
                                        });
                                    }
                                    return;
                                }
                                let _ = Bridge::select_work_tab(id, cx);
                                if let Some(app) = weak.upgrade() {
                                    app.update(cx, |this, cx| this.follow_active_work(cx));
                                }
                            }
                        })
                        .on_close({
                            let weak = weak_close.clone();
                            move |id, _, cx| {
                                if id.as_ref() == HOME_TAB_ID {
                                    return;
                                }
                                let _ = Bridge::close_work_tab(id, cx);
                                if let Some(app) = weak.upgrade() {
                                    app.update(cx, |this, cx| this.follow_active_work(cx));
                                }
                            }
                        }),
                ),
        )
        .child(
            div()
                .flex_none()
                .h(TITLE_BAR_HEIGHT)
                .flex()
                .items_center()
                .child(new_tab_button(weak)),
        )
        // Trailing slot: right-edge inset only. Run / Stop lives in the Coder
        // activity bar; keep a few px of inset — flush to 0 sits in the macOS
        // rounded-corner dead zone.
        .child(
            div()
                .flex_none()
                .h(TITLE_BAR_HEIGHT)
                .flex()
                .items_center()
                .justify_end()
                .pl_1()
                .pr(px(10.))
                .min_w(px(8.)),
        )
}

fn follow_after_close(weak: &WeakEntity<HaloApp>, cx: &mut App) {
    if let Some(app) = weak.upgrade() {
        app.update(cx, |this, cx| this.follow_active_work(cx));
    }
}

fn tab_icon(id: impl Into<ElementId>, image: Arc<Image>, cx: &App) -> impl IntoElement {
    div()
        .id(id)
        .size(TAB_ICON_SIZE)
        .flex_none()
        .overflow_hidden()
        .rounded(crate::pages::chrome::radius_xs(cx.theme()))
        .child(img(image).size_full().object_fit(ObjectFit::Cover))
}

fn work_tab_dirty(tab: &WorkTab, state: &BridgeState) -> bool {
    let live = state.active_work_tab_id.as_deref() == Some(tab.id.as_str());
    match &tab.data {
        WorkData::Intercept(_) => state
            .paused
            .iter()
            .any(|row| row.tab_id.as_deref() == Some(tab.id.as_str())),
        WorkData::Intruder(work) => {
            if live {
                state.intruder.running
            } else {
                work.status.running
            }
        }
        WorkData::Crawler(work) => {
            let status = if live {
                state
                    .crawler
                    .session
                    .as_ref()
                    .map(|session| session.status.as_str())
            } else {
                work.session.as_ref().map(|session| session.status.as_str())
            };
            matches!(status, Some("running" | "paused"))
        }
        _ => false,
    }
}

fn new_tab_button(weak: &WeakEntity<HaloApp>) -> impl IntoElement {
    let weak = weak.clone();
    Button::new("work-tab-add")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Plus)
        .tooltip("New tab")
        .dropdown_menu(move |menu, _, _| {
            let open = |page: Page, weak: WeakEntity<HaloApp>| {
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    if let Some(app) = weak.upgrade() {
                        app.update(cx, |this, cx| this.open_new_tab(page, cx));
                    }
                }
            };
            let open_window = |page: Page| {
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    HaloApp::open_in_new_window(page, cx);
                }
            };
            menu.item(PopupMenuItem::new("Core").disabled(true))
                .item(PopupMenuItem::new("HTTP").on_click(open(Page::HttpHistory, weak.clone())))
                .item(PopupMenuItem::new("Intercept").on_click(open(Page::Intercept, weak.clone())))
                .item(PopupMenuItem::new("Repeater").on_click(open(Page::Repeater, weak.clone())))
                .item(PopupMenuItem::new("Intruder").on_click(open(Page::Intruder, weak.clone())))
                .separator()
                .item(PopupMenuItem::new("Additional").disabled(true))
                .item(PopupMenuItem::new("Crawler").on_click(open(Page::Crawler, weak.clone())))
                .item(PopupMenuItem::new("Coder").on_click(open(Page::Coder, weak.clone())))
                .item(
                    PopupMenuItem::new("Source Control")
                        .on_click(open(Page::SourceControl, weak.clone())),
                )
                .item(PopupMenuItem::new("Agent").on_click(open(Page::Agent, weak.clone())))
                .item(PopupMenuItem::new("Notes").on_click(open(Page::Notes, weak.clone())))
                .item(PopupMenuItem::new("API Mock").on_click(open(Page::ApiMock, weak.clone())))
                .item(
                    PopupMenuItem::new("API Override")
                        .on_click(open(Page::ApiOverride, weak.clone())),
                )
                .item(PopupMenuItem::new("JWT").on_click(open(Page::Jwt, weak.clone())))
                .item(
                    PopupMenuItem::new("Port Scanner")
                        .on_click(open(Page::PortScanner, weak.clone())),
                )
                .item(
                    PopupMenuItem::new("File Explorer")
                        .on_click(open(Page::FileExplorer, weak.clone())),
                )
                .item(
                    PopupMenuItem::new("R2 Storage").on_click(open(Page::R2Storage, weak.clone())),
                )
                .item(PopupMenuItem::new("Wordlists").on_click(open(Page::Wordlists, weak.clone())))
                .item(
                    PopupMenuItem::new("Split View").on_click(open(Page::SplitView, weak.clone())),
                )
                .separator()
                .item(PopupMenuItem::new("New Window").on_click(open_window(Page::Desktop)))
                .item(
                    PopupMenuItem::new("HTTP in New Window")
                        .on_click(open_window(Page::HttpHistory)),
                )
                .item(
                    PopupMenuItem::new("Intercept in New Window")
                        .on_click(open_window(Page::Intercept)),
                )
                .item(
                    PopupMenuItem::new("Repeater in New Window")
                        .on_click(open_window(Page::Repeater)),
                )
                .item(
                    PopupMenuItem::new("Settings in New Window")
                        .on_click(open_window(Page::Settings)),
                )
        })
}

pub(crate) fn session_button(state: &BridgeState, weak: &WeakEntity<HaloApp>) -> impl IntoElement {
    let current = state
        .active_activity_session()
        .map(|session| session.title.clone())
        .unwrap_or_else(|| "Session".into());
    let sessions: Vec<(String, String, bool)> = state
        .activity_sessions
        .iter()
        .map(|session| {
            (
                session.id.clone(),
                session.title.clone(),
                state.active_activity_session_id.as_deref() == Some(session.id.as_str()),
            )
        })
        .collect();
    let weak = weak.clone();

    Button::new("activity-session")
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .leading_icon(PhosphorIcon::Folder)
        .label(current)
        .dropdown_caret(true)
        .tooltip("Session holds the open tabs and their activity")
        .dropdown_menu(move |menu, window, cx| {
            let mut menu = menu;
            for (id, title, active) in &sessions {
                let weak = weak.clone();
                let id = id.clone();
                let label = if *active {
                    format!("{title} · current")
                } else {
                    title.clone()
                };
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = Bridge::select_activity_session(&id, cx);
                    if let Some(app) = weak.upgrade() {
                        app.update(cx, |this, cx| this.follow_active_work(cx));
                    }
                }));
            }
            menu = menu.separator().item({
                let weak = weak.clone();
                PopupMenuItem::new("New session").on_click(move |_, _, cx| {
                    let _ = Bridge::create_activity_session(cx);
                    if let Some(app) = weak.upgrade() {
                        app.update(cx, |this, cx| this.follow_active_work(cx));
                    }
                })
            });
            if sessions.len() > 1 {
                let remove_list: Vec<(String, String)> = sessions
                    .iter()
                    .map(|(id, title, _)| (id.clone(), title.clone()))
                    .collect();
                let weak = weak.clone();
                menu = menu.separator().submenu(
                    "Remove session",
                    window,
                    cx,
                    move |sub, _, _| {
                        let mut sub = sub;
                        for (id, title) in &remove_list {
                            let id = id.clone();
                            let title = title.clone();
                            let weak = weak.clone();
                            sub = sub.item(
                                PopupMenuItem::new(title.clone())
                                    .icon(PhosphorIcon::Trash)
                                    .on_click(move |_, window, cx| {
                                        let id = id.clone();
                                        let title = title.clone();
                                        let weak = weak.clone();
                                        confirm_destructive(
                                            window,
                                            cx,
                                            "Remove session?",
                                            format!(
                                                "\"{title}\" and its open tabs will be removed. This cannot be undone."
                                            ),
                                            "Remove",
                                            move |_, cx| {
                                                let _ = Bridge::remove_activity_session(&id, cx);
                                                if let Some(app) = weak.upgrade() {
                                                    app.update(cx, |this, cx| {
                                                        this.follow_active_work(cx)
                                                    });
                                                }
                                            },
                                        );
                                    }),
                            );
                        }
                        sub
                    },
                );
            }
            menu
        })
}
