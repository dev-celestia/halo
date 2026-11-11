//! The contract above the engine: policy first, then the call, then the budget.
//!
//! Every front — the CLI today, an MCP server later — sits on this type, so the
//! guard cannot be forgotten by one of them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::policy::Policy;
use crate::types::{
    ActOutcome, ActRequest, EngineStatus, FindRequest, Snapshot, SnapshotRequest, Truncation,
};
use crate::{render, ToolResult, DEFAULT_MAX_CHARS, HARD_MAX_CHARS};

/// The floor a caller may not go below: one rendered element is still useful.
const MIN_MAX_CHARS: usize = 200;

pub struct Desktop<E: Engine> {
    engine: E,
    generation: u64,
    /// Snapshot id printed by the latest snapshot. A ref from any other id is
    /// refused before the engine starts. Cleared after an action changes the
    /// UI, so the old refs cannot be reused.
    snapshot_id: Option<String>,
    default_max_chars: usize,
    policy: Policy,
    /// Where the generation survives between processes. The CLI front is one
    /// process per command: without this the guard would reset to zero and
    /// accept every ref.
    state: Option<PathBuf>,
    /// `status` is a process spawn, and it is asked for on every snapshot to
    /// gate surfaces. Cache it for the life of this facade.
    status: Option<EngineStatus>,
}

impl<E: Engine> Desktop<E> {
    pub fn new(engine: E) -> Self {
        Self {
            engine,
            generation: 0,
            snapshot_id: None,
            default_max_chars: DEFAULT_MAX_CHARS,
            policy: Policy::default(),
            state: None,
            status: None,
        }
    }

    pub fn with_state(mut self, path: PathBuf) -> Self {
        let (generation, snapshot_id) = read_guard(&path);
        self.generation = generation;
        self.snapshot_id = snapshot_id;
        self.state = Some(path);
        self
    }

