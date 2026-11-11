# Plan: Computer Use via `agent-desktop`

Status: proposal · Owner: Halo agent surface · Scope: macOS first
P0: contract pinned (§7); live half blocked on the Accessibility grant.

## 1. Goal

Let a Halo agent see and operate native desktop apps — read an accessibility
tree, click, type, select, scroll — through `agent-desktop`, with the same
guard rails Halo already gives the browser:

- one wrapper CLI the agent is told to use, never the raw engine;
- stale-ref protection and a hard output budget;
- verify-after-act, not "the call returned OK";
- explicit, default-off consent for anything that touches the real pointer or
  keyboard;
- a settings row and a doctor entry the user can reason about.

Non-goals for v1: remote/VNC desktops, vision-only (screenshot) driving,
Windows/Linux parity, letting agents automate Halo itself.

## 2. What exists today

| Piece | State |
| --- | --- |
| `agent-desktop` binary | Installed by the user (`/usr/local/bin/agent-desktop`, v1.0.2, envelope `version: "2.5"`). Detected, not used. Contract pinned in §7. |
| Doctor probe | `crates/bridge/src/bridge/doctor_probes.rs:183` (`browser:agent-desktop`) + install action in `doctor.rs:609`. |
| Agent prompt | Nothing. Agents are never told the tool exists. |
| Wrapper CLI | Nothing. |
| Permissions | Not checked anywhere; Accessibility / Screen Recording are `denied` on this machine right now. |
| Settings | No "Computer use" section; only Browser tools. |

`agent-desktop` contract we must respect (from `agent-desktop skills get desktop`):

- One JSON envelope per call: `{ok, data|error, version}`; errors carry
  `code`, `disposition {delivery, retry}`.
- Refs look like `@s8f3k2p9:e12` and are valid for one snapshot; acting on an
  old one returns `STALE_REF`.
- Semantic (headless) actions are the default and take no focus. `--headed`
  sends physical input and is required for double-click, hover and drag.
- Single-instance socket; one runtime cache. Two concurrent drivers conflict.
- Read-only commands (`snapshot`, `find`, `get`, `is`, `list-*`) need
  Accessibility. `screenshot` additionally needs Screen Recording.
- `permissions --guide` opens the right System Settings panes; only the user
  can complete the grant.

## 3. Design

### 3.1 One new crate: `halo-desktop-tools`

Mirrors `halo-browser-tools`: a lib that owns the contract, plus a bundled
binary the harness is told to run.

```
crates/desktop-tools/
├── Cargo.toml                 # lib halo_desktop_tools + bin halo-desktop
├── src/lib.rs                 # ToolResult, re-exports
├── src/engine.rs              # trait Desktop: apps/windows/snapshot/act/...
├── src/desktop.rs             # agent-facing semantics: budget, generation, verify
├── src/policy.rs              # app denylist, secure fields, denied keys, headed gate
├── src/lease.rs               # single-writer lease over the engine socket
├── src/agent_desktop/mod.rs   # argv shapes, session env, JSON envelope parsing
├── src/agent_desktop/exec.rs
├── src/agent_desktop/probe.rs # resolve_bin/resolve_cli/ready/version
└── src/bin/halo-desktop.rs    # CLI front (clap), same shape as halo-browser
```

`halo-browser-tools` stays untouched; the shared ideas (budget, generation,
verify) are copied deliberately rather than generalised — the browser's
fingerprint is URL + title + body hash, the desktop's is app + window title +
focused element + subtree hash.

### 3.2 Agent-facing commands

```
halo-desktop status
halo-desktop apps [--app NAME]
halo-desktop windows [--app NAME]
halo-desktop snapshot [--app NAME] [--window-id ID] [--surface KIND] [--skeleton]
                      [--root REF] [-i] [--compact] [--max-depth N] [--timeout-ms MS]
halo-desktop find --app NAME [--role R] [--name N] [--text T] [--exact] [--root REF] [--first]
halo-desktop read --ref REF [--property text|value|title|bounds|role|states]
halo-desktop act --ref REF --gen N --action click|type|press|select|scroll|focus [--value V]
halo-desktop screenshot --app NAME --out PATH      # Screen Recording + opt-in
halo-desktop close [--app NAME | --all]            # only what this lease launched
```

