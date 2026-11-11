//! Cloudflare R2 credentials — account metadata on disk, secret in the keychain.

use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::PhosphorIcon;
use celestia_ui::components::primitive::input::InputState;
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::{Bridge, BridgeState};

use crate::components::confirm_destructive;
use crate::pages::chrome::section;
use crate::pages::editors::ToolEditors;

pub fn render_r2_tab(state: &BridgeState, editors: &ToolEditors, cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_4()
        .child(render_status(state, cx))
        .child(section(
            "Cloudflare R2",
            Some(
                "Account metadata is saved in a private settings file. The Secret Access Key is stored in the macOS keychain and is never written to disk."
                    .into(),
            ),
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_3()
                .child(field(
                    "Account ID",
                    Some("Found on the Cloudflare dashboard overview."),
                    editors.r2_account.clone(),
                    cx,
                ))
                .child(field(
                    "Access Key ID",
                    Some("From an R2 API token with object read and write access."),
                    editors.r2_access_key.clone(),
                    cx,
                ))
                .child(field(
                    "Secret Access Key",
                    Some("Leave blank to keep the saved secret."),
                    editors.r2_secret.clone(),
                    cx,
                ))
                .child(field(
                    "Custom Endpoint URL",
                    Some("Optional. Blank uses https://<account>.r2.cloudflarestorage.com."),
                    editors.r2_endpoint.clone(),
                    cx,
                ))
                .child(render_actions(state, editors, cx))
                .child(feedback_line(state, cx)),
            cx,
        ))
}

fn render_status(state: &BridgeState, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let credentials = state.r2.credentials.clone();
    let (tone, title) = match &credentials {
        Some(credentials) if credentials.has_secret => {
            (theme.success, "Connected to Cloudflare R2")
        }
        Some(_) => (theme.warning, "Account saved — secret missing"),
        None => (theme.muted_foreground, "Not configured"),
    };

    let mut body = div().flex().flex_col().gap_1().child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_color(if credentials.is_some() {
                        theme.primary
                    } else {
                        theme.muted_foreground
                    })
                    .child(PhosphorIcon::Cloud),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::NORMAL)
                    .child(title),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .text_xs()
                    .text_color(tone)
                    .child(div().size(px(7.)).rounded_full().bg(tone))
                    .child(match &credentials {
                        Some(credentials) if credentials.has_secret => "Secret stored",
                        Some(_) => "Secret missing",
                        None => "No credentials",
                    }),
            ),
    );

    if let Some(credentials) = &credentials {
        body = body
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Account {}", credentials.account_id))
                    .child(format!("Access key {}", credentials.access_key_id)),
            )
            .children(credentials.custom_endpoint_url.as_ref().map(|endpoint| {
                div()
                    .text_xs()
                    .font_family("Menlo")
                    .text_color(theme.muted_foreground)
                    .child(endpoint.clone())
            }));
    } else {
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Add your account ID, access key, and secret to use the R2 Storage app."),
        );
    }

    div()
        .w_full()
        .p_3()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.2))
        .child(body)
        .into_any_element()
}

fn field(label: &str, hint: Option<&str>, input: Entity<InputState>, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::NORMAL)
                .child(label.to_string()),
        )
        .child(div().w_full().child(input))
        .children(hint.map(|hint| div().text_xs().text_color(muted).child(hint.to_string())))
}

fn render_actions(state: &BridgeState, editors: &ToolEditors, _cx: &App) -> AnyElement {
    let busy = state.r2.busy;
    let has_saved = state
        .r2
        .credentials
        .as_ref()
        .map(|credentials| credentials.has_secret)
        .unwrap_or(false);

    let account = editors.r2_account.clone();
    let access_key = editors.r2_access_key.clone();
    let secret = editors.r2_secret.clone();
    let endpoint = editors.r2_endpoint.clone();

    div()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .child(
            Button::new("r2-save")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Check)
                .label(if busy { "Saving…" } else { "Save Credentials" })
                .disabled(busy)
                .loading(busy)
                .loading_style(ButtonLoadingStyle::GridShift)
                .on_click(move |_, window, cx| {
                    let account_id = account.read(cx).value().to_string();
                    let access_key_id = access_key.read(cx).value().to_string();
                    let secret_value = secret.read(cx).value().to_string();
                    let endpoint_value = endpoint.read(cx).value().to_string();
                    let secret_value = if secret_value.trim().is_empty() {
                        None
                    } else {
                        Some(secret_value)
                    };
                    let endpoint_value = if endpoint_value.trim().is_empty() {
                        None
                    } else {
                        Some(endpoint_value.trim().to_string())
                    };
                    match Bridge::r2_save_credentials(
                        account_id,
                        access_key_id,
                        secret_value,
                        endpoint_value,
                        cx,
                    ) {
                        Ok(()) => {
                            secret.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                        }
                        Err(err) => {
                            Bridge::state(cx).update(cx, |s| s.apply_err(err));
                        }
                    }
                }),
        )
        .child(
            Button::new("r2-clear")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Small)
                .leading_icon(PhosphorIcon::Trash)
                .label("Clear")
                .disabled(busy || !has_saved)
                .on_click(|_, window, cx| {
                    confirm_destructive(
                        window,
                        cx,
                        "Clear R2 credentials?",
                        "The saved account details and the keychain secret will be removed. Buckets stay untouched.",
                        "Clear",
                        |_, cx| {
                            if let Err(err) = Bridge::r2_clear_credentials(cx) {
                                Bridge::state(cx).update(cx, |s| s.apply_err(err));
                            }
                        },
                    );
                }),
        )
        .into_any_element()
}

fn feedback_line(state: &BridgeState, cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    if let Some(err) = state.r2.error.as_ref().or(state.last_error.as_ref()) {
        div()
            .id("r2-err")
            .role(Role::Alert)
            .text_xs()
            .text_color(cx.theme().danger)
            .child(err.clone())
    } else if let Some(msg) = state.last_message.as_ref() {
        div()
            .id("r2-msg")
            .role(Role::Status)
            .text_xs()
            .text_color(muted)
            .child(msg.clone())
    } else {
        div()
            .id("r2-hint")
            .text_xs()
            .text_color(muted)
            .child("Settings apply to the R2 Storage app immediately after saving.")
    }
}