    pub fn with_budget(mut self, max_chars: usize) -> Self {
        self.default_max_chars = max_chars.clamp(MIN_MAX_CHARS, HARD_MAX_CHARS);
        self
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn engine(&mut self) -> &mut E {
        &mut self.engine
    }

    /// Permissions, version, surfaces. Cached after the first call.
    pub fn status(&mut self) -> ToolResult<EngineStatus> {
        if let Some(status) = &self.status {
            return Ok(status.clone());
        }
        let status = self.engine.status()?;
        self.status = Some(status.clone());
        Ok(status)
    }

    /// Whether the agent can read anything at all right now.
    pub fn can_observe(&mut self) -> ToolResult<bool> {
        Ok(self.status()?.accessibility.is_granted())
    }

    /// Running apps, summarised one per line.
    pub fn apps(&mut self, filter: Option<&str>) -> ToolResult<String> {
        let value = self.engine.apps(filter)?;
        Ok(render::list(&value, self.default_max_chars).0)
    }

    pub fn windows(&mut self, app: Option<&str>) -> ToolResult<String> {
        if let Some(app) = app {
            self.policy.check_app(app)?;
        }
        let value = self.engine.windows(app)?;
        let value = strip_denied(&self.policy, value)?;
        Ok(render::list(&value, self.default_max_chars).0)
    }

    /// Read the tree, then render it inside the budget.
    pub fn snapshot(&mut self, request: &SnapshotRequest) -> ToolResult<Snapshot> {
        if let Some(app) = request.app.as_deref() {
            self.policy.check_app(app)?;
        }
        self.check_surface(request.surface.as_deref())?;

        let raw = self.engine.snapshot_raw(request)?;
        // The frontmost app is only known after the call. A denied one drops
        // the tree and does not move the generation.
        if !raw.app.is_empty() {
            self.policy.check_app(&raw.app)?;
        }
        self.generation += 1;
        self.snapshot_id = (!raw.snapshot_id.is_empty()).then(|| raw.snapshot_id.clone());
        let persisted = self.persist();

        let budget = request
            .max_chars
            .unwrap_or(self.default_max_chars)
            .clamp(MIN_MAX_CHARS, HARD_MAX_CHARS);
        let (body, truncated) = render::tree(&raw.tree, budget);

        let mut text = String::new();
        if !raw.app.is_empty() {
            text.push_str(&format!("app: {}\n", raw.app));
        }
        if !raw.window_title.is_empty() {
            text.push_str(&format!("window: {}\n", raw.window_title));
        }
        text.push_str(&format!("refs: {}\n", raw.ref_count));
        if raw.complete {
            text.push_str("complete: true\n");
        } else {
            // Not an error: the engine hit its own observation budget. Drill
            // into a region instead of retrying and waiting for a timeout.
            let observed = raw
                .nodes_observed
                .map(|count| format!(", nodes_observed: {count}"))
                .unwrap_or_default();
            text.push_str(&format!(
                "complete: false{observed} — the tree was cut; drill with --root\n"
            ));
        }
        if !persisted {
            text.push_str(
                "note: the generation counter could not be saved, so it restarts at 1 on the \
                 next command; the engine's own STALE_REF check still applies\n",
            );
        }
        text.push_str(&body);

        Ok(Snapshot {
            generation: self.generation,
            text,
            complete: raw.complete,
            truncated,
        })
    }

    pub fn find(&mut self, request: &FindRequest) -> ToolResult<String> {
        if let Some(app) = request.app.as_deref() {
            self.policy.check_app(app)?;
        }
        self.check_surface(request.surface.as_deref())?;
        let value = self.engine.find_raw(request)?;
        let value = strip_denied(&self.policy, value)?;
        let (text, truncation) = render::list(&value, self.default_max_chars);
        Ok(with_hint(text, truncation))
    }

    pub fn read(&mut self, reference: &str, property: Option<&str>) -> ToolResult<String> {
        self.guard_ref(reference)?;
        self.refuse_secure(reference)?;
        let value = self.engine.read_raw(reference, property)?;
        let (text, truncation) = render::list(&value, self.default_max_chars);
        Ok(with_hint(text, truncation))
    }

    /// One headless action. A stale generation or an older snapshot id is
    /// refused before [`Engine::perform`] runs.
    pub fn act(&mut self, request: &ActRequest) -> ToolResult<ActOutcome> {
        self.guard_generation(request.generation)?;
        if request.action.needs_ref() {
            let reference = request.reference.as_deref().unwrap_or("");
            self.guard_ref(reference)?;
        }
        if matches!(request.action, crate::types::ActAction::Press) {
            let combo = request.value.as_deref().unwrap_or("");
            self.policy.check_key(combo)?;
        }
        if matches!(request.action, crate::types::ActAction::Type) {
            let reference = request.reference.as_deref().unwrap_or("");
            self.refuse_secure(reference)?;
        }

        let before = request
            .reference
            .as_deref()
            .filter(|_| request.action.needs_ref())
            .and_then(|reference| self.property(reference, "value").ok());
        let performed = self.engine.perform(request)?;
        let (delivery, retry) = disposition(&performed);

        let (mut changed, mut evidence) = match request.action {
            crate::types::ActAction::Type => {
                let wanted = request.value.as_deref().unwrap_or("");
                match self.property(request.reference.as_deref().unwrap_or(""), "value") {
                    Ok(after) => {
                        let changed = before.as_deref() != Some(after.as_str());
                        let evidence = if after == wanted {
                            format!("field confirmed: {after:?}")
                        } else {
                            format!("field reads {after:?}, expected {wanted:?}")
                        };
                        (changed, evidence)
                    }
                    Err(_) => (
                        true,
                        "action sent; the element could not be re-read. Snapshot again before the next ref.".to_string(),
                    ),
                }
            }
            _ => match request
                .reference
                .as_deref()
                .filter(|_| request.action.needs_ref())
            {
                Some(reference) => match self.property(reference, "value") {
                    Ok(after) => {
                        let changed = before.as_deref() != Some(after.as_str());
                        let evidence = if changed {
                            "the element changed after the action".to_string()
                        } else {
                            "no visible change after the action".to_string()
                        };
                        (changed, evidence)
                    }
                    Err(_) => (
                        true,
                        "action sent; the element could not be re-read. Snapshot again before the next ref.".to_string(),
                    ),
                },
                None => (false, "action sent".to_string()),
            },
        };
        if retry.as_deref() == Some("unsafe") {
            evidence.push_str(" Do not repeat this action.");
            changed = true;
        }
        if changed {
            self.generation += 1;
            self.snapshot_id = None;
            self.persist();
        }

        Ok(ActOutcome {
            generation: self.generation,
            changed,
            evidence,
            delivery,
            retry,
        })
    }

    fn guard_generation(&self, generation: u64) -> ToolResult<()> {
        if generation == self.generation {
            return Ok(());
        }
        Err(format!(
            "stale ref: that ref belongs to generation {generation} but the desktop is at \
             generation {}. Take a snapshot and act on the refs it returns.",
            self.generation
        ))
    }

    fn guard_ref(&self, reference: &str) -> ToolResult<()> {
        let Some(current) = self.snapshot_id.as_deref().filter(|id| !id.is_empty()) else {
            return Err(
                "Take a snapshot first, then use a ref from that snapshot.".to_string(),
            );
        };
        match snapshot_id_of(reference) {
            Some(id) if id == current => Ok(()),
            Some(_) => Err(format!(
                "stale ref: {reference} belongs to an older snapshot. Take a fresh snapshot and \
                 use the new ref."
            )),
            None => Err(format!(
                "ref {reference:?} is not a qualified ref (@snapshot:eN)"
            )),
        }
    }

    /// Role is read so the value of a secure field never is.
    fn refuse_secure(&mut self, reference: &str) -> ToolResult<()> {
        let role = self.property(reference, "role")?;
        self.policy.check_role(&role)
    }

    fn property(&mut self, reference: &str, property: &str) -> ToolResult<String> {
        let value = self.engine.read_raw(reference, Some(property))?;
        Ok(json_text(value.get("value").unwrap_or(&value)))
    }

    /// Ask for a surface the engine does not advertise and you get
    /// `PLATFORM_NOT_SUPPORTED` — after a spawn. Check first.
    fn check_surface(&mut self, surface: Option<&str>) -> ToolResult<()> {
        let Some(surface) = surface else {
            return Ok(());
        };
        let status = self.status()?;
        if status.supported_surfaces.is_empty() {
            return Ok(());
        }
        if status
            .supported_surfaces
            .iter()
            .any(|known| known.eq_ignore_ascii_case(surface))
        {
            return Ok(());
        }
        Err(format!(
            "surface {surface:?} is not supported on this platform. Supported: {}.",
            status.supported_surfaces.join(", ")
        ))
    }

    /// Persist the generation. Returns `false` when the cache is not writable —
    /// the guard still works, but only the engine's own `STALE_REF` check is
    /// left, so the caller must say so rather than silently promise a guard it
    /// does not have.
    fn persist(&self) -> bool {
        let Some(path) = &self.state else {
            return true;
        };
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        let payload = GenerationFile {
            generation: self.generation,
            snapshot_id: self.snapshot_id.clone(),
        };
        match serde_json::to_string(&payload) {
            Ok(encoded) => std::fs::write(path, encoded).is_ok(),
            Err(_) => false,
        }
    }
}

fn with_hint(text: String, truncation: Option<Truncation>) -> String {
    match truncation {
        Some(truncation) => format!("{text}\n… [{}]", truncation.hint),
        None => text,
    }
}

#[derive(Serialize, Deserialize)]
struct GenerationFile {
    generation: u64,
    #[serde(default)]
    snapshot_id: Option<String>,
}

fn read_guard(path: &Path) -> (u64, Option<String>) {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<GenerationFile>(&raw).ok())
        .map(|file| {
            (
                file.generation,
                file.snapshot_id.filter(|id| !id.is_empty()),
            )
        })
        .unwrap_or((0, None))
}

