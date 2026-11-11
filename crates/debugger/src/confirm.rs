//! Modal confirmations for destructive actions triggered from the debugger.

use std::rc::Rc;

use celestia_ui::components::primitive::button::{Button, ButtonVariant};
use celestia_ui::components::primitive::toast::WindowExt as _;
use gpui::prelude::ParentElement as _;
use gpui::{App, SharedString, Window};
use gpui_component::dialog::DialogFooter;

/// Opens a modal that runs `on_confirm` only after the user explicitly
/// approves a destructive action.
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
    // `open_alert_dialog` takes an `Fn` builder, which may run more than once,
    // so the non-`Clone` callback has to be shared rather than moved out.
    let on_confirm = Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_confirm = on_confirm.clone();
        let action_label = action_label.clone();
        alert
            .confirm()
            .title(title.clone())
            .description(description.clone())
            .on_ok({
                let on_confirm = on_confirm.clone();
                move |_, window, cx| {
                    on_confirm(window, cx);
                    true
                }
            })
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("debugger-confirm-dialog-cancel")
                            .variant(ButtonVariant::Outline)
                            .label("Cancel")
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new("debugger-confirm-dialog-action")
                            .variant(ButtonVariant::Destructive)
                            .label(action_label)
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                on_confirm(window, cx);
                            }),
                    ),
            )
    });
}
