//! The contract: generation, budget, verify. Engine-agnostic.

use std::path::PathBuf;

use crate::engine::Engine;
use crate::types::{
    ActOutcome, ActRequest, Action, Fingerprint, NavOutcome, Snapshot, SnapshotRequest, Truncation,
};
use crate::ToolResult;

/// Default per-call ceiling. A github.com home page is ~7k chars, a Wikipedia
/// article ~71k — 4k keeps a loop affordable without hiding the structure.
pub const DEFAULT_MAX_CHARS: usize = 4000;

/// No caller may exceed this, whatever it asks for.
pub const HARD_MAX_CHARS: usize = 12_000;

pub struct Browser<E: Engine> {
    engine: E,
    generation: u64,
    default_max_chars: usize,
    /// Empty means "any host". See [`crate::policy`].
    allowed_domains: Vec<String>,
    /// Where the generation survives between processes. The MCP front is
    /// long-lived, but the CLI front is one process per command: without this
    /// the guard would reset to zero and accept every ref.
    state: Option<PathBuf>,
}

impl<E: Engine> Browser<E> {
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            generation: 0,
            default_max_chars: DEFAULT_MAX_CHARS,
            allowed_domains: Vec::new(),
            state: None,
        }
    }

    /// Keep the generation across process restarts.
    pub fn with_state(mut self, path: PathBuf) -> Self {
        self.state = Some(path);
        self.generation = read_generation(&self.state);
        self
    }

    /// Restrict navigation to these hosts. Empty allows any host.
    pub fn with_domains(mut self, domains: Vec<String>) -> Self {
        self.allowed_domains = domains;
        self
    }

    pub fn with_budget(mut self, max_chars: usize) -> Self {
        self.default_max_chars = max_chars.clamp(500, HARD_MAX_CHARS);
        self
    }

    pub fn engine(&self) -> &E {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut E {
        &mut self.engine
    }

    /// Current generation. Every ref the agent holds belongs to this number.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn open(&mut self, url: &str) -> ToolResult<NavOutcome> {
        let url = normalize_url(url);
        // Before the engine sees it: a refused scheme must not reach Chrome.
        crate::policy::check_url(&url, &self.allowed_domains)?;
        self.engine.navigate(&url)?;
        self.generation += 1;
        self.persist();
        Ok(NavOutcome {
            generation: self.generation,
            url: self.engine.url()?,
            title: self.engine.title()?,
        })
    }

    pub fn snapshot(&mut self, request: &SnapshotRequest) -> ToolResult<Snapshot> {
        let interactive = request.interactive.unwrap_or(true);
        let raw = self
            .engine
            .snapshot_raw(request.scope.as_deref(), interactive)?;
        self.generation += 1;
        self.persist();

        let cap = request
            .max_chars
            .unwrap_or(self.default_max_chars)
            .min(HARD_MAX_CHARS);
        let (text, truncated) = truncate(&raw, cap);
        Ok(Snapshot {
            generation: self.generation,
            text,
            truncated,
        })
    }

    /// Dispatch one action and report whether the page actually moved.
    ///
    /// A ref from an older generation is rejected rather than attempted: the
    /// alternative is clicking whatever now happens to hold that number.
    pub fn act(&mut self, request: &ActRequest) -> ToolResult<ActOutcome> {
        if request.generation != self.generation {
            return Err(format!(
                "stale ref: that ref belongs to generation {} but the page is at generation {}. \
                 Call browser_snapshot and act on the refs it returns.",
                request.generation, self.generation
            ));
        }

        let before = self.fingerprint()?;
        let mut evidence = String::new();
        match request.action {
            Action::Click => {
                self.engine.click(&require_ref(&request.reference)?)?;
            }
            Action::Fill => {
                let reference = require_ref(&request.reference)?;
                let wanted = request.value.as_deref().unwrap_or("");
                self.engine.fill(&reference, wanted)?;
                // Body text never contains an input's value, so read it back.
                let readback = self.engine.value(&reference).unwrap_or_default();
                evidence = if readback == wanted {
                    format!("field confirmed: {readback:?}")
                } else {
                    format!("field reads {readback:?}, expected {wanted:?}")
                };
            }
            Action::Press => {
                if !request.reference.trim().is_empty() {
                    self.engine.focus(&require_ref(&request.reference)?)?;
                }
                self.engine
                    .press(request.value.as_deref().unwrap_or("Enter"))?;
            }
        }

        let after = self.fingerprint()?;
        let changed = before != after;
        // The verify doubles as the generation gate: an unchanged page leaves the
        // refs valid, so a click that hit nothing does not cost a re-snapshot.
        if changed {
            self.generation += 1;
            self.persist();
        }
        if evidence.is_empty() {
            evidence = if changed {
                "page moved after the action".to_string()
            } else {
                "no visible change after the action".to_string()
            };
        }

        Ok(ActOutcome {
            generation: self.generation,
            changed,
            url: after.url,
            title: after.title,
            note: if changed {
                "page changed; snapshot again before the next ref".to_string()
            } else {
                "page unchanged; the refs from the last snapshot are still current".to_string()
            },
            evidence,
        })
    }

    pub fn read(&mut self, selector: Option<&str>, max_chars: Option<usize>) -> ToolResult<String> {
        let raw = self.engine.text(selector)?;
        let cap = max_chars.unwrap_or(self.default_max_chars).min(HARD_MAX_CHARS);
        let (text, _) = truncate(&raw, cap);
        Ok(text)
    }

    pub fn close(&mut self) -> ToolResult<()> {
        self.engine.close()
    }

    fn persist(&self) {
        let Some(path) = self.state.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, self.generation.to_string());
    }

    fn fingerprint(&mut self) -> ToolResult<Fingerprint> {
        let url = self.engine.url()?;
        let title = self.engine.title()?;
        let text = self.engine.text(None)?;
        Ok(Fingerprint {
            url,
            title,
            text_hash: Fingerprint::hash_text(&text),
        })
    }
}