fn snapshot_id_of(reference: &str) -> Option<&str> {
    let rest = reference.trim().strip_prefix('@')?;
    let (id, element) = rest.split_once(':')?;
    if id.is_empty() || element.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn json_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn disposition(value: &serde_json::Value) -> (Option<String>, Option<String>) {
    let disposition = value.get("disposition");
    let read = |key: &str| {
        disposition
            .and_then(|item| item.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    (read("delivery"), read("retry"))
}

/// Drop rows for a denied app. A result that is only denied apps is an error,
/// so the tree is never printed.
fn strip_denied(policy: &crate::policy::Policy, value: serde_json::Value) -> ToolResult<serde_json::Value> {
    match value {
        serde_json::Value::Array(items) => filter_items(policy, items).map(serde_json::Value::Array),
        serde_json::Value::Object(mut map) => {
            for key in ["apps", "windows", "matches", "results", "items", "elements"] {
                if let Some(serde_json::Value::Array(items)) = map.remove(key) {
                    map.insert(key.to_string(), filter_items(policy, items)?.into());
                    return Ok(serde_json::Value::Object(map));
                }
            }
            if let Some(app) = map.get("app").and_then(serde_json::Value::as_str) {
                policy.check_app(app)?;
            }
            if let Some(inner) = map.get("match") {
                if let Some(app) = inner.get("app").and_then(serde_json::Value::as_str) {
                    policy.check_app(app)?;
                }
            }
            Ok(serde_json::Value::Object(map))
        }
        other => Ok(other),
    }
}

fn filter_items(
    policy: &crate::policy::Policy,
    items: Vec<serde_json::Value>,
) -> ToolResult<Vec<serde_json::Value>> {
    let mut kept = Vec::new();
    let mut denial = None;
    for item in items {
        if let Some(app) = item.get("app").and_then(serde_json::Value::as_str) {
            if let Err(error) = policy.check_app(app) {
                denial = Some(error);
                continue;
            }
        }
        kept.push(item);
    }
    if kept.is_empty() {
        if let Some(error) = denial {
            return Err(error);
        }
    }
    Ok(kept)
}

/// Halo's private root for the engine's sessions. Never `~/.agent-desktop`.
pub fn engine_home() -> PathBuf {
    cache_dir().join("engine")
}

/// Halo's own cache root for desktop state. Never `~/.agent-desktop`: that is
/// the user's, and mixing our sessions into it would make `session gc` the
/// user's problem.
pub fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("HALO_DESKTOP_CACHE_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let standard = home.join("Library/Caches/Halo/desktop");
    if std::fs::create_dir_all(&standard).is_ok() {
        return standard;
    }
    // Fallback if ~/Library/Caches/Halo is root-owned or not writable
    let user_cache = home.join(".cache/halo-desktop");
    if std::fs::create_dir_all(&user_cache).is_ok() {
        return user_cache;
    }
    PathBuf::from("/tmp/halo-desktop")
}

/// Per-session generation file, so two chats do not share a counter.
pub fn state_path(session: &str) -> PathBuf {
    cache_dir().join(format!("generation-{session}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::types::{ActAction, ActRequest, FindRequest, RawSnapshot, SnapshotRequest};
    use crate::ToolResult;
    use serde_json::{json, Value};

    #[derive(Default)]
    struct Fake {
        app: String,
        snapshot_id: String,
        role: String,
        value: String,
        snapshots: u32,
        performs: Vec<String>,
        reads: Vec<String>,
    }

    impl Engine for Fake {
        fn status(&mut self) -> ToolResult<crate::types::EngineStatus> {
            Ok(crate::types::EngineStatus::default())
        }

        fn apps(&mut self, _: Option<&str>) -> ToolResult<Value> {
            Ok(Value::Null)
        }

        fn windows(&mut self, _: Option<&str>) -> ToolResult<Value> {
            Ok(json!([{"app": self.app}]))
        }

        fn snapshot_raw(&mut self, _: &SnapshotRequest) -> ToolResult<RawSnapshot> {
            self.snapshots += 1;
            Ok(RawSnapshot {
                app: self.app.clone(),
                snapshot_id: self.snapshot_id.clone(),
                complete: true,
                tree: json!({"role": "window", "name": "Doc"}),
                ..RawSnapshot::default()
            })
        }

        fn find_raw(&mut self, _: &FindRequest) -> ToolResult<Value> {
            Ok(Value::Null)
        }

        fn read_raw(&mut self, _: &str, property: Option<&str>) -> ToolResult<Value> {
            let property = property.unwrap_or("text");
            self.reads.push(property.to_string());
            let value = if property == "role" {
                self.role.clone()
            } else {
                self.value.clone()
            };
            Ok(json!({"property": property, "value": value}))
        }

        fn perform(&mut self, request: &ActRequest) -> ToolResult<Value> {
            self.performs.push(request.action.engine_name().to_string());
            Ok(json!({}))
        }
    }

    fn textedit() -> Fake {
        Fake {
            app: "TextEdit".into(),
            snapshot_id: "abc".into(),
            role: "textarea".into(),
            ..Fake::default()
        }
    }

    #[test]
    fn a_stale_generation_does_not_spawn_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("generation.json");
        std::fs::write(&path, r#"{"generation":4,"snapshot_id":"abc"}"#).unwrap();
        let mut desktop = Desktop::new(Fake::default()).with_state(path);

        let error = desktop
            .act(&ActRequest {
                reference: Some("@abc:e1".into()),
                generation: 1,
                action: ActAction::Click,
                value: None,
            })
            .expect_err("old generation");

        assert!(error.contains("stale"), "{error}");
        assert_eq!(desktop.engine().snapshots, 0);
        assert!(desktop.engine().performs.is_empty());
    }

    #[test]
    fn an_older_snapshot_id_does_not_spawn_a_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("generation.json");
        std::fs::write(&path, r#"{"generation":1,"snapshot_id":"current"}"#).unwrap();
        let mut desktop = Desktop::new(Fake::default()).with_state(path);

        let error = desktop
            .read("@previous:e1", Some("value"))
            .expect_err("old snapshot");

        assert!(error.contains("stale"), "{error}");
        assert!(desktop.engine().reads.is_empty());
    }

    #[test]
    fn a_denied_frontmost_app_drops_the_tree() {
        let mut fake = Fake::default();
        fake.app = "Passwords".into();
        fake.snapshot_id = "secret".into();
        let mut desktop = Desktop::new(fake);

        let error = desktop
            .snapshot(&SnapshotRequest::default())
            .expect_err("passwords");

        assert!(error.contains("blocked"), "{error}");
        assert!(!error.contains("Doc"), "{error}");
        assert_eq!(desktop.generation(), 0);
    }

    #[test]
    fn a_named_denied_app_is_refused_before_spawn() {
        let mut desktop = Desktop::new(textedit());
        let error = desktop
            .snapshot(&SnapshotRequest {
                app: Some("Passwords".into()),
                ..SnapshotRequest::default()
            })
            .expect_err("named passwords");
        assert!(error.contains("blocked"), "{error}");
        assert_eq!(desktop.engine().snapshots, 0);
    }

    #[test]
    fn a_secure_field_is_not_read() {
        let mut fake = textedit();
        fake.role = "AXSecureTextField".into();
        fake.value = "secret".into();
        let mut desktop = Desktop::new(fake);
        desktop.snapshot(&SnapshotRequest::default()).unwrap();
        desktop.engine().reads.clear();

        let error = desktop
            .read("@abc:e1", Some("value"))
            .expect_err("secure");

        assert!(error.contains("secure"), "{error}");
        assert!(
            !desktop.engine().reads.iter().any(|read| read == "value"),
            "{:?}",
            desktop.engine().reads
        );
    }

    #[test]
    fn a_quit_combo_is_refused_before_spawn() {
        let mut desktop = Desktop::new(Fake::default());
        let error = desktop
            .act(&ActRequest {
                reference: None,
                generation: 0,
                action: ActAction::Press,
                value: Some("cmd+q".into()),
            })
            .expect_err("quit");
        assert!(error.contains("blocked"), "{error}");
        assert!(desktop.engine().performs.is_empty());
    }
}