Rules encoded in the wrapper (not left to the model):

- **Generation.** Every `snapshot`/`find` prints `generation: N` and the wrapper
  persists it per lease. `act` refuses a ref from an older generation *before*
  spawning the engine, so a stale ref never reaches the OS.
- **Budget.** Default 4 000 chars (hard cap 16 000), truncated on a line
  boundary with an explicit "narrow with --root/--app" hint. AX trees are
  larger than DOMs; `--skeleton` returns `children_count` on cut branches.
- **Verify.** After an act, re-read the target (or focused element) and print
  `evidence` + `changed`, using the fingerprint above. `disposition` from the
  engine is passed through so the model can see `delivery: not_delivered`
  versus `unsafe`.
- **No auto-fallback.** A headless failure never retries with `--headed`.
  Headed requires `--headed` *and* the user's physical-input consent.
- **Refuse `PERM_DENIED` loudly**: exit non-zero with the exact System Settings
  pane and a pointer to Halo → Settings → Agent → Computer use.

### 3.3 Security and consent model

Four independent gates; all must be open:

1. **Master switch** (Settings → Agent → Computer use), default **off**. While
   off, the prompt block is not injected and `halo-desktop` refuses.
2. **macOS TCC**: Accessibility for read/act; Screen Recording only for
   `screenshot`. Halo only reports state and offers "Open permission guide"
   (runs `agent-desktop permissions --guide` on a user click).
3. **Policy** (`policy.rs`):
   - denylist always wins: Passwords/Keychain Access, System Settings privacy
     panes, Halo itself, security agents, the user's password managers;
   - Text/terminal surfaces (Terminal, iTerm, Warp) denied by default — file
     and shell work belongs to the existing shell/read/write tools, not to
     keystrokes in someone's session;
   - secure text fields (`AXSecureTextField`): never read, never type, never
     echo — even if a ref is addressable;
   - denied shortcuts: `cmd+q`, `cmd+alt+esc`, `ctrl+cmd+q`, `cmd+shift+delete`
     and anything else that quits/force-quits or clears a volume;
   - clicking a control that opens a system auth dialog is refused.
4. **Physical input** (`--headed`): per-run consent surfaced in the approval
   card ("this run can move your mouse and type"), plus a settings sub-toggle.
   Headless semantic actions remain the default forever.

Every acted-on ref is recorded in the transcript as a `desktop` card
(app, window, action, target name, result) so the user can audit a run after
the fact — the same reason the browser has a live pane.

### 3.4 Concurrency: one writer

Corrected after the P0 spike (§7.4): there is **no single instance socket**.
Each session owns its own refstore (`sessions/<id>/refstore.lock`, `liveness/`),
so headless semantic actions from two sessions do not corrupt each other's refs.
What actually collides is *the user's attention*: `--headed` input, focus, and
two chats driving the same app.

So the lease is a **UX guard, not a correctness requirement**:

- `lease.rs` takes an exclusive file lock under
  `~/Library/Caches/Halo/desktop/lease.json` containing `{chat_id, session_id, pid, since}`;
- a second chat gets `DESKTOP_BUSY: held by chat …` instead of racing the same window;
- release on `close`, chat deletion, run cancel and app quit (with a short
  deadline, like `halo_agent_browser_close_all_blocking`);
- a stale lease (pid gone, or older than the max run time) is reclaimed after
  an explicit check, never on a timer alone;
- the lease carries the `session_id`, because a session-owned ref only resolves
  under the same `--session` scope. Losing the session id loses every ref.

### 3.5 Harness and prompt wiring

Follow the browser's pattern exactly (`crates/harness/src/browser.rs`):