/// A session name reduced to something safe as a path segment or a flag value.
pub fn sanitize_session(session: &str) -> String {
    let safe: String = session
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let safe = safe.trim_matches('-');
    if safe.is_empty() {
        "halo".to_string()
    } else {
        safe.chars().take(48).collect()
    }
}

/// True when `dir` exists and this process may create files inside it.
///
/// `create_dir_all` is not a writability test: it returns `Ok` whenever the
/// path already exists as a directory, so a root-owned leftover from a
/// privileged run looks perfectly healthy and every later write fails. The
/// probe has to actually write.
fn is_writable_dir(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    if std::fs::write(&probe, b"1").is_err() {
        return false;
    }
    let _ = std::fs::remove_file(&probe);
    true
}

/// Halo's browser cache: generation counters, per-chat profiles, downloads.
///
/// A guard that cannot persist silently resets to zero, which is worse than
/// having none, so an unwritable `~/Library/Caches` falls back to the temp dir.
pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    let preferred = home.join("Library/Caches/Halo/browser");
    #[cfg(not(target_os = "macos"))]
    let preferred = home.join(".cache/halo/browser");

    if is_writable_dir(&preferred) {
        preferred
    } else {
        let fallback = std::env::temp_dir().join("halo-browser");
        let _ = std::fs::create_dir_all(&fallback);
        fallback
    }
}

/// Where a session's generation lives between processes.
pub fn state_path(session: &str) -> PathBuf {
    cache_dir().join(format!("{}.generation", sanitize_session(session)))
}

/// A session's Chrome user-data-dir.
///
/// Per chat, so one chat's cookies are not another's, and under Halo's cache
/// rather than agent-browser's own default — which is what lets a deleted chat
/// take its logged-in state with it.
pub fn profile_dir(session: &str) -> PathBuf {
    cache_dir().join("profiles").join(sanitize_session(session))
}

/// Where a session's downloads land, so they never arrive in `~/Downloads`.
pub fn download_dir(session: &str) -> PathBuf {
    cache_dir().join("downloads").join(sanitize_session(session))
}

fn read_generation(state: &Option<PathBuf>) -> u64 {
    state
        .as_ref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

fn require_ref(reference: &str) -> ToolResult<String> {    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return Err("missing ref: pass the @eN value from the last snapshot".into());
    }
    Ok(if trimmed.starts_with('@') {
        trimmed.to_string()
    } else {
        format!("@{trimmed}")
    })
}

