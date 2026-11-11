//! Application keymap — single catalog for `bind_keys` and Settings → Keyboard.
//!
//! Window chords (`New` / `Close` / `Minimize` / `Full Screen`) are registered by
//! [`celestia_ui::components::composite::window::bind_window_keys`]; they appear
//! here for discovery only.

use gpui::{App, KeyBinding};

use crate::actions::{
    CoderQuickOpenClose, CoderQuickOpenDown, CoderQuickOpenUp, CommitSourceControl,
    OpenCoderFolder, PickAgentModel, SaveCoderFile, ToggleCoderQuickOpen, ToggleCoderTerminal,
    ToggleCoderWordWrap, ToggleDebugger, ToggleProxy, ToggleTheme, TriggerCoderCompletions,
    primary,
};

/// One shortcut row in Settings → Keyboard.
#[derive(Clone, Debug)]
pub struct HotkeyEntry {
    pub label: &'static str,
    pub description: &'static str,
    /// GPUI keystroke strings (e.g. `cmd-d`, `alt-z`). Multiple = alternate chords.
    pub chords: Vec<String>,
}

/// Grouped shortcuts for the Keyboard settings tab.
#[derive(Clone, Debug)]
pub struct HotkeySection {
    pub title: &'static str,
    pub entries: Vec<HotkeyEntry>,
}

/// Full mapped hotkey list shown in Settings.
pub fn hotkey_catalog() -> Vec<HotkeySection> {
    vec![
        HotkeySection {
            title: "Application",
            entries: vec![
                HotkeyEntry {
                    label: "Toggle theme",
                    description: "Switch between light and dark mode in the active theme family.",
                    chords: vec![primary("d")],
                },
                HotkeyEntry {
                    label: "Toggle proxy",
                    description: "Start or stop the HTTP/TLS intercepting proxy.",
                    chords: vec![primary("shift-p")],
                },
            ],
        },
        HotkeySection {
            title: "Coder",
            entries: vec![
                HotkeyEntry {
                    label: "Save file",
                    description: "Write the focused editor buffer to disk.",
                    chords: vec![primary("s")],
                },
                HotkeyEntry {
                    label: "Go to file",
                    description: "Quickly search and open a file in the workspace.",
                    chords: vec![primary("p")],
                },
                HotkeyEntry {
                    label: "Open folder",
                    description: "Open a project folder in Coder.",
                    chords: vec![primary("o")],
                },
                HotkeyEntry {
                    label: "Toggle terminal",
                    description: "Show or hide the integrated terminal panel.",
                    // Control+` on every platform (⌘+` is reserved / unused here).
                    chords: vec!["ctrl-`".into()],
                },
                HotkeyEntry {
                    label: "Toggle word wrap",
                    description: "Wrap or unwrap long lines in the Coder editor.",
                    chords: vec!["alt-z".into(), primary("alt-z")],
                },
                HotkeyEntry {
                    label: "Commit staged changes",
                    description: "Commit from the Source Control panel (panel must be open).",
                    chords: vec![primary("enter")],
                },
                HotkeyEntry {
                    label: "Trigger completion",
                    description: "Open the autocomplete menu at the caret in the focused pane.",
                    // Control+Space everywhere, matching VS Code. Not scoped to
                    // the editor: the handler no-ops when no Coder pane is
                    // focused, and the terminal is excluded because Ctrl+Space
                    // is a real keystroke there (NUL).
                    chords: vec!["ctrl-space".into()],
                },
            ],
        },
        HotkeySection {
            title: "Window",
            entries: vec![
                HotkeyEntry {
                    label: "New window",
                    description: "Open another Halo window.",
                    chords: vec![primary("n")],
                },
                HotkeyEntry {
                    label: "Close window",
                    description: "Close the current window.",
                    chords: vec![primary("w")],
                },
                HotkeyEntry {
                    label: "Minimize",
                    description: "Minimize the current window.",
                    chords: vec![primary("m")],
                },
                HotkeyEntry {
                    label: "Toggle full screen",
                    description: "Enter or exit full screen.",
                    chords: vec![if cfg!(target_os = "macos") {
                        "cmd-ctrl-f".into()
                    } else {
                        "f11".into()
                    }],
                },
            ],
        },
        HotkeySection {
            title: "Developer",
            entries: vec![HotkeyEntry {
                label: "Toggle debugger",
                description: "Show or hide the in-app DevTools / debugger panel.",
                chords: vec![primary("shift-i"), "f12".into()],
            }],
        },
    ]
}

