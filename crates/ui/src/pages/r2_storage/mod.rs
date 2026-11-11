//! R2 Storage — Cloudflare R2 buckets, prefixes, and object transfers.
//!
//! Credentials live in Settings → R2 (the secret in the OS keychain). Without
//! them the page is an onboarding card; with them it is a three-pane client:
//! buckets, the current prefix, and object details.

use celestia_ui::components::primitive::button::{Button, ButtonSize, ButtonVariant};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::scroll::ScrollableElement as _;
use halo_bridge::{
    Bridge, BridgeState, ExplorerViewMode, R2FileEntry, R2FolderEntry, R2UploadProgress, R2View,
};

use crate::components::confirm_delete;
use crate::pages::chrome::{PAGE_GAP, empty_state, format_bytes, panel};
use crate::pages::editors::ToolEditors;
use gpui_component::menu::{ContextMenuExt as _, PopupMenuItem};

pub fn render_r2_storage(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    let r2 = &state.r2;
    let body: AnyElement = if r2.has_credentials() {
        render_workspace(r2, editors, cx)
    } else if r2.credentials_loaded {
        render_onboarding(r2, cx)
    } else {
        centered_hint("Checking R2 credentials…", cx).into_any_element()
    };

    div()
        .id("r2-storage-page")
        .relative()
        .w_full()
        .h_full()
        .min_h(px(480.))
        .flex()
        .flex_col()
        .gap(px(PAGE_GAP))
        .child(body)
        .children(r2.upload.as_ref().map(|upload| upload_card(upload, cx)))
}

// ── onboarding ─────────────────────────────────────────────────────────────

fn render_onboarding(r2: &R2View, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let detail = if r2.credentials.is_some() {
        "Account details are saved, but no Secret Access Key is stored yet."
    } else {
        "Add your Cloudflare account ID, access key, and secret to browse buckets and transfer objects."
    };

    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(430.))
                .p_6()
                .rounded(px(12.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .text_color(theme.primary)
                        .text_size(px(40.))
                        .child(PhosphorIcon::Cloud),
                )
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::NORMAL)
                        .child("Cloudflare R2 Storage Not Configured"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_center()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                )
                .child(
                    Button::new("r2-configure")
                        .variant(ButtonVariant::Primary)
                        .leading_icon(PhosphorIcon::Gear)
                        .label("Configure Credentials")
                        .on_click(|_, _, cx| {
                            crate::windows::open_settings_window(halo_bridge::SettingsTab::R2, cx);
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("The Secret Access Key is stored in the macOS keychain."),
                ),
        )
        .children(r2.error.as_ref().map(|message| {
            div()
                .absolute()
                .bottom(px(16.))
                .left(px(16.))
                .right(px(16.))
                .child(error_banner(message, cx))
        }))
        .into_any_element()
}

fn centered_hint(message: &str, cx: &App) -> AnyElement {
    div()
        .w_full()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(message.to_string())
        .into_any_element()
}

fn error_banner(message: &str, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1p5()
        .rounded(theme.radius_sm())
        .border_1()
        .border_color(theme.danger.opacity(0.4))
        .bg(theme.danger.opacity(0.08))
        .text_xs()
        .text_color(theme.danger)
        .child(PhosphorIcon::WarningCircle)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(message.to_string()),
        )
        .into_any_element()
}

// ── workspace ──────────────────────────────────────────────────────────────

mod bucket_sidebar;
mod details;
mod objects;
mod toolbar;

use bucket_sidebar::*;
use details::*;
use objects::*;
use toolbar::*;