- `crates/harness/src/desktop.rs` — `cli()`, `instructions()`,
  `prompt_suffix()`, gated on `ready()` (CLI present + engine present).
- `crates/harness/src/desktop_prompt.md` — the loop, in the browser prompt's
  voice: `status → snapshot --skeleton → find/snapshot --root → act --gen →
  verify`, plus "do not drive `agent-desktop` directly".
- Permission state is passed in at run start (one `agent-desktop status` call,
  cached). If denied, the block says so and tells the agent to ask the user to
  enable it — the browser precedent of hiding an unusable tool would hide the
  one action that fixes it.
- `crates/coder/src/agent/mod.rs`: append `desktop::prompt_suffix()` next to
  `browser::prompt_suffix()` for OpenCode/Antigravity; add it to Pi's system
  prompt in `crates/harness/src/pi/run.rs` (next to line 229).
- Inject `HALO_DESKTOP_LEASE=<chat_id>` alongside `HALO_BROWSER_SESSION`.
- Automation agent nodes get the same block (they run through `halo_harness`),
  but only when the automation's run policy marks desktop use allowed; the
  approval summary must show it.

### 3.6 UI

Settings → Agent, after "Browser tools":

```
Computer use                                [ Off ]
Read and operate desktop apps through the accessibility tree.
Engine: agent-desktop 1.0.2  ✓
Accessibility: denied        [ Open permission guide ]
Screen Recording: denied     [ Open permission guide ]   (screenshots only)
[ ] Allow physical input (mouse and keyboard)
Blocked apps: Passwords, System Settings, Terminal, …
[ Close desktop session ]
```

Chat surface: a small "Controlling <App>" strip with **Stop** while a lease is
live, plus the `desktop` transcript card. No new nav page in v1 (there is no
live video to show).

### 3.7 Doctor

Extend the existing `browser:agent-desktop` item: keep the install probe, add
`permissions` to the status detail and a second action "Open permission guide"
(for both Accessibility and Screen Recording). Do not auto-install anything
else; do not request TCC silently.

## 4. Delivery phases

| Phase | Work | Exit criteria |
| --- | --- | --- |
| **P0 — contract spike** (½ day) | Exercise `agent-desktop` against TextEdit/Notes/Finder: snapshot sizes, `--skeleton`, `STALE_REF`, secure-field behaviour, permission-denied output, second-driver behaviour. Write findings into this doc. | Facts pinned; no Halo code yet. |
| **P1 — observe only** (1–2 days) | `desktop-tools` crate with `status/apps/windows/snapshot/find/read`; budget + generation; prompt block; settings row (read-only) + doctor permission detail; lease. | Agent can say what is on screen and name a control, with no input, on a clean account. |
| **P2 — semantic actions** (2–3 days) | `act` for click/type/press/select/scroll/focus; verify-after-act; policy (denylist, secure fields, denied keys); transcript card; `DESKTOP_BUSY`; stop/quit teardown. | Fill a TextEdit document and toggle a checkbox in a test app; stale ref and denied app both fail closed. |
| **P3 — physical input + screenshots** (1–2 days) | `--headed` behind consent; double-click/hover/drag; `screenshot` into a Halo cache dir (and optionally as an image to the model); Screen Recording row. | Physical input never fires without both the settings toggle and per-run consent. |
| **P4 — automations + polish** (1 day) | Automation agent-node prompt + run-policy flag; docs; QA matrix; error copy pass. | An approved automation can use computer use; an unapproved one cannot. |

## 5. Testing

Unit (no OS calls):

- generation rejects an older ref before spawn;
- budget truncates on a line boundary and names the narrowing flag;
- policy: denylist precedence, secure-field refusal, denied key combos,
  headed gate requires both switches;
- lease: second holder → `DESKTOP_BUSY`, stale pid reclaimed, clean release.

Integration (macOS runner, real engine):

- TextEdit: snapshot → type → read back → verify `changed`;
- Finder: window list with no window open → `WINDOW_NOT_FOUND` guidance;
- permission-denied path with Accessibility revoked → actionable error, no
  partial action;
