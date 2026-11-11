//! Turning a TCC state into the one action that fixes it.
//!
//! A permission prompt is the worst place to be vague: macOS shows one list, the
//! user has to guess which app belongs in it, and a rebuilt binary silently
//! inherits nothing. Worse, the app that must be added is *not* this CLI — TCC
//! attributes the grant to whatever launched the process.
//!
//! This module owns the wording for every surface — `status`, `permissions`, and
//! the `PERM_DENIED` error — so the three cannot drift apart.

use crate::types::{EngineStatus, PermissionState};

pub const ACCESSIBILITY_PANE: &str = "System Settings › Privacy & Security › Accessibility";
pub const SCREEN_RECORDING_PANE: &str = "System Settings › Privacy & Security › Screen Recording";

#[derive(Debug, Clone)]
pub struct PermissionRow {
    pub label: &'static str,
    pub state: PermissionState,
    pub pane: &'static str,
    pub needed_for: &'static str,
}

/// What each permission buys, in the order the user should care about them.
pub fn rows(status: &EngineStatus) -> Vec<PermissionRow> {
    vec![
        PermissionRow {
            label: "accessibility",
            state: status.accessibility,
            pane: ACCESSIBILITY_PANE,
            needed_for: "reading the tree and acting on controls",
        },
        PermissionRow {
            label: "screen recording",
            state: status.screen_recording,
            pane: SCREEN_RECORDING_PANE,
            needed_for: "screenshots only — not needed to read or act",
        },
    ]
}

