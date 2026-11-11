//! Default model catalog for Pi agent.
//!
//! IDs must match `provider` + `/` + `id` from Pi's live `get_available_models`
//! (credentials required). Stale catalog entries fail startup with
//! "Selected Pi model is unavailable or ambiguous".

use super::types::PiModelInfo;

/// Curated subset of models Pi 1.4.0 exposes when the matching provider key is set.
pub fn default_models() -> Vec<PiModelInfo> {
    vec![
        PiModelInfo {
            id: "anthropic/claude-sonnet-5-5".into(),
            display_name: "Claude Sonnet 5.5".into(),
            provider: "anthropic".into(),
        },
        PiModelInfo {
            id: "anthropic/claude-sonnet-4-5".into(),
            display_name: "Claude Sonnet 4.5".into(),
            provider: "anthropic".into(),
        },
        PiModelInfo {
            id: "anthropic/claude-opus-5".into(),
            display_name: "Claude Opus 5".into(),
            provider: "anthropic".into(),
        },
        PiModelInfo {
            id: "anthropic/claude-haiku-4-5".into(),
            display_name: "Claude Haiku 4.5".into(),
            provider: "anthropic".into(),
        },
        PiModelInfo {
            id: "openai/gpt-5.4".into(),
            display_name: "GPT-5.4".into(),
            provider: "openai".into(),
        },
        PiModelInfo {
            id: "openai/gpt-4o".into(),
            display_name: "GPT-4o".into(),
            provider: "openai".into(),
        },
        PiModelInfo {
            id: "openai/gpt-4o-mini".into(),
            display_name: "GPT-4o Mini".into(),
            provider: "openai".into(),
        },
        PiModelInfo {
            id: "openai/o3-mini".into(),
            display_name: "o3-mini".into(),
            provider: "openai".into(),
        },
        PiModelInfo {
            id: "google/gemini-3.5-flash".into(),
            display_name: "Gemini 3.5 Flash".into(),
            provider: "google".into(),
        },
        PiModelInfo {
            id: "google/gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            provider: "google".into(),
        },
        PiModelInfo {
            id: "google/gemini-2.5-flash".into(),
            display_name: "Gemini 2.5 Flash".into(),
            provider: "google".into(),
        },
        PiModelInfo {
            id: "deepseek/deepseek-flash".into(),
            display_name: "DeepSeek Flash".into(),
            provider: "deepseek".into(),
        },
        PiModelInfo {
            id: "deepseek/deepseek-v4-pro".into(),
            display_name: "DeepSeek V4 Pro".into(),
            provider: "deepseek".into(),
        },
        PiModelInfo {
            id: "groq/llama-3.3-70b-versatile".into(),
            display_name: "Llama 3.3 70B (Groq)".into(),
            provider: "groq".into(),
        },
        PiModelInfo {
            id: "ollama".into(),
            display_name: "Ollama (Local)".into(),
            provider: "ollama".into(),
        },
    ]
}

/// Map retired picker IDs onto current Pi catalog keys.
pub fn migrate_model_id(model: &str) -> String {
    match model {
        "anthropic/claude-3-7-sonnet" | "anthropic/claude-3-5-sonnet" => {
            "anthropic/claude-sonnet-5-5".into()
        }
        "anthropic/claude-3-5-haiku" => "anthropic/claude-haiku-4-5".into(),
        "deepseek/deepseek-v4-flash" => "deepseek/deepseek-flash".into(),
        "deepseek/deepseek-r1" => "deepseek/deepseek-v4-pro".into(),
        // The retired Pi antigravity/* provider: keep existing selections usable
        // by pointing them at the equivalent Gemini API model.
        "antigravity/gemini-3.8-flash" | "antigravity/gemini-3.5-flash" => {
            "google/gemini-3.5-flash".into()
        }
        "antigravity/gemini-3.1-pro" => "google/gemini-2.5-pro".into(),
        // Cursor SDK's Auto selection surfaces as `default` on ready — map it
        // back to the catalog id Halo lists.
        "default" => "auto".into(),
        other => other.to_string(),
    }
}