- two chats racing → exactly one lease, other refuses;
- secret field: snapshot shows no value, typing refused.

Manual matrix: macOS 14/15/26, Intel + Apple Silicon, Retina + scaled external
display, VoiceOver on, two Spaces/full-screen app, app with an empty AX tree
(a game or Java app) → clear failure text.

## 6. Risks and open questions

- **TCC friction.** Every rebuilt dev binary may need re-approval; sign with a
  stable identity, and let the doctor explain "granted but stale build".
- **Single instance.** A user running `agent-desktop` from another tool (or
  another Halo window) collides. Detect and surface rather than silently
  sharing the cursor.
- **Privacy.** The AX tree can contain message text and document contents.
  Budget + denylist + secure-field refusal reduce exposure; screenshots are
  opt-in. Decide whether snapshots should be redacted by default or only
  blocked per app.
- **Prompt injection via screen content.** Text on screen is untrusted input.
  The prompt must say so explicitly, and actions with side effects stay behind
  the existing approval mode.
- **Automation runs.** Unattended computer use is the riskiest mode. Proposal:
  allowed only when the automation revision explicitly lists `desktop` in its
  run policy, and the approval summary names the permitted apps.
- Open: per-app allowlist UI in v1, or a single global toggle with a fixed
  denylist? (Recommendation: global toggle + fixed denylist in v1, allowlist in
  v2.)
- Open: should the agent be allowed to `launch` apps, or only operate apps the
  user already has open? (Recommendation: launch only from an explicit
  user-approved list.)
- Open: ship screenshots to the model as images in P3, or keep them on disk for
  the user? (Recommendation: on disk first; images behind an opt-in.)

## 7. P0 findings — contract spike (2026-11-02)

Environment: macOS, `agent-desktop 1.0.2`, envelope `version: "2.5"`.
Binary is a symlink: `/usr/local/bin/agent-desktop -> ../lib/node_modules/agent-desktop/bin/agent-desktop`.

### 7.1 Envelope and exit codes (verified)

- One JSON object per call on **stdout**. stderr stays empty on failure
  (`--verbose` adds debug logging to stderr).
- Shape: `{"version":"2.5","ok":true|false,"command":"<name>","data"|"error":{…}}`.
  Field order is **not** stable — parse by key, never by position.
- `command` is the top-level command name (`session` for every `session …` subcommand).
- Exit codes, repeated 3× per case and stable: `0` success · `1` runtime error
  (`PERM_DENIED`, `TIMEOUT`) · `2` argument/validation error (`INVALID_ARGS`).
- ⇒ the wrapper must branch on the parsed envelope; the exit code is only a cheap
  fast path. Never read stderr.
- `error` always carries `code`, `message`, `disposition {delivery, retry}`.
  `INVALID_ARGS` carries `not_delivered` / `safe`.

### 7.2 Permissions as observed

`status` → `accessibility: denied`, `screen_recording: denied`, `automation: unknown`.

Works with Accessibility denied: `status`, `permissions`, `version`, `skills get`,
`list-displays`, `session *`, `trace *`.
Fails `PERM_DENIED` (exit 1): `snapshot`, `find`, `list-windows`, `list-surfaces`,
and every ref action.

⇒ Doctor must treat "installed but Accessibility denied" as a first-class state,
distinct from "not installed". The permission guide is the only fix and needs the user.

### 7.3 The engine already has an observation budget — ours is a second layer

`snapshot` bounds itself: on exhaustion it still returns `ok: true` with
`data.complete: false`, `truncated: true`, `nodes_observed`, and
`subtree_truncated: true` on cut nodes.

⇒ `--max N` does not exist. Truncating the *tree* is the engine's job
(`--max-depth`, default 10; `-i --compact`; `--skeleton`). The wrapper's
4 000-char budget is a **rendering** budget on the text it prints. Two different
things — do not conflate them.
⇒ Read `complete`; never wait for `TIMEOUT` to learn a tree was partial.

