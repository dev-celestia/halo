# Halo UI Docs — Page-by-Page Deep Dives

Source-grounded documentation for **every page module** in the reference implementation at
`references/hexbuffer/src/pages/` (25 folders). Each doc covers every file in its folder:
component names and props, layout hierarchies, handlers, IPC commands/events, store fields,
icon/library inventories (from actual imports), styling details, and notable findings
(dead code, bugs, copy-paste artifacts).

Stack facts that hold across all pages (document the ACTUAL imports, always):

- **Icons**: `@phosphor-icons/react` — not lucide-react.
- **UI primitives**: internal kit **`@celestia-project/ui`** v0.4.0 — shadcn-style, but composes
  via `render` props instead of `asChild`, and adds extensions like `leading="tight"`,
  `textSize="xs"`, `mono`, `variant="quiet"`, `size="icon-xs"`.
- **Editor**: Monaco only through the kit's `TextEditor` wrapper (`value`, `language`, `height`,
  `theme`, `options`; imperative `executeEdits` / `getModel().getFullModelRange()`).
- **State**: Zustand stores in `src/stores/` (most persist to `localStorage` under
  `halo-*` keys); some pages keep state local or page-local (`pages/intercept/state/`).
- **IPC**: Tauri v2 — `invoke()` commands and `listen()` events; a few pages talk raw
  WebSocket/HTTP instead (inspector's CDP, encoder's crypto-js).
- **Shared libs** referenced throughout: `@/lib/ipc.ts`, `@/lib/status-colors.ts`,
  `@/lib/http-message.ts`, `@/triggers/*` (action layer used by pages and AI tools alike),
  `@/layout/tabs-layout` (`TabbedPageLayout`), `@/components/ui/resizable`.

## Index

### Proxy & traffic

| Doc | Page | Highlights |
| :-- | :-- | :-- |
| [live-traffic.md](live-traffic.md) | HTTP history table, filters, detail inspector | 4 localStorage stores; dual storage modes; stream pause on context menu |
| [intercept.md](intercept.md) | Live request/response pause-and-modify | Page-local 625-line store; 1 s polling (no events); accessibility-engineered color palette; queue context menu |
| [api-override.md](api-override.md) | Proxy response override rules | Shares `mock_forge_*` backend with api-mock; hosts only via HTTP History right-click; full-URL rules |
| [listener.md](listener.md) | OOB collaborator (hosts, payloads, interactions) | Rust-source-derived IPC contract test; 10 s poll loop; orphaned-interactions group |
| [inspector.md](inspector.md) | CDP debugger attach (network/storage/console) | Raw CDP WebSocket from the webview; virtualized network ledger; 2 backend commands only |

### Attack tooling

| Doc | Page | Highlights |
| :-- | :-- | :-- |
| [intruder.md](intruder.md) | Invoker fuzzing (Sniper etc.) | Only Sniper reachable from UI; 50 ms event batching; no scope check on manual Start |
| [nuclei-run.md](nuclei-run.md) | Nuclei scan hub → staging → cockpit | 26-template catalogue; hand-rolled YAML parser; fake target test harness |
| [repeater.md](repeater.md) | Request forge + collections tree | dnd-kit tree with fixed row metrics; `new Function` script sandbox; URL⇄param sync |
| [regression.md](regression.md) | Nuclei-YAML regression tester | runId-guarded event stream; backend validation gates Save; conditions = matcher hits |
| [sql-injection.md](sql-injection.md) | Standalone SQLi tester | No store/persistence; default header bug (`Content-TextT`); rows capped at 100 |
| [port-scanner.md](port-scanner.md) | TCP connect scanner | `parsePorts` mini-language (tested); Stealth vs "Noisy" modes drive notification alerts |
| [hash.md](hash.md) | Hashcat engine front end | Optimistic attack state; blake3/scrypt disabled for attacks |
| [jwt.md](jwt.md) | JWT decode/verify/forge | Fully client-side Web Crypto; genSecret persisted to disk (security concern) |

### Browser, AI & automation

| Doc | Page | Highlights |
| :-- | :-- | :-- |
| [browser.md](browser.md) | AI browser crawl (Activity + Findings) | 9 `ai-browser:*` events; nuclei-style black console; insight→Memory bridge; orphaned human-input flow |
| [desktop.md](desktop.md) | AI assistant | ChatTransport; human-in-the-loop tool confirmation (10-min TTL); 26-tool executor; virtualized transcript |
| [workflow.md](workflow.md) | Node workflow editor | 36 node types; dual registry; never executes itself (Tauri events only) |
| [memory.md](memory.md) | Uteke memory engine (ONNX) | ~200 MB model download; imperative bridge into Notes store |

### Utilities

| Doc | Page | Highlights |
| :-- | :-- | :-- |
| [api-mock.md](api-mock.md) | Local mock API server (port 4000) | Dynamic `:param` routes with `{{template}}` preview; orphaned LogsPanel; dead sub-tab model |
| [comparer.md](comparer.md) | A/B text differ | npm `diff` (lines/words/chars) with inline highlighting; zero IPC; localStorage persistence |
| [encoder.md](encoder.md) | Encode/decode/hash toolbox | Pure crypto-js; clipboard `execCommand` fallback |
| [notes.md](notes.md) | Notes with drawing | SQLite bodies + localStorage tabs; PNGs to filesystem; dead BlockTextEditor |
| [kanban.md](kanban.md) | Task board | dnd-kit with per-mode drop semantics; **no persistence at all**; hard-coded assignee roster |
| [split-view.md](split-view.md) | Window layout manager | Portals toolbar into window header via `WindowHeaderSlot`; cross-window sync registry |
| [file-explorer.md](file-explorer.md) | Project file browser | R2 security (secrets never cross to webview); safePathSegments; onDragDropEvent |
| [settings.md](settings.md) | Settings hub | Dual-gate AI consent; CSS-driven search; Base UI render-prop quirk |

## Conventions in these docs

Each doc follows the same skeleton: **Overview & Purpose → Annotated File Tree →
Architecture & Dataflow (mermaid) → State Management → IPC Contracts → UI Layout &
Component Breakdown → UI Libraries & Primitives Used → Keyboard Shortcuts & Interactions →
Styling Details → Integration Points → Notable Findings → Source Files Documented.**
Every claim is grounded in the source files listed in the last section; "Notable Findings"
records dead code, orphaned components, and behavior worth knowing before re-implementing
the page in the Rust port.

Related: the backend/IPC contracts behind these pages are documented chapter-by-chapter in
[`docs/README.md`](../README.md) (01–06), and the security posture in
[`docs/security-audit-2026-10-21.md`](../security-audit-2026-10-21.md).
