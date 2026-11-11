//! Destructive-action confirmation dialogs.
//!
//! Every delete in the UI goes through [`confirm_delete`] (or
//! [`confirm_destructive`] for clear/drop wording), which opens a modal
//! dialog and runs the action only after the user confirms.
//!
//! The dialog is owned by the window root (gpui-component's dialog layer), so
//! it sits above any page-local overlay. Click outside (backdrop), Escape, or
//! Cancel dismisses it — unless a confirm action is already in flight.
//!
//! While the confirm action runs, the primary button keeps the dialog open and
//! shows the same GridShift loading mark used elsewhere in the app.

use std::rc::Rc;
use std::time::Duration;

use celestia_ui::components::primitive::button::{Button, ButtonLoadingStyle, ButtonVariant};
use celestia_ui::components::primitive::toast::WindowExt as _;
use celestia_ui::theme::ActiveTheme as _;
use gpui::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled,
    Window, div, px,
};
use gpui_component::dialog::DialogFooter;

/// Footer buttons as their own view so `.loading` / disabled state re-renders.
///
/// The dialog layer is `RenderOnce` with `&mut App`, so reading a side entity
/// there does not subscribe the layer — a plain `busy` flag never painted.
/// Rendering the footer from this entity fixes that.
struct ConfirmFooter {
    busy: bool,
    label: SharedString,
    on_confirm: Rc<dyn Fn(&mut Window, &mut App) + 'static>,
}

impl ConfirmFooter {
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        cx.notify();

        let on_confirm = self.on_confirm.clone();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            // Yield so the GridShift mark can paint before sync work runs.
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let result = cx.update_window(window_handle, |_, window, cx| {
                on_confirm(window, cx);
                window.close_dialog(cx);
            });
            if result.is_err() {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Render for ConfirmFooter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy;
        let label = self.label.clone();
        let busy_label: SharedString = format!("{label}…").into();
        DialogFooter::new()
            .child(
                Button::new("confirm-dialog-cancel")
                    .variant(ButtonVariant::Outline)
                    .label("Cancel")
                    .disabled(busy)
                    .on_click(|_, window, cx| {
                        window.close_dialog(cx);
                    }),
            )
            .child(
                Button::new("confirm-dialog-action")
                    .variant(ButtonVariant::Destructive)
                    .label(if busy { busy_label } else { label })
                    // Do not also `.disabled(true)` while loading — that cancels
                    // the loading styling on Button.
                    .loading(busy)
                    .loading_style(ButtonLoadingStyle::GridShift)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.start(window, cx);
                    })),
            )
    }
}

/// Confirm a destructive action, using `Delete` as the confirm label.
pub fn confirm_delete(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) {
    confirm_destructive(window, cx, title, description, "Delete", on_confirm);
}

/// Confirm a destructive action with a custom confirm label (`Clear`, `Drop`,
/// `Discard`, …). Cancel, Escape, and click-outside dismiss; backdrop does not
/// dismiss while the action is running.
///
/// The confirm button shows a GridShift loading state and the dialog stays open
/// until the action returns, so slow deletes (e.g. removing a project and all
/// of its chats) match the rest of the app's busy affordance.
pub fn confirm_destructive(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    action_label: impl Into<SharedString>,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) {
    let title = title.into();
    let description = description.into();
    let action_label = action_label.into();
    // `open_dialog` takes an `Fn` builder, which may run more than once,
    // so the non-`Clone` callback has to be shared rather than moved out.
    let on_confirm = Rc::new(on_confirm);
    let footer = cx.new(|_| ConfirmFooter {
        busy: false,
        label: action_label,
        on_confirm: on_confirm.clone(),
    });

    // Use a normal Dialog (not AlertDialog): AlertDialog hard-disables backdrop
    // dismiss. Dialog defaults to overlay_closable and routes backdrop/Escape
    // through on_cancel.
    window.open_dialog(cx, move |dialog, _, _| {
        let footer = footer.clone();
        let description = description.clone();

        dialog
            .title(title.clone())
            .w(px(420.))
            .overlay_closable(true)
            .content(move |content, _, cx| {
                content.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description.clone()),
                )
            })
            .on_ok({
                let footer = footer.clone();
                move |_, window, cx| {
                    footer.update(cx, |this, cx| this.start(window, cx));
                    // Keep the dialog open; we close it after the action finishes.
                    false
                }
            })
            .on_cancel({
                let footer = footer.clone();
                move |_, _, cx| {
                    // Block backdrop / Escape while delete is in flight.
                    !footer.read(cx).busy
                }
            })
            .footer(footer.clone())
    });
}