/// Bare hosts are the common case from a model; give them a scheme.
///
/// Anything that already carries a scheme is left alone — including the ones
/// the policy will refuse, which must stay recognisable as `file:` rather than
/// being rewritten into `https://file:…`.
fn normalize_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.contains("://") || crate::policy::scheme_of(trimmed).is_some() {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// Cut on a line boundary and say so. A silent cut is how a model concludes the
/// page is empty.
fn truncate(raw: &str, cap: usize) -> (String, Option<Truncation>) {
    let total = raw.chars().count();
    if total <= cap {
        return (raw.to_string(), None);
    }

    let mut kept = String::new();
    let mut used = 0usize;
    for line in raw.lines() {
        let width = line.chars().count() + 1;
        if used + width > cap {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
        used += width;
    }

    let kept_chars = kept.chars().count();
    let dropped_chars = total.saturating_sub(kept_chars);
    let hint = format!(
        "truncated {dropped_chars} chars; narrow with scope (a CSS selector) or raise max_chars (hard cap {HARD_MAX_CHARS})"
    );
    kept.push_str(&format!("… [{hint}]"));
    (
        kept,
        Some(Truncation {
            kept_chars,
            dropped_chars,
            hint,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Mock {
        tree: String,
        body: String,
        url: String,
        clicks: Vec<String>,
    }

    impl Engine for Mock {
        fn navigate(&mut self, url: &str) -> ToolResult<()> {
            self.url = url.to_string();
            Ok(())
        }
        fn snapshot_raw(&mut self, _scope: Option<&str>, _i: bool) -> ToolResult<String> {
            Ok(self.tree.clone())
        }
        fn click(&mut self, reference: &str) -> ToolResult<()> {
            self.clicks.push(reference.to_string());
            self.body.push_str("changed");
            Ok(())
        }
        fn fill(&mut self, _r: &str, value: &str) -> ToolResult<()> {
            self.body.push_str(value);
            Ok(())
        }
        fn value(&mut self, _r: &str) -> ToolResult<String> {
            Ok(self.body.clone())
        }
        fn focus(&mut self, _r: &str) -> ToolResult<()> {
            Ok(())
        }
        fn press(&mut self, _k: &str) -> ToolResult<()> {
            Ok(())
        }
        fn text(&mut self, _s: Option<&str>) -> ToolResult<String> {
            Ok(self.body.clone())
        }
        fn url(&mut self) -> ToolResult<String> {
            Ok(self.url.clone())
        }
        fn title(&mut self) -> ToolResult<String> {
            Ok("mock".into())
        }
        fn close(&mut self) -> ToolResult<()> {
            Ok(())
        }
    }

    fn browser() -> Browser<Mock> {
        let mut mock = Mock {
            tree: "- link \"Search\" [ref=e29]\n".into(),
            ..Default::default()
        };
        mock.url = "https://example.com/".into();
        Browser::new(mock)
    }

    #[test]
    fn stale_ref_is_rejected() {
        let mut b = browser();
        let snap = b.snapshot(&SnapshotRequest::default()).unwrap();
        // A second snapshot moves the generation on, orphaning the first ref.
        b.snapshot(&SnapshotRequest::default()).unwrap();

        let err = b
            .act(&ActRequest {
                reference: "@e29".into(),
                generation: snap.generation,
                action: Action::Click,
                value: None,
            })
            .unwrap_err();
        assert!(err.contains("stale ref"), "{err}");
        assert!(b.engine().clicks.is_empty());
    }

    #[test]
    fn unchanged_action_keeps_generation() {
        let mut mock = Mock::default();
        mock.url = "https://example.com/".into();
        // click() mutates the body, so use press, which does not.
        let mut b = Browser::new(mock);
        let snap = b.snapshot(&SnapshotRequest::default()).unwrap();
        let out = b
            .act(&ActRequest {
                reference: "@e1".into(),
                generation: snap.generation,
                action: Action::Press,
                value: Some("Tab".into()),
            })
            .unwrap();
        assert!(!out.changed);
        assert_eq!(out.generation, snap.generation);
    }

    #[test]
    fn changed_action_advances_generation() {
        let mut b = browser();
        let snap = b.snapshot(&SnapshotRequest::default()).unwrap();
        let out = b
            .act(&ActRequest {
                reference: "e29".into(),
                generation: snap.generation,
                action: Action::Click,
                value: None,
            })
            .unwrap();
        assert!(out.changed);
        assert_eq!(out.generation, snap.generation + 1);
        assert_eq!(b.engine().clicks, vec!["@e29".to_string()]);
    }

    #[test]
    fn truncation_is_labelled() {
        let long = "x".repeat(500);
        let (text, cut) = truncate(&long, 100);
        assert!(text.chars().count() < 400);
        assert!(cut.is_some());
        assert!(text.contains("truncated"));
    }

    #[test]
    fn bare_host_gets_a_scheme() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(normalize_url("http://a.b"), "http://a.b");
        // A refused scheme must survive normalisation intact.
        assert_eq!(normalize_url("file:///etc/hosts"), "file:///etc/hosts");
        assert_eq!(normalize_url("javascript:alert(1)"), "javascript:alert(1)");
        assert_eq!(normalize_url("localhost:3000/x"), "https://localhost:3000/x");
    }

    #[test]
    fn open_refuses_a_local_file() {
        let mut b = browser();
        let error = b.open("file:///etc/hosts").unwrap_err();
        assert!(error.contains("refusing to open file"), "{error}");
        // The engine never ran: nothing navigated.
        assert_eq!(b.engine().url, "https://example.com/");
    }

    #[test]
    fn open_refuses_a_host_outside_the_allowlist() {
        let mut b = browser().with_domains(vec!["example.com".into()]);
        assert!(b.open("https://www.example.com/a").is_ok());
        let error = b.open("https://evil.test/").unwrap_err();
        assert!(error.contains("not in the allowed domains"), "{error}");
    }

    /// A directory that exists but rejects writes must not count as usable.
    ///
    /// This is the leftover-from-root case: `create_dir_all` returns `Ok` for
    /// an existing root-owned directory, so the old check picked it and every
    /// later write failed. Skip the negative half when the test user is root —
    /// root can write anywhere, so the premise cannot be built.
    #[test]
    fn an_existing_but_unwritable_dir_is_not_writable() {
        let base = std::env::temp_dir().join(format!(
            "halo-browser-cache-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let locked = base.join("locked");
        std::fs::create_dir_all(&locked).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
            let writable = is_writable_dir(&locked);
            // Restore so cleanup can descend into it.
            let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700));
            if !nix_is_root() {
                assert!(!writable, "a 0500 directory must not be reported writable");
            }
        }

        // A missing directory is created and therefore usable.
        assert!(is_writable_dir(&base.join("fresh")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    fn nix_is_root() -> bool {
        // Root ignores permission bits, so the assertion above is meaningless.
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }
}
