use gpui::{Action, actions};

actions!(
    halo,
    [
        ToggleTheme,
        ToggleProxy,
        SaveCoderFile,
        ToggleCoderTerminal,
        ToggleCoderWordWrap,
        /// Open the completion menu at the caret in the focused Coder pane.
        TriggerCoderCompletions,
        /// Commit the staged changes from the Source Control panel (⌘/Ctrl+Enter).
        CommitSourceControl,
        ToggleDebugger,
        /// File → New Project… (Coder) — same as Open Folder.
        NewCoderProject,
        /// File → Open Folder… (Coder).
        OpenCoderFolder,
        /// File → Close Project (Coder) — clear the open folder, keep recents.
        CloseCoderProject,
        /// File → Settings… (Coder) — open the Settings window on the Coder tabs.
        OpenCoderSettings,
        /// Go to File… (Coder Quick Open) — ⌘P / Ctrl+P.
        ToggleCoderQuickOpen,
        /// Move up in Quick Open results.
        CoderQuickOpenUp,
        /// Move down in Quick Open results.
        CoderQuickOpenDown,
        /// Close Quick Open dialog.
        CoderQuickOpenClose,
        /// App menu → Quit (macOS application menu).
        QuitApp,
        /// App menu → Hide Halo.
        HideApp,
        /// App menu → Hide Others.
        HideOtherApps,
        /// App menu → Show All.
        ShowAllApps,
    ]
);

/// File → Open Recent → \<path\> (Coder). Built dynamically into the menu bar.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = halo, no_json)]
pub struct OpenCoderRecent {
    pub path: String,
}

/// Pick the Nth visible model in the Agent model picker (⌘/Ctrl+1…9).
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = halo, no_json)]
pub struct PickAgentModel {
    pub index: u8,
}

/// Platform primary modifier + key chord for `bind_keys` / `Kbd` display.
pub fn primary(key: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-{key}")
    } else {
        format!("ctrl-{key}")
    }
}
