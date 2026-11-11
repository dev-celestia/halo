# Codebase Audit — 2026-11-06

Scope: the project's own Rust code (23 crates under `crates/`, ~169k lines).
Excluded: `references/` (vendored reference projects), `vendor/` (patched
dependencies), `crates/cutlass/**` (vendored from Cutlass, MIT/Apache-2.0 —
kept bit-identical to upstream).

## What was done

Every project-owned file **≥ 1000 lines** was split into thematic modules,
behavior-preserving (verbatim code moves + minimal visibility adjustments +
short `//!` module docs). No logic, naming, or public-API changes. One
exception: the `bridge::Backend` trait was reorganized into per-area
supertraits (`BackendProxy`, `BackendHistory`, …) so `native/` and `stub/`
can implement one area per file — Rust forbids two `impl Backend for T`
blocks in one crate, and the aggregate `Backend` façade keeps every existing
`&dyn Backend` / `T: Backend` caller working.

| Before | After |
|---|---|
| `bridge/src/work.rs` (3003) | `work/` — kind, tab, coder_settings, git_types, agent_types, coder_work, coder_sessions, coder_panes, coder_layout, terminal_layout |
| `bridge/src/types.rs` (1277) | `types/` — history, collections, forge, intruder, crawler, debug_log, events, settings, notes, mock_forge, port_scanner |
| `bridge/src/native.rs` (2293) | `native/` — one `impl BackendXxx for NativeBackend` per area + `dto.rs` |
| `bridge/src/stub.rs` (1579) | `stub/` — same area cut as native |
| `bridge/src/backend.rs` (478) | area supertraits + aggregate `Backend` façade |
| `bridge/src/bridge/coder_lsp.rs` (1998) | `coder_lsp/` — state, diagnostics, providers, bridge, tests |
| `bridge/src/bridge/coder_fs.rs` (1957) | `coder_fs/` — project, files, tabs, panes, explorer_ops |
| `bridge/src/bridge/git.rs` (1578) | `git/` — source_control, graph, helpers, lanes, tests |
| `bridge/src/bridge/state.rs` (1030) | `state/` — tabs, projection, overviews, events, notes |
| `bridge/src/bridge/util.rs` (1131) | `util/` — remap, shell_layout, workspace, undo, paths, languages, format, settings, misc, filter |
| `bridge/src/bridge/postman_import.rs` (1193) | `postman_import/` — types, parse, body, auth, url, scripts, variables, tests |
| `bridge/src/bridge/doctor_probes.rs` (1336) | `doctor_probes/` — dependencies, desktop, runtimes, ca, app, orphans, storage, tests |
| `ui/src/app.rs` (2456) | `app/` — mod (struct + EditorDraft), construction, sync, drafts, actions, nav, work_tabs, render |
| `ui/src/pages/agent/transcript.rs` (1876) | `transcript/` — status, work, sources, reasoning, messages, rows, cards, tests |
| `ui/src/pages/settings/coder/agent.rs` (1453) | `agent/` — mod (render), shared, pi, opencode, antigravity, deepseek, browser, mcp, credential |
| `ui/src/pages/r2_storage/mod.rs` (1450) | folder gains toolbar, bucket_sidebar, objects, details |
| `ui/src/pages/agent/mod.rs` (1097) | setup cards extracted to `setup_cards.rs` |
| `coder/src/git.rs` (1459) | `git/` — mod (DTOs), process, status, mutations, queries, tests |
| `coder/src/editor.rs` (1020) | `editor/` — mod (consts), paths, layout, prefs, file_ops, tests |
| `storage/src/db/repository/proxy_logs.rs` (1309) | `proxy_logs/` — mod, writes, queries, summaries, mappers |

Deferred (in-flight work, do not split yet):

- `coder/src/agent/cursor.rs` (1413, growing) — actively edited; natural seam
  is auth/install vs protocol/run (`agent/cursor/` folder).
- `bridge/src/bridge/halo_agent/*` (session.rs 1386, reducer.rs 1086) —
  steering-contract work in flight.

## Conventions established

- Folder modules follow the `halo_agent/` template: `mod.rs` = `mod` decls +
  re-exports only; sibling files per theme; tests in a sibling `tests.rs`
  (`#[cfg(test)] mod tests;`) or inline where they exercise one file.
- Cross-file private helpers: `pub(super)` when only siblings in the same
  folder need them, `pub(in crate::bridge)`-style when wider, `pub(crate)` for
  re-exported module items. Public API paths in `lib.rs` are unchanged —
  folder `mod.rs` re-exports keep `pub use work::{…}` etc. working.
- Multiple inherent `impl` blocks across files are fine (used for
  `Bridge`, `HaloApp`, `BridgeState`, `Database`); trait impls must stay one
  per type — hence the Backend area-trait split.
- After moving code: `cargo check` the crate + dependents, `cargo test -p`,
  `rustfmt` only the touched files.

## Tier-2 follow-ups (700–999 lines, not yet split)

Each has a clean seam if it ever needs one:

- `bridge/doctor_actions.rs` (970) — action dispatch per DoctorCategory.
- `bridge/repeater.rs` (910) — send + folder tree + diff surfaces.
- `bridge/jwt.rs` (897) — decode/verify vs UI helpers (tests are ~250 lines).
- `bridge/doctor.rs` (879) — reactive view; pairs with doctor_actions.
- `ui/pages/source_control/diff/syntax.rs` (930) — tokenizer vs byte
  scanners; behavior locked by `diff/tests.rs`.
- `ui/pages/agent/history.rs` (891) — session rail rows.
- `ui/pages/repeater/folder_tree.rs` (872) — tree nodes vs drag logic.
- `ui/theme.rs` (857) — trivial 3-way split: mode animation, preset
  constructors, highlight-JSON data.
- `ui/pages/agent/{context_rail,transcript_view,model_picker}.rs`
  (823/818/802) — one `render_*` entry each; split per section if needed.
- `harness/opencode/run.rs` (1010) / `antigravity/run.rs` (893) — steering
  work in flight; `supervise` + control plumbing would move to sibling files.
- `spider/engine/common/mod.rs` (990) — `process_item` and
  `enqueue_navigation` are the two monster methods to extract.
- `spider/output/mod.rs` (828), `spider/types/options.rs` (814) — formatters /
  enums+parsers.
- `traffic/lifecycle.rs` (866) — HTTP handler vs WebSocket handler halves;
  `websocket.rs` already exists as the home for the WS half.
- `mitm-proxy/builder.rs` (847) — 61% is a test module; move tests first.
- `storage/db/payload_store.rs` (802) — ephemeral/disk/store split.
- `ui/pages/editors.rs` (824) — per-tool editor wiring.

## Health notes

- `cargo check --workspace` green at audit time; `halo-bridge` (198 tests),
  `halo-coder` (43), `halo-storage` (10) green after the splits.
- Pre-existing dead-code warnings in `coder/src/agent/cursor.rs`
  (login/SDK install path currently unused) — cleanup candidate.
- `cargo test -p halo` (GPUI integration tests) hits a rustc SIGBUS during
  the lib-test build on this machine. **Confirmed pre-existing**: a clean
  worktree at the pre-refactor baseline (b0ac1b8) crashes identically.
  Environmental — shared `/tmp` target dir and/or concurrent builds; library
  code compiles clean before and after the refactor.
- No crate opts into the workspace `[workspace.lints]` block (it governs
  `crates/cutlass/**` only). Opting Halo crates in is a cheap follow-up but
  will surface many `unwrap`/`expect` hits in test code first.