/// One line per permission, for `status` and `permissions`.
pub fn summary(status: &EngineStatus) -> String {
    rows(status)
        .into_iter()
        .map(|row| {
            format!(
                "{}: {} ({})",
                row.label,
                row.state.label(),
                row.needed_for
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The full instructions, only when something is actually missing.
///
/// `None` means every permission the current phase needs is granted, so callers
/// do not print advice at a user who has already done it.
pub fn guidance(status: &EngineStatus) -> Option<String> {
    if status.accessibility.is_granted() {
        return None;
    }

    let owner = owner_phrase();
    let state = status.accessibility.label();
    Some(format!(
        "Accessibility is {state}, so no window can be read or operated.

  1. Open {ACCESSIBILITY_PANE}
  2. Add {owner}
     That is the app that launched this command, not the CLI itself. System
     Settings lists apps by display name; if it also lists halo-desktop or
     agent-desktop separately, add those too.
  3. Turn its switch on, then run `halo-desktop permissions` again.

  Already switched on? A rebuilt binary can sit under a stale grant: switch it
  off and back on, or remove the entry and add the app again.

  Jump straight to the pane: `halo-desktop permissions --open`
"
    ))
}

/// The optional grant. Never printed by `status` — a user who does not want
/// screenshots should not be nagged — but `permissions` always offers it,
/// because that is the command someone runs when they are trying to enable
/// something and cannot find where.
pub fn screen_recording_guidance(status: &EngineStatus) -> Option<String> {
    if status.screen_recording.is_granted() {
        return None;
    }
    let owner = owner_phrase();
    Some(format!(
        "Screen Recording is {}. Only `screenshot` needs it — reading the tree and \
         acting on controls work without it.

  1. Open {SCREEN_RECORDING_PANE}
  2. Add {owner}
     That is the app that launched this command, not the CLI itself.
  3. Quit and reopen it: macOS applies a new Screen Recording grant only to a
     fresh process, so the switch alone will not take effect
",
        status.screen_recording.label()
    ))
}

/// Short form for the `PERM_DENIED` error path, where the agent needs the fix in
/// one sentence and can ask for the long version itself.
pub fn denied_advice() -> String {
    format!(
        "Accessibility permission is not granted, so no window can be read. Ask the user to \
         open {ACCESSIBILITY_PANE} and add {} (the app that launched this command), then \
         retry. `halo-desktop permissions` prints the full steps.",
        owner_phrase()
    )
}

/// Which app TCC will attribute the grant to.
///
/// TCC keys a grant to the *responsible* process, not to the binary it spawned,
/// so a CLI run from Halo is covered by Halo's grant and one run from a terminal
/// is covered by the terminal's. Guessing wrong here sends the user to a pane
/// where the app they were told to add is not even listed.
pub fn owner_hint() -> String {
    // An explicit override wins: the app can set it to its own display name and
    // stop every heuristic below from guessing.
    if let Ok(owner) = std::env::var("HALO_DESKTOP_OWNER") {
        if !owner.trim().is_empty() {
            return owner;
        }
    }
    owner_from(
        std::env::var("__CFBundleIdentifier").ok().as_deref(),
        std::env::var("TERM_PROGRAM").ok().as_deref(),
    )
}

/// How to name the app in prose.
///
/// A reverse-DNS bundle id is accurate but unreadable, and System Settings lists
/// apps by display name. Say what it is rather than printing a raw identifier at
/// someone who is already stuck.
fn owner_phrase() -> String {
    owner_phrase_from(&owner_hint())
}

fn owner_phrase_from(owner: &str) -> String {
    let looks_like_bundle = owner.contains('.')
        && !owner.contains(' ')
        && owner.split('.').count() >= 2
        && !owner.contains('_');
    if looks_like_bundle {
        format!("the app with bundle id {owner}")
    } else {
        owner.to_string()
    }
}

fn owner_from(bundle: Option<&str>, program: Option<&str>) -> String {
    if let Some(bundle) = bundle.filter(|value| !value.trim().is_empty()) {
        let display = match bundle {
            "com.apple.Terminal" => "Terminal",
            "com.googlecode.iterm2" => "iTerm",
            "com.microsoft.VSCode" => "Visual Studio Code",
            "dev.warp.Warp-Stable" => "Warp",
            "com.apple.dt.Xcode" => "Xcode",
            "com.tinyspeck.slackmacgap" => "Slack",
            other => other,
        };
        return display.to_string();
    }

    if let Some(program) = program.filter(|value| !value.trim().is_empty()) {
        return program.to_string();
    }

    // Launched from a GUI app with no bundle id in the environment: the only
    // honest answer is the app that owns this feature.
    "Halo".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(accessibility: PermissionState, screen: PermissionState) -> EngineStatus {
        EngineStatus {
            accessibility,
            screen_recording: screen,
            ..EngineStatus::default()
        }
    }

    #[test]
    fn known_terminals_are_named_not_guessed() {
        assert_eq!(owner_from(Some("com.apple.Terminal"), None), "Terminal");
        assert_eq!(owner_from(Some("com.googlecode.iterm2"), None), "iTerm");
        assert_eq!(owner_from(None, Some("Apple_Terminal")), "Apple_Terminal");
    }

    #[test]
    fn an_unknown_bundle_is_reported_as_is() {
        assert_eq!(owner_from(Some("com.example.thing"), None), "com.example.thing");
    }

    #[test]
    fn a_gui_launch_falls_back_to_halo() {
        assert_eq!(owner_from(None, None), "Halo");
    }

    #[test]
    fn a_bundle_id_is_explained_instead_of_printed_bare() {
        assert_eq!(owner_phrase_from("iTerm"), "iTerm");
        assert_eq!(owner_phrase_from("Halo"), "Halo");
        let phrase = owner_phrase_from("com.example.app");
        assert!(phrase.contains("bundle id com.example.app"), "{phrase}");
        assert!(!phrase.contains("display name"), "{phrase}");
    }

    #[test]
    fn no_guidance_once_accessibility_is_granted() {
        let granted = status(PermissionState::Granted, PermissionState::Denied);
        assert!(guidance(&granted).is_none(), "advice must not nag");
    }

    #[test]
    fn denied_guidance_names_the_pane_the_app_and_the_retry() {
        let denied = status(PermissionState::Denied, PermissionState::Granted);
        let text = guidance(&denied).expect("denied must produce guidance");
        assert!(text.contains(ACCESSIBILITY_PANE), "{text}");
        assert!(text.contains("the app that launched this command"), "{text}");
        assert!(text.contains("halo-desktop permissions --open"), "{text}");
        // Already granted, so no screen-recording detour.
        assert!(!text.contains("Screen Recording"), "{text}");
    }

    #[test]
    fn the_optional_grant_is_explained_separately_and_names_the_restart() {
        let partial = status(PermissionState::Granted, PermissionState::Denied);
        // Not blocking, so the status path stays quiet.
        assert!(guidance(&partial).is_none());

        let text = screen_recording_guidance(&partial).expect("screen recording advice");
        assert!(text.contains("Only `screenshot` needs it"), "{text}");
        assert!(text.contains("Quit and reopen"), "{text}");
        assert!(text.contains(SCREEN_RECORDING_PANE), "{text}");
        // This block can be printed on its own (accessibility granted), so it
        // must name the app to add rather than point at a block that is absent.
        assert!(text.contains("the app that launched this command"), "{text}");
        assert!(!text.contains("same app as above"), "{text}");
    }

    #[test]
    fn nothing_to_offer_once_both_are_granted() {
        let granted = status(PermissionState::Granted, PermissionState::Granted);
        assert!(screen_recording_guidance(&granted).is_none());
    }

    #[test]
    fn the_error_path_and_the_status_path_agree_on_the_pane() {
        let advice = denied_advice();
        assert!(advice.contains(ACCESSIBILITY_PANE), "{advice}");
        assert!(advice.contains("halo-desktop permissions"), "{advice}");
    }

    #[test]
    fn summary_is_one_line_per_permission() {
        let text = summary(&status(PermissionState::Denied, PermissionState::Unknown));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].starts_with("accessibility: denied"), "{text}");
        assert!(lines[1].starts_with("screen recording: unknown"), "{text}");
    }
}
