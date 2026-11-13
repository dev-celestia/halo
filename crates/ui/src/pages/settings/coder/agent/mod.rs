//! Agent section — harness, model, autonomy, and context handed to a run.

use celestia_ui::components::Input;
use celestia_ui::components::primitive::button::{
    Button, ButtonLoadingStyle, ButtonSize, ButtonVariant,
};
use celestia_ui::components::primitive::icon::{Phosphor, PhosphorIcon};
use celestia_ui::components::primitive::switch::Switch;
use celestia_ui::components::primitive::tabs::{Tabs, TabsList, TabsSize, TabsTrigger};
use celestia_ui::theme::ActiveTheme as _;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::PopupMenuItem;
use halo_bridge::{
    AgentApprovalMode, AntigravityLoginStatus, Bridge, BridgeState, CoderSettings, McpKeyValue,
    McpServerConfig, McpTransport,
};

use crate::pages::editors::ToolEditors;

use crate::pages::settings::widgets::{
    action, chip, dropdown_select, row, rows, section, stepper, switch,
};

pub(in crate::pages::settings) fn render(
    state: &BridgeState,
    editors: &ToolEditors,
    cx: &App,
) -> impl IntoElement {
    let settings = state.coder_settings.clone();
    let harnesses = Bridge::halo_agent_list_harnesses(cx).unwrap_or_default();
    let current_harness = state.coder.agent_harness.clone();
    let models = Bridge::halo_agent_list_models(&current_harness, cx).unwrap_or_default();
    let current_model = state.coder.agent_model.clone();
    let installed = state.coder.agent_harness_installed;
    let installing = state.coder.agent_installing.clone();
    let needs_upgrade = state.coder.agent_pi_needs_upgrade;
    let setup_hint = state.coder.agent_pi_setup_hint.clone();

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section(
            "Tool approval",
            "Default approval policy for new sessions. You can override this per chat from the composer toolbar.",
            rows().child(row(
                "Approval mode",
                settings.agent_approval_mode.description(),
                approval_mode_dropdown(settings.agent_approval_mode),
                cx,
            )),
            cx,
        ))
        .child(section(
            "Turn limits",
            "Pause long-running tool loops automatically so you can review progress before continuing.",
            rows().child(row(
                "Max turns",
                "Maximum tool-call rounds before a run pauses for confirmation.",
                stepper(
                    "coder-agent-max-turns",
                    i64::from(settings.agent_max_turns),
                    1,
                    40,
                    |value, cx| {
                        Bridge::update_coder_settings(cx, move |s| {
                            s.agent_max_turns = value as u16;
                        });
                    },
                ),
                cx,
            )),
            cx,
        ))
        .child(section(
            "Audio cues",
            "Subtle sound effects when a turn finishes, needs approval, or encounters an error.",
            rows().child(row(
                "Session sounds",
                "Play an audio cue on completion, approval requests, and errors.",
                switch(
                    "coder-agent-sound",
                    settings.agent_sound_enabled,
                    |next, cx| {
                        Bridge::update_coder_settings(cx, move |s| {
                            s.agent_sound_enabled = next;
                        });
                    },
                ),
                cx,
            )),
            cx,
        ))
        .child(section(
            "Prompt context",
            "Workspace context automatically included with each prompt.",
            rows()
                .child(row(
                    "Include open files",
                    "Attach the file paths and buffers currently open in editor tabs.",
                    switch(
                        "coder-agent-open-files",
                        settings.agent_include_open_files,
                        |next, cx| {
                            Bridge::update_coder_settings(cx, move |s| {
                                s.agent_include_open_files = next;
                            });
                        },
                    ),
                    cx,
                ))
                .child(row(
                    "Include git diff",
                    "Attach uncommitted workspace changes so the agent has up-to-date diff context.",
                    switch(
                        "coder-agent-git-diff",
                        settings.agent_include_git_diff,
                        |next, cx| {
                            Bridge::update_coder_settings(cx, move |s| {
                                s.agent_include_git_diff = next;
                            });
                        },
                    ),
                    cx,
                )),
            cx,
        ))
        .child(section(
            "Browser automation",
            "Drive an isolated Chrome session via halo-browser with generation-checked element references.",
            browser_setup(
                &state.coder.agent_browser,
                state.coder.agent_browser_loading,
                cx,
            ),
            cx,
        ))
        .child(section(
            "Desktop control",
            "Inspect and interact with desktop applications through the macOS Accessibility tree.",
            computer_use_setup(state, cx),
            cx,
        ))
        .child(section(
            "Provider API keys",
            "Stored locally on your machine and passed to agent sessions.",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_3()
                .child(credential_card(
                    "Anthropic",
                    "Enter your Anthropic API key (sk-ant-…) to use Claude models.",
                    settings.anthropic_api_key.as_deref(),
                    &editors.coder_settings_anthropic_key,
                    "coder-anthropic-key-apply",
                    "coder-anthropic-key-clear",
                    "Save key",
                    |val, cx| {
                        Bridge::update_coder_settings(cx, move |s| {
                            s.anthropic_api_key = val;
                        });
                    },
                    cx,
                ))
                .child(credential_card(
                    "OpenAI",
                    "Enter your OpenAI API key (sk-…) to use GPT and o-series models.",
                    settings.openai_api_key.as_deref(),
                    &editors.coder_settings_openai_key,
                    "coder-openai-key-apply",
                    "coder-openai-key-clear",
                    "Save key",
                    |val, cx| {
                        Bridge::update_coder_settings(cx, move |s| {
                            s.openai_api_key = val;
                        });
                    },
                    cx,
                ))
                .child(credential_card(
                    "Google Gemini",
                    "Enter your Google AI Studio API key (AIza…) to use Gemini models.",
                    settings.gemini_api_key.as_deref(),
                    &editors.coder_settings_gemini_key,
                    "coder-gemini-key-apply",
                    "coder-gemini-key-clear",
                    "Save key",
                    |val, cx| {
                        Bridge::update_coder_settings(cx, move |s| {
                            s.gemini_api_key = val;
                        });
                    },
                    cx,
                ))
                .child(credential_card(
                    "Ollama",
                    "Connect to a local Ollama server (default: http://localhost:11434/v1).",
                    settings.ollama_base_url.as_deref(),
                    &editors.coder_settings_ollama_url,
                    "coder-ollama-url-apply",
                    "coder-ollama-url-clear",
                    "Save URL",
                    |val, cx| {
                        Bridge::update_coder_settings(cx, move |s| {
                            s.ollama_base_url = val;
                        });
                    },
                    cx,
                )),
            cx,
        ))
        .child(section(
            "Agent harness",
            "Select the runtime that powers Halo Agent, along with its default model and binary path.",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_4()
                .child(harness_tabs(
                    harnesses.iter().map(|h| {
                        let mut label = h.display_name.clone();
                        if !h.available {
                            label.push_str(" (soon)");
                        } else if !h.installed {
                            label.push_str(" (not installed)");
                        }
                        (h.id.clone(), label, h.available)
                    }),
                    &current_harness,
                ))
                .child(harness_tab_body(
                    &current_harness,
                    &settings,
                    editors,
                    model_chips(models.iter().map(|m| (m.id.clone(), m.display_name.clone())), current_model.as_deref()),
                    current_model.as_deref(),
                    installed,
                    installing.as_deref(),
                    needs_upgrade,
                    setup_hint.as_deref(),
                    cx,
                )),
            cx,
        ))
}

/// Celestia Tabs strip — selecting a trigger also selects that harness.
mod antigravity;
mod browser;
mod credential;
mod deepseek;
mod mcp;
mod opencode;
mod pi;
mod shared;

use antigravity::*;
use browser::*;
use credential::*;
use deepseek::*;
use mcp::*;
use opencode::*;
use pi::*;
use shared::*;
