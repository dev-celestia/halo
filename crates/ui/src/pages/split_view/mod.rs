//! Split View — 2/3/4 pane workspace hosting other Halo tools.

use celestia_ui::components::Tooltip;
use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::image_icon::{ImageIcon, ImageIconShape, ImageIconSize};
use celestia_ui::components::primitive::resizable::{ResizablePanelGroup, resizable_panel};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{Bridge, BridgeState, SplitLayoutKind};

use crate::app_icons;
use crate::nav::Page;
use crate::pages::editors::ToolEditors;
use crate::pages::{
    render_agent, render_api_mock, render_api_override, render_coder, render_comparer,
    render_crawler, render_diagram, render_doctor, render_file_explorer, render_http,
    render_intercept, render_intruder, render_jwt, render_notes, render_port_scanner,
    render_r2_storage, render_repeater, render_settings, render_source_control, render_wordlists,
};

/// Mark size of the slot header icon (ImageIconSize::Sm draws a 20px box).
const SLOT_MARK: f32 = 20.;
/// Mark size of an app-pick tile.
const PICK_MARK: f32 = 32.;

pub fn render_split_view(
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let sv = &state.split_view;
    div()
        .id("split-view-page")
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(render_toolbar(sv.layout, sv.filled_count(), cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_hidden()
                .child(render_layout(
                    sv.layout, &sv.slots, state, editors, window, cx,
                )),
        )
}

fn render_toolbar(layout: SplitLayoutKind, filled: usize, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .id("split-view-toolbar")
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
        .px_2()
        .py_1p5()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.35))
        .child(div().flex().flex_row().items_center().gap_1().children(
            SplitLayoutKind::ALL.iter().map(|kind| {
                let selected = *kind == layout;
                Button::new(SharedString::from(format!("split-layout-{}", kind.id())))
                    .variant(if selected {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    })
                    .size(ButtonSize::Small)
                    .label(kind.label())
                    .tooltip(kind.title())
                    .on_click({
                        let kind = *kind;
                        move |_, _, cx| {
                            let _ = Bridge::split_view_set_layout(kind, cx);
                        }
                    })
                    .into_any_element()
            }),
        ))
        .child(
            Button::new("split-view-reset")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::ArrowClockwise)
                .tooltip(format!("Clear all slots ({filled} filled)"))
                .disabled(filled == 0)
                .on_click(|_, _, cx| {
                    let _ = Bridge::split_view_reset(cx);
                }),
        )
}

fn render_layout(
    layout: SplitLayoutKind,
    slots: &[Option<String>; 4],
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    // Render slots sequentially so each can borrow `window` mutably.
    let s0 = render_slot(0, slots[0].as_deref(), state, editors, window, cx);
    let s1 = render_slot(1, slots[1].as_deref(), state, editors, window, cx);
    match layout {
        SplitLayoutKind::Split2 => ResizablePanelGroup::horizontal("split-2")
            .with_handle(true)
            .child(resizable_panel().child(s0))
            .child(resizable_panel().child(s1))
            .into_any_element(),
        SplitLayoutKind::Split3 => {
            let s2 = render_slot(2, slots[2].as_deref(), state, editors, window, cx);
            ResizablePanelGroup::horizontal("split-3")
                .with_handle(true)
                .child(resizable_panel().child(s0))
                .child(
                    resizable_panel().child(
                        ResizablePanelGroup::vertical("split-3-right")
                            .with_handle(true)
                            .child(resizable_panel().child(s1))
                            .child(resizable_panel().child(s2)),
                    ),
                )
                .into_any_element()
        }
        SplitLayoutKind::Split4 => {
            let s2 = render_slot(2, slots[2].as_deref(), state, editors, window, cx);
            let s3 = render_slot(3, slots[3].as_deref(), state, editors, window, cx);
            ResizablePanelGroup::vertical("split-4")
                .with_handle(true)
                .child(
                    resizable_panel().child(
                        ResizablePanelGroup::horizontal("split-4-top")
                            .with_handle(true)
                            .child(resizable_panel().child(s0))
                            .child(resizable_panel().child(s1)),
                    ),
                )
                .child(
                    resizable_panel().child(
                        ResizablePanelGroup::horizontal("split-4-bottom")
                            .with_handle(true)
                            .child(resizable_panel().child(s2))
                            .child(resizable_panel().child(s3)),
                    ),
                )
                .into_any_element()
        }
    }
}

fn render_slot(
    slot: usize,
    page_id: Option<&str>,
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let page = page_id
        .and_then(Page::from_id)
        .filter(|p| *p != Page::SplitView);
    let theme = cx.theme();
    // Marks are rasterized 1:1 with the device grid.
    let scale = window.scale_factor();

    let mut root = div()
        .id(SharedString::from(format!("split-slot-{slot}")))
        .size_full()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_1()
        .border_color(theme.border.opacity(0.5))
        .bg(theme.background);

    match page {
        Some(page) => {
            root = root.child(render_slot_header(slot, page, scale, cx)).child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .child(render_embedded_page(page, state, editors, window, cx)),
            );
        }
        None => {
            root = root.child(render_app_picker(slot, scale, cx));
        }
    }
    root.into_any_element()
}

