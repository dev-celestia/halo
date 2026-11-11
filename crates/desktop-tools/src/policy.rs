//! What an agent is never allowed to touch, whatever ref it holds.
//!
//! This layer exists because the accessibility tree is a *superset* of what the
//! user would hand over. A snapshot of Mail contains message bodies; a snapshot
//! of Passwords contains the list of every account. The engine has no opinion
//! about any of that — it answers what it is asked — so the opinion lives here,
//! above it, and applies to every engine.

use crate::ToolResult;

/// Apps that are never addressable, matched case-insensitively as substrings.
///
/// Substring rather than equality because the display name varies by locale and
/// version ("System Settings" vs "System Preferences", "1Password 8" vs
/// "1Password").
pub const DEFAULT_DENY_APPS: &[&str] = &[
    // Credentials and the vault behind them.
    "passwords",
    "keychain access",
    "1password",
    "bitwarden",
    "lastpass",
    "keepass",
    // The panes that grant permission: an agent must not edit its own leash.
    "system settings",
    "system preferences",
    "securityagent",
    // Terminal surfaces: file and shell work belongs to the shell tool, not to
    // keystrokes typed into someone's live session.
    "terminal",
    "iterm",
    "warp",
    // Halo driving Halo is a feedback loop with no upside.
    "halo",
];

/// Key combos that quit, force-quit or wipe. Normalised before comparison, so
/// `Shift+Cmd+Delete` and `cmd+shift+delete` are the same combo.
pub const DENY_KEYS: &[&str] = &["cmd+q", "cmd+alt+esc", "ctrl+cmd+q", "cmd+shift+delete"];

#[derive(Debug, Clone)]
pub struct Policy {
    deny_apps: Vec<String>,
    deny_keys: Vec<String>,
    /// Whether the user allowed physical input. Both this and the per-run flag
    /// must be on; see [`Policy::check_headed`].
    allow_headed: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            deny_apps: DEFAULT_DENY_APPS.iter().map(|app| app.to_string()).collect(),
            deny_keys: DENY_KEYS.iter().map(|key| key.to_string()).collect(),
            allow_headed: false,
        }
    }
}

impl Policy {
    /// No restrictions. Only for tests and for a future explicit allowlist mode —
    /// never the default.
    pub fn unrestricted() -> Self {
        Self {
            deny_apps: Vec::new(),
            deny_keys: Vec::new(),
            allow_headed: true,
        }
    }

    pub fn with_headed(mut self, allow: bool) -> Self {
        self.allow_headed = allow;
        self
    }

    pub fn deny_apps(&self) -> &[String] {
        &self.deny_apps
    }

    pub fn allow_headed(&self) -> bool {
        self.allow_headed
    }

    /// Refuse an app before the engine is asked for anything.
    pub fn check_app(&self, app: &str) -> ToolResult<()> {
        let needle = app.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Ok(());
        }
        match self
            .deny_apps
            .iter()
            .find(|denied| !denied.is_empty() && needle.contains(denied.as_str()))
        {
            Some(denied) => Err(format!(
                "{app} is blocked by policy ({denied}). Computer use is for apps the user \
                 opens for you; credentials, permission panes and terminal sessions are \
                 off limits — use the shell and file tools for that work."
            )),
            None => Ok(()),
        }
    }

    /// A secure field is never read, never typed into, never echoed.
    pub fn check_role(&self, role: &str) -> ToolResult<()> {
        let role = role.trim().to_ascii_lowercase();
        if role.contains("secure") {
            return Err(
                "That element is a secure text field. Its value is never read and never \
                 typed by an agent; ask the user to fill it."
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Refuse a combo that would quit the app, force-quit, or wipe a volume.
    pub fn check_key(&self, combo: &str) -> ToolResult<()> {
        let wanted = normalize_combo(combo);
        if wanted.is_empty() {
            return Ok(());
        }
        if self
            .deny_keys
            .iter()
            .any(|denied| normalize_combo(denied) == wanted)
        {
            return Err(format!(
                "{combo} is blocked by policy: it quits or force-quits an app, or clears a \
                 volume. Ask the user to do it."
            ));
        }
        Ok(())
    }

    /// Physical input needs the settings switch *and* the per-run flag.
    pub fn check_headed(&self, requested: bool) -> ToolResult<()> {
        if !requested {
            return Ok(());
        }
        if self.allow_headed {
            return Ok(());
        }
        Err(
            "Physical input is off. This action would move the mouse or synthesize keys; \
             the user has to enable \"Allow physical input\" in Settings → Agent → Computer \
             use, and approve it for this run. Headless semantic actions do not need it."
                .to_string(),
        )
    }
}

/// `Shift+Cmd+Delete` and `cmd+shift+delete` are the same combo.
fn normalize_combo(combo: &str) -> String {
    let mut parts: Vec<String> = combo
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect();
    parts.sort();
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_credentials_and_permission_panes() {
        let policy = Policy::default();
        assert!(policy.check_app("Passwords").is_err());
        assert!(policy.check_app("System Settings").is_err());
        assert!(policy.check_app("1Password 8").is_err());
        assert!(policy.check_app("Terminal").is_err());
        // Halo itself, however the window is titled.
        assert!(policy.check_app("Halo").is_err());
    }

    #[test]
    fn allows_ordinary_apps() {
        let policy = Policy::default();
        assert!(policy.check_app("TextEdit").is_ok());
        assert!(policy.check_app("Notes").is_ok());
        assert!(policy.check_app("Finder").is_ok());
        assert!(policy.check_app("").is_ok());
    }

    #[test]
    fn a_secure_field_is_never_addressable() {
        let policy = Policy::default();
        assert!(policy.check_role("AXSecureTextField").is_err());
        assert!(policy.check_role("securetextfield").is_err());
        assert!(policy.check_role("textfield").is_ok());
        assert!(policy.check_role("textarea").is_ok());
    }

    #[test]
    fn denied_keys_match_regardless_of_order_or_case() {
        let policy = Policy::default();
        assert!(policy.check_key("cmd+q").is_err());
        assert!(policy.check_key("Shift+Cmd+Delete").is_err());
        assert!(policy.check_key("CMD+SHIFT+DELETE").is_err());
        assert!(policy.check_key("alt+cmd+esc").is_err());
        assert!(policy.check_key("cmd+c").is_ok());
        assert!(policy.check_key("return").is_ok());
    }

    #[test]
    fn physical_input_needs_both_switches() {
        let off = Policy::default();
        assert!(off.check_headed(false).is_ok());
        assert!(off.check_headed(true).is_err());

        let on = Policy::default().with_headed(true);
        assert!(on.check_headed(true).is_ok());
    }

    #[test]
    fn unrestricted_still_normalises_keys() {
        let policy = Policy::unrestricted();
        assert!(policy.check_app("Passwords").is_ok());
        assert!(policy.check_key("cmd+q").is_ok());
    }
}