/// Register Halo action keybindings (not window chrome — use `bind_window_keys`).
pub fn bind_app_keys(cx: &mut App) {
    // Key bindings are matched *before* `on_key_down`, and a matched binding does
    // not stop the listener from running — so a global chord fires while the
    // terminal is focused. Off macOS `primary()` is Ctrl, which is also the
    // shell's control prefix (Ctrl-D EOF, Ctrl-S XOFF, Ctrl-O
    // operate-and-get-next), so those chords are scoped out of the terminal.
    // On macOS they are ⌘-chords, which the terminal ignores anyway.
    let app_scope: Option<&'static str> = if cfg!(target_os = "macos") {
        None
    } else {
        Some("!TerminalSurface")
    };
    cx.bind_keys([
        KeyBinding::new(&primary("d"), ToggleTheme, app_scope),
        KeyBinding::new(&primary("shift-p"), ToggleProxy, None),
        KeyBinding::new(&primary("s"), SaveCoderFile, app_scope),
        KeyBinding::new(&primary("p"), ToggleCoderQuickOpen, app_scope),
        KeyBinding::new(&primary("p"), ToggleCoderQuickOpen, Some("HaloApp")),
        KeyBinding::new("up", CoderQuickOpenUp, Some("CoderQuickOpen")),
        KeyBinding::new("down", CoderQuickOpenDown, Some("CoderQuickOpen")),
        KeyBinding::new("ctrl-p", CoderQuickOpenUp, Some("CoderQuickOpen")),
        KeyBinding::new("ctrl-n", CoderQuickOpenDown, Some("CoderQuickOpen")),
        KeyBinding::new("escape", CoderQuickOpenClose, Some("CoderQuickOpen")),
        // Agent model picker — ⌘/Ctrl+1…9 selects the Nth visible model.
        KeyBinding::new(
            &primary("1"),
            PickAgentModel { index: 0 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("2"),
            PickAgentModel { index: 1 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("3"),
            PickAgentModel { index: 2 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("4"),
            PickAgentModel { index: 3 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("5"),
            PickAgentModel { index: 4 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("6"),
            PickAgentModel { index: 5 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("7"),
            PickAgentModel { index: 6 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("8"),
            PickAgentModel { index: 7 },
            Some("AgentModelPicker"),
        ),
        KeyBinding::new(
            &primary("9"),
            PickAgentModel { index: 8 },
            Some("AgentModelPicker"),
        ),
        // Commit from the Source Control panel. The handler ignores the chord
        // unless that panel is open with staged files and a message.
        KeyBinding::new(&primary("enter"), CommitSourceControl, None),
        // VS Code's trigger-suggest chord. Global rather than editor-scoped so
        // it works whatever the editor's key context is called; the handler
        // ignores it unless a Coder pane is focused. Excluded from the terminal,
        // where Ctrl+Space is a real keystroke (NUL).
        KeyBinding::new(
            "ctrl-space",
            TriggerCoderCompletions,
            Some("!TerminalSurface"),
        ),
        KeyBinding::new(&primary("o"), OpenCoderFolder, app_scope),
        // VS Code-style Control+` (not ⌘) so Mac matches other platforms.
        KeyBinding::new("ctrl-`", ToggleCoderTerminal, None),
        KeyBinding::new("ctrl-`", ToggleCoderTerminal, Some("HaloApp")),
        // VS Code-style Alt+Z. Also bind under HaloApp and with the
        // primary modifier so the chord still fires when the editor or OS
        // would otherwise treat Option/Alt+Z as character input.
        // Never in the terminal: ⌥Z is ESC z, a real keystroke for the shell.
        KeyBinding::new("alt-z", ToggleCoderWordWrap, Some("!TerminalSurface")),
        KeyBinding::new(
            "alt-z",
            ToggleCoderWordWrap,
            Some("HaloApp && !TerminalSurface"),
        ),
        KeyBinding::new(
            &primary("alt-z"),
            ToggleCoderWordWrap,
            Some("!TerminalSurface"),
        ),
        KeyBinding::new(
            &primary("alt-z"),
            ToggleCoderWordWrap,
            Some("HaloApp && !TerminalSurface"),
        ),
        // DevTools-style chords. F12 is a reliable fallback when the
        // OS / focused input swallows cmd-shift-i.
        KeyBinding::new(&primary("shift-i"), ToggleDebugger, None),
        KeyBinding::new(&primary("shift-i"), ToggleDebugger, Some("HaloApp")),
        KeyBinding::new("f12", ToggleDebugger, None),
        KeyBinding::new("f12", ToggleDebugger, Some("HaloApp")),
    ]);
}