fn render_slot_header(slot: usize, page: Page, scale: f32, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .id(SharedString::from(format!("split-slot-header-{slot}")))
        .w_full()
        .h(px(28.))
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_2()
        .border_b_1()
        .border_color(theme.border.opacity(0.5))
        .bg(theme.muted.opacity(0.3))
        .child(
            ImageIcon::src(
                format!("split-slot-icon-{slot}"),
                app_icons::icon_at(page, app_icons::device_px(SLOT_MARK, scale)),
            )
            .size(ImageIconSize::Sm)
            .shape(ImageIconShape::Rounded)
            .bordered(false),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .text_ellipsis()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(page.label()),
        )
        .child(
            Button::new(SharedString::from(format!("split-slot-change-{slot}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::ArrowsLeftRight)
                .tooltip("Change application")
                .dropdown_menu(move |menu, _, _| {
                    let mut menu = menu;
                    for app in Page::split_slot_apps() {
                        let id = app.id();
                        menu =
                            menu.item(PopupMenuItem::new(app.label()).on_click(move |_, _, cx| {
                                let _ = Bridge::split_view_set_slot(slot, Some(id), cx);
                            }));
                    }
                    menu
                }),
        )
        .child(
            Button::new(SharedString::from(format!("split-slot-clear-{slot}")))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .leading_icon(PhosphorIcon::X)
                .tooltip("Clear slot")
                .on_click(move |_, _, cx| {
                    let _ = Bridge::split_view_clear_slot(slot, cx);
                }),
        )
}

fn render_app_picker(slot: usize, scale: f32, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let pick_px = app_icons::device_px(PICK_MARK, scale);
    let mut grid = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_start()
        .justify_center()
        .gap_1()
        .max_w(px(420.));

    for app in Page::split_slot_apps() {
        let id = app.id();
        let tip = app.label();
        grid = grid.child(
            div()
                .id(SharedString::from(format!("split-pick-{slot}-{id}")))
                .size(px(40.))
                .rounded(theme.radius_sm())
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(theme.muted))
                .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                .on_click(move |_, _, cx| {
                    let _ = Bridge::split_view_set_slot(slot, Some(id), cx);
                })
                .child(
                    // Plain img, not ImageIcon: its own `.id()` hitbox would
                    // steal clicks from this pick tile in GPUI.
                    div()
                        .size(px(PICK_MARK))
                        .overflow_hidden()
                        .rounded(theme.radius_sm())
                        .child(
                            img(app_icons::icon_at(app, pick_px))
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        ),
                ),
        );
    }

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .p_4()
        .child(
            Phosphor::new(PhosphorIcon::SquaresFour)
                .size(px(28.))
                .color(theme.muted_foreground),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::NORMAL)
                .child("Fill this slot"),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Pick an app to pin here."),
        )
        .child(grid)
}

fn render_embedded_page(
    page: Page,
    state: &BridgeState,
    editors: &ToolEditors,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    match page {
        Page::HttpHistory => render_http(state, editors, cx).into_any_element(),
        Page::Intercept => render_intercept(state, editors, cx).into_any_element(),
        Page::Repeater => render_repeater(state, editors, cx).into_any_element(),
        Page::Intruder => render_intruder(state, editors, cx).into_any_element(),
        Page::Crawler => render_crawler(state, editors, cx).into_any_element(),
        Page::Coder => render_coder(state, editors, window, cx).into_any_element(),
        Page::SourceControl => render_source_control(state, editors, cx).into_any_element(),
        Page::Agent => render_agent(state, editors, window, cx).into_any_element(),
        Page::Notes => render_notes(state, editors, window, cx).into_any_element(),
        Page::Diagram => render_diagram(state, editors, window, cx).into_any_element(),
        Page::Doctor => render_doctor(state, cx).into_any_element(),
        Page::Settings => render_settings(state, editors, cx).into_any_element(),
        Page::ApiMock => render_api_mock(state, editors, cx).into_any_element(),
        Page::ApiOverride => render_api_override(state, editors, cx).into_any_element(),
        Page::Jwt => render_jwt(state, editors, cx).into_any_element(),
        Page::PortScanner => render_port_scanner(state, editors, cx).into_any_element(),
        Page::Comparer => render_comparer(state, editors, cx).into_any_element(),
        Page::FileExplorer => render_file_explorer(state, editors, cx).into_any_element(),
        Page::R2Storage => render_r2_storage(state, editors, cx).into_any_element(),
        Page::Wordlists => render_wordlists(state, editors, cx).into_any_element(),
        // A satellite never fills a split slot; it has its own window.
        Page::Desktop | Page::SplitView => div().into_any_element(),
    }
}