### 7.4 Sessions, tracing, concurrency (verified live, isolated `AGENT_DESKTOP_HOME`)

- `session start --name X` → `{session_id, name, trace, created_at, artifacts, activation, next}`.
  Needs no TCC.
- Session ids look like `run-1793606074806-18143-0`.
- Per session on disk: `session.json`, `trace/`, `liveness/`, `refstore.lock`.
  The lock is **per session**, so concurrent sessions are isolated — there is no
  global socket and no global cursor lock.
- `status` under a session reports `session_id`, `tracing`, `state_root`.
- `AGENT_DESKTOP_HOME` relocates the whole root and must be absolute
  (`INVALID_ARGS` otherwise). Halo should point it at
  `~/Library/Caches/Halo/desktop/agent-desktop` so the user's own CLI sessions
  never mix with ours.
- Several agents may share one session id; `--agent-id` (1–64 chars) gives
  per-subagent cursors, but only under `session start --cursor --multi-agent`.

### 7.5 Tracing replaces most of the audit story

- Every command writes trace events by default (`trace: on`).
  `trace show [--limit] [--event PREFIX]` and `trace export [--out path.html]`
  render them — **both need no OS permission**.
- Trace lines redact `text`, `value`, `name`, `title`, `url` to `{"redacted": true}`.
- `--screenshots` adds pre/post PNGs; unredacted and sensitive, needs tracing on.

⇒ §3.3's "record every acted-on ref in the transcript" can be satisfied by
`trace export` instead of a bespoke event log. The `desktop` card then only needs
the summary; the HTML viewer becomes the "show me what it did" affordance.

### 7.6 Flag surface the plan did not know about

- `snapshot`: `--max-depth` (10), `--include-bounds`, `--surface`, `--root`,
  `--skeleton`, `-i/--interactive-only`, `--compact`, `--window-id`,
  `--timeout-ms` (3000), `-w/--wait-for`, `--wait-for-gone`,
  `--wait-timeout` (30000), `--force-electron-a11y`, `--trace`, `--trace-strict`.
- `status.supported_surfaces` advertises only
  `window, focused, menu, menubar, sheet, popover, alert`. `desktop`, `dock`,
  `spotlight` are accepted by the `--surface` enum but not advertised — gate on
  `supported_surfaces` and expect `PLATFORM_NOT_SUPPORTED` (supported list in
  `details`).
- `batch '<json>' --stop-on-error` runs several commands in one process. Not a
  transaction. The right tool for `act` + `wait --event` in one spawn.
- `wait` (ms / element / text / `--event window-opened`) exists — the
  "a fixed sleep is a race" rule should point at it.
- `cursor-overlay enable` draws an agent cursor and ripple; presentation only.
  Natural visual for the "Controlling <App>" strip.
- `find` also has `--first`, `--count`, `--limit`, `--native-id`, `--description`, `--surface`.

### 7.7 Pinned and still open

Pinned once Accessibility was granted: Finder snapshot sizes (§7.9), the
`find --first` and `get` wire shapes, and the fact that `get` does **not** reject
a ref from an older snapshot.

Still open, none of it blocking P2:

- `STALE_REF` was never observed. `get` on an older ref resolved fine, so the
  code path needs a UI change to reproduce — a real `act` against a mutating
  window, which is P2 work.
- secure-field (`AXSecureTextField`) addressability — does a ref even exist? The
  policy refuses the role, but that has not been exercised against a real field.
- `POLICY_DENIED` text for triple-click and the headed-gated commands.
- two-session contention against the same app (does the second `snapshot` see a
  coherent tree?).
- whether `TIMEOUT` on `list-apps` reproduces outside this sandbox — observed
  here as `ps: Operation not permitted`, i.e. a sandbox artifact, not a product bug.

### 7.8 Corrections applied to this plan

1. §3.2 — `snapshot --max N` does not exist; replaced with the real flag set.
2. §3.4 — "single instance socket / one cursor" is wrong. The lease is a UX guard
   over focus and headed input, not a correctness lock; it must carry the
   `session_id`, since refs only resolve under their session scope.
