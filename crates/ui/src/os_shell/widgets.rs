//! Reorderable session widgets on the desktop.

use celestia_ui::components::{Phosphor, PhosphorIcon, Tooltip};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use halo_bridge::BridgeState;

use crate::app::HaloApp;

use super::helpers::proxy_metric;
use super::widget_cards::render_proxy_widget;

/// Session widgets shown on the desktop (reorderable).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesktopWidget {
    Proxy,
}

impl DesktopWidget {
    pub(crate) const ALL: [Self; 1] = [Self::Proxy];

    fn id(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Proxy => "Proxy",
        }
    }
}

#[derive(Clone)]
struct WidgetDrag {
    widget: DesktopWidget,
    label: SharedString,
    position: Point<Pixels>,
}

impl Render for WidgetDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let size = size(px(180.), px(40.));
        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                div()
                    .w(size.width)
                    .h(size.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.accent.opacity(0.5))
                    .bg(theme.popover.opacity(0.92))
                    .shadow_md()
                    .child(
                        Phosphor::new(PhosphorIcon::DotsSixVertical)
                            .size(px(14.))
                            .color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.foreground)
                            .child(self.label.clone()),
                    ),
            )
    }
}

pub(super) fn render_widgets(
    state: &BridgeState,
    order: &[DesktopWidget],
    dragging: Option<DesktopWidget>,
    drop_target: Option<DesktopWidget>,
    weak: &WeakEntity<HaloApp>,
    cx: &App,
) -> impl IntoElement {
    let (proxy_value, proxy_delta, proxy_tone) = proxy_metric(state);

    let mut stack = div()
        .id("widgets")
        .w(px(240.))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(10.));

    for &widget in order {
        let body = match widget {
            DesktopWidget::Proxy => render_proxy_widget(
                state,
                proxy_value.clone(),
                proxy_delta.clone(),
                proxy_tone,
                weak,
                cx,
            )
            .into_any_element(),
        };

        stack = stack.child(sortable_widget(
            widget,
            body,
            dragging == Some(widget),
            drop_target == Some(widget),
            weak,
            cx,
        ));
    }

    stack
}

fn sortable_widget(
    widget: DesktopWidget,
    child: impl IntoElement,
    dragging: bool,
    drop_target: bool,
    weak: &WeakEntity<HaloApp>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let weak_drag = weak.clone();
    let weak_over = weak.clone();
    let weak_drop = weak.clone();
    let drag = WidgetDrag {
        widget,
        label: widget.label().into(),
        position: Point::default(),
    };

    div()
        .id(SharedString::from(format!("sortable-{}", widget.id())))
        .relative()
        .w_full()
        .flex_none()
        .rounded(theme.radius)
        .when(dragging, |this| this.opacity(0.45))
        .when(drop_target && !dragging, |this| {
            this.border_1()
                .border_color(theme.accent.opacity(0.55))
                .bg(theme.accent.opacity(0.08))
        })
        .can_drop(|payload, _, _| payload.is::<WidgetDrag>())
        .on_drag_move(move |event: &DragMoveEvent<WidgetDrag>, _, cx| {
            let dragged = event.drag(cx).widget;
            if dragged == widget {
                return;
            }
            if let Some(app) = weak_over.upgrade() {
                app.update(cx, |this, cx| {
                    this.set_widget_drop_target(Some(widget), cx);
                });
            }
        })
        .on_drop(move |payload: &WidgetDrag, _, cx| {
            let from = payload.widget;
            if let Some(app) = weak_drop.upgrade() {
                app.update(cx, |this, cx| this.reorder_widget(from, widget, cx));
            }
        })
        .child(child)
        .child(
            div()
                .id(SharedString::from(format!("drag-{}", widget.id())))
                .absolute()
                .top(px(6.))
                .right(px(6.))
                .size(px(18.))
                .rounded(theme.radius_sm())
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(theme.border.opacity(0.5))
                .bg(theme.muted.opacity(0.65))
                .cursor_move()
                .hover(|s| s.bg(theme.muted))
                .tooltip(move |window, cx| Tooltip::new("Drag to reorder").build(window, cx))
                .on_drag(drag, move |info, position, _, cx| {
                    if let Some(app) = weak_drag.upgrade() {
                        app.update(cx, |this, cx| {
                            this.set_dragging_widget(Some(info.widget), cx);
                        });
                    }
                    let mut preview = info.clone();
                    preview.position = position;
                    cx.new(|_| preview)
                })
                .child(
                    Phosphor::new(PhosphorIcon::DotsSixVertical)
                        .size(px(12.))
                        .color(theme.muted_foreground),
                ),
        )
}
