//! Single-line text prompt dialog.
//!
//! Rename / create flows in different pages all need the same thing: a title,
//! one focused text field, Cancel, and a confirm button. Keeping it here means
//! the empty-name guard and the Enter path behave the same everywhere.

use std::rc::Rc;

use celestia_ui::components::primitive::input::{Input, InputState};
use gpui::{App, AppContext as _, ParentElement as _, SharedString, Styled as _, Window, div, px};
use gpui_component::WindowExt as _;
use gpui_component::dialog::DialogButtonProps;

/// Open a modal with one text field and run `on_ok` with the trimmed value.
///
/// `on_ok` is skipped while the field is blank: the dialog stays open so the
/// user can correct the name instead of losing the work they typed.
pub fn prompt_text(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    initial: &str,
    ok_label: impl Into<SharedString>,
    on_ok: impl Fn(&str, &mut App) + 'static,
) {
    let title = title.into();
    let ok_label = ok_label.into();
    let input = cx.new(|cx| InputState::new(window, cx));
    input.update(cx, |state, cx| {
        state.set_value(initial.to_string(), window, cx);
    });
    let content_input = input.clone();
    let ok_input = input.clone();
    let on_ok = Rc::new(on_ok);

    window.open_dialog(cx, move |dialog, _, _| {
        let content_input = content_input.clone();
        let ok_input = ok_input.clone();
        let on_ok = on_ok.clone();
        let title = title.clone();
        let ok_label = ok_label.clone();
        dialog
            .title(title)
            .w(px(420.))
            .content(move |content, _, _| {
                content.child(div().py_2().child(Input::new(&content_input)))
            })
            .button_props(
                DialogButtonProps::default()
                    .ok_text(ok_label)
                    .cancel_text("Cancel")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                let name = ok_input.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return false;
                }
                on_ok(&name, cx);
                true
            })
    });
}