3. §3.3 — the audit trail can lean on `--trace` + `trace export`.
4. §3.6 / §3.7 — status detail must distinguish "installed, permission denied"
   from "not installed".
5. §3.2 — `-i --compact` must not be the default (see §7.9). `--skeleton` first.
6. §3.2 — the generation guard is *not* redundant with the engine: `get` still
   resolves refs from an older snapshot (see §7.9). It is the primary guard.

### 7.9 Live run (Accessibility granted, Finder)

Measured through `halo-desktop`, rendering budget 16 000:

| Flags | chars | refs | `complete` |
| --- | --- | --- | --- |
| `--skeleton` | 2 862 | 49 | **true** |
| `-i --compact` | 5 429 | 88 | false (192 nodes) |
| `--skeleton -i --compact` | 5 341 | 88 | false (192 nodes) |

Two things follow:

- **`-i --compact` is the wrong default here.** It walks deeper to filter, so it
  returns *more* text than a skeleton and still gets cut. The engine's own skill
  recommends `-i --compact` by default; on a real Finder window that is worse
  than `--skeleton` on both axes.
- **`--skeleton` and `-i` fight each other.** `--skeleton` clamps depth to 3 and
  adds `children_count`; adding `-i` re-expands the walk and the clamp is lost.
  Use one or the other, never both.

Other contract facts from the same run:

- **`get` resolves a ref from an older snapshot.** After a second snapshot,
  `get @<old>:e1 --property role` still answered `scrollarea` — no `STALE_REF`.
  The refmap is per snapshot id and outlives the next snapshot. So the engine's
  "refs are valid for one snapshot" is a *guideline*, not enforcement: our
  generation guard is the primary protection, and dropping it would be a real
  regression rather than a redundant check.
- `find --first` answers `{"match": {…}, "snapshot_id": …}`; `get` answers
  `{"property", "ref", "value"}`. The renderer unwraps both.
- Generation persistence is real: three snapshots produced 1, 2, 3 and
  `{"generation":3}` on disk. When the cache directory is not writable the
  counter restarts at 1 per command, and `halo-desktop` now says so in its output
  instead of silently promising a guard it does not have.
- Policy refusal happens before the spawn: `snapshot --app Passwords` never
  reached the engine.

### 7.10 Permission guidance (implemented)

`crates/desktop-tools/src/permissions.rs` is the single source of truth for every
word about TCC. `status`, `permissions`, `doctor` and the `PERM_DENIED` error all
render from it, so they cannot drift into three different explanations.

- **The app to add is not the CLI.** TCC attributes a grant to the *responsible*
  process, so a CLI run from Halo is covered by Halo's grant and one run from a
  terminal by the terminal's. `owner_hint()` resolves it from
  `HALO_DESKTOP_OWNER`, then `__CFBundleIdentifier`, then `TERM_PROGRAM`, then
  falls back to `Halo`. A bare bundle id is rendered as "the app with bundle id
  …" rather than printed at someone who is already stuck.
- **Blocking vs optional.** `guidance()` speaks only about Accessibility and
  returns `None` once it is granted — `status` never nags. `screen_recording_guidance()`
  is separate and only `permissions` prints it, because Screen Recording buys
  nothing until P3.
- **Two failure modes the naive text misses.** "Already switched on but still
  denied" (a rebuilt binary under a stale grant) and "switched on but nothing
  changed" (macOS applies a new Screen Recording grant only to a fresh process,
  so the app must be relaunched). Both are in the copy.
- **`permissions --open` is the only path that touches System Settings**, and it
  runs `agent-desktop permissions --guide` on an explicit flag — never
  automatically, and it never *requests* a grant itself.

### 7.11 Doctor

`halo-desktop doctor` reports the engine path and version, then the permission
summary, then points at `halo-desktop permissions` when anything is missing. The
in-app Doctor item still needs wiring (§3.7), but the CLI side is done and is
what the app row should call.
