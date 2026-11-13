//! DeepSeek direct-API setup panel.

use super::*;

pub(super) const DEEPSEEK_FLASH: &str = "deepseek/deepseek-flash";
pub(super) const DEEPSEEK_PRO: &str = "deepseek/deepseek-v4-pro";

/// Key plus the two models Pi ships for DeepSeek (`https://api.deepseek.com`).
pub(super) fn deepseek_setup(
    settings: &CoderSettings,
    editors: &ToolEditors,
    current_model: Option<&str>,
    cx: &App,
) -> impl IntoElement {
    let current = current_model.unwrap_or("").to_string();
    section(
        "DeepSeek",
        "Create an API key at platform.deepseek.com to use DeepSeek models in Pi.",
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(credential_card(
                "DeepSeek",
                "Enter your DeepSeek API key (sk-…). Saving a key selects DeepSeek V4 Pro when another provider is active.",
                settings.deepseek_api_key.as_deref(),
                &editors.coder_settings_deepseek_key,
                "coder-deepseek-key-apply",
                "coder-deepseek-key-clear",
                "Save key",
                {
                    let current = current.clone();
                    move |val, cx| {
                        let select_pro = val.is_some() && !current.starts_with("deepseek/");
                        Bridge::update_coder_settings(cx, move |s| {
                            s.deepseek_api_key = val;
                        });
                        if select_pro {
                            let _ = Bridge::halo_agent_set_model(DEEPSEEK_PRO, cx);
                        }
                    }
                },
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(chip(
                        "coder-deepseek-model-flash",
                        "Flash",
                        current_model == Some(DEEPSEEK_FLASH),
                        |cx| {
                            let _ = Bridge::halo_agent_set_model(DEEPSEEK_FLASH, cx);
                        },
                    ))
                    .child(chip(
                        "coder-deepseek-model-pro",
                        "V4 Pro",
                        current_model == Some(DEEPSEEK_PRO),
                        |cx| {
                            let _ = Bridge::halo_agent_set_model(DEEPSEEK_PRO, cx);
                        },
                    )),
            ),
        cx,
    )
}
