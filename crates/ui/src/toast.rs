//! Thin wrappers around celestia-ui `primitive/toast` (gpui-component Notification).

use celestia_ui::components::primitive::toast::{Notification, WindowExt};
use gpui::{App, SharedString, Window, div, prelude::*};
use gpui_component::button::{Button, ButtonVariants as _};
use halo_bridge::Bridge;

/// Marker type for success toast id (replaces previous success toast).
struct BridgeSuccessToast;
/// Marker type for error toast id (replaces previous error toast).
struct BridgeErrorToast;
/// Marker type for info toast id (replaces previous info toast).
struct BridgeInfoToast;

pub fn success(window: &mut Window, cx: &mut App, message: impl Into<SharedString>) {
    window.push_notification(
        Notification::success(message).id::<BridgeSuccessToast>(),
        cx,
    );
}

pub fn error(window: &mut Window, cx: &mut App, message: impl Into<SharedString>) {
    window.push_notification(Notification::error(message).id::<BridgeErrorToast>(), cx);
}

/// Error toast with an Install action stacked below the message (`flex-col`).
///
/// Uses [`Notification::content`] (not `.action`) so the button sits under the
/// text inside the notification's vertical stack. Pushes via gpui-component
/// directly so autohide stays off.
pub fn error_with_install(
    window: &mut Window,
    cx: &mut App,
    message: impl Into<SharedString>,
    server_id: impl Into<String>,
    action_label: impl Into<SharedString>,
) {
    let server_id = server_id.into();
    let action_label = action_label.into();
    let note = Notification::error(message)
        .id::<BridgeErrorToast>()
        .autohide(false)
        .content(move |_this, _window, _cx| {
            let install_id = server_id.clone();
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_start()
                .pt_2()
                .child(
                    Button::new("toast-lsp-install")
                        .primary()
                        .label(action_label.clone())
                        .on_click(move |_, window, cx| {
                            Bridge::coder_lsp_install_server(&install_id, cx);
                            gpui_component::WindowExt::remove_notification::<BridgeErrorToast>(
                                window, cx,
                            );
                        }),
                )
                .into_any_element()
        });
    gpui_component::WindowExt::push_notification(window, note, cx);
}

pub fn info(window: &mut Window, cx: &mut App, message: impl Into<SharedString>) {
    window.push_notification(Notification::info(message).id::<BridgeInfoToast>(), cx);
}

/// Close / dismiss all active toast notifications.
#[allow(dead_code)]
pub fn close(window: &mut Window, cx: &mut App) {
    window.clear_notifications(cx);
}

/// Close / dismiss any active success toast.
#[allow(dead_code)]
pub fn close_success(window: &mut Window, cx: &mut App) {
    window.remove_notification::<BridgeSuccessToast>(cx);
}

/// Close / dismiss any active error toast.
#[allow(dead_code)]
pub fn close_error(window: &mut Window, cx: &mut App) {
    window.remove_notification::<BridgeErrorToast>(cx);
}

/// Close / dismiss any active info toast.
#[allow(dead_code)]
pub fn close_info(window: &mut Window, cx: &mut App) {
    window.remove_notification::<BridgeInfoToast>(cx);
}

/// Alias for [`close`] to clear all active toast notifications.
#[allow(dead_code)]
pub fn clear(window: &mut Window, cx: &mut App) {
    window.clear_notifications(cx);
}

/// Alias for [`close`] to dismiss all active toast notifications.
#[allow(dead_code)]
pub fn dismiss(window: &mut Window, cx: &mut App) {
    window.clear_notifications(cx);
}

/// Progress placeholders like `"Starting proxy…"` — skip until the final result lands.
pub fn is_progress_message(message: &str) -> bool {
    let trimmed = message.trim_end();
    trimmed.ends_with('…') || trimmed.ends_with("...")
}

/// Messages that belong in a status bar rather than the toast lane.
pub fn is_status_bar_message(message: &str) -> bool {
    message.starts_with("Saved ")
}
