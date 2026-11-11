# Intercept (`intercept`) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/intercept/` (17 files, ~2,330 lines)
> Exported page component: `InterceptPage`
> Companion chapter: `docs/04-intercept.md` (backend engine)

## 1. Overview & Purpose

The live traffic pause-and-modify workbench. Tab-scoped **capture hosts** decide which proxied requests get held; paused items stream into a queue (left panel, 35%), the selected item's raw HTTP message opens in a Monaco editor (right panel, 65%) where the analyst edits and then **Forward**, **Intercept response** (forward + hold the coming response), **Drop** (502 to client), or **Don't capture** (un-scope host + forward everything held for it). The page polls `get_paused_requests` every second; state lives in a page-local store (`state/intercept-store.ts`, the largest file at 625 lines) that owns tabs, selection, and raw-text drafts, and delegates all backend writes to `@/triggers/intercept`.

This is the most rigorously-commented page in the reference implementation: layout fixes, contrast rules, and dead-code removals are all documented in source comments, and two test files pin both the DOM structure and the color palette.

## 2. Annotated File Tree

```
intercept/
├── index.tsx                            (113)  Composition only: ProxyAlert + TabbedPageLayout + toolbar + 35/65 split
├── types.ts                              (46)  InterceptStatus (snake_case), PausedRequest, InterceptTab + createInterceptTab
├── api.ts                                (30)  Direct commands: get/set status & scope, paused requests, bypass patterns (4 unused)
├── lib.ts                               (117)  Raw-message builders, host/path parsing, formatRawMessage (JSON-aware, fixes Content-Length)
├── lib/
│   ├── request-styles.ts                (131)  Tone tables (method/status/direction) — accessibility-documented palette
│   └── request-styles.test.ts           (108)  Pins every tone + guards against bare text shades in light mode
├── state/
│   └── intercept-store.ts               (625)  The page brain: tabs, scope sync, selection sync, forward/drop flows
├── hooks/
│   └── use-intercept-page.ts             (35)  1-second refresh poll + tab actions pass-through
└── components/
    ├── proxy-alert.tsx                    (71)  Warning alert w/ Start Proxy (contrast rationale in comments)
    ├── intercept-toolbar.tsx             (235)  Capture-host chips + add input, Intercept On/Off, paused count (data-slot attrs)
    ├── queue-panel.tsx                    (94)  List + empty states; removal animation ids
    ├── queue-row.tsx                     (285)  Row: badge, host+direction arrow, path, time⇄hover-actions, ContextMenu
    ├── request-panel.tsx                  (86)  "Raw Request/Response" Label + Format button + TextEditor (readOnly w/o selection)
    └── hooks/
        ├── use-intercept-toolbar.ts       (37)  Host-input state (Enter = add)
        ├── use-queue-panel.ts            (104)  Row actions + removingIds animation set + sendRawToRepeater bridge
        └── use-request-panel.ts           (35)  rawRequest editing + Format toast logic
```

Tests: `components/queue-row.render.test.tsx` (jsdom; mounts through the real `ContextMenu` and asserts the timestamp shares the host's baseline — the documented layout bug), `lib/request-styles.test.ts` (palette mapping + `bareTextShadesIn` light-mode guard from `@/styles/light-mode-shades`).

## 3. Architecture & Dataflow

```mermaid
flowchart TD
    subgraph Poll["1s poll (use-intercept-page)"]
        R["refresh(): get_intercept_status + get_paused_requests"]
    end
    subgraph Store["useInterceptStore (persist 'halo-intercept-tabs')"]
        ST[(tabs+captureHosts, requests, selectedRequestId, rawRequest)]
    end
    subgraph TR["triggers/intercept"]
        FWD["forwardRequest → forward_intercepted_request"]
        FRS["forwardResponse → forward_intercepted_response"]
        DRP["dropRequest → drop_intercepted_request"]
        FTAB["forwardTab → forward_intercepted_tab"]
        TGL["toggleIntercept → set_intercept_enabled"]
    end
    Store -->|"syncActiveScope()"|"set_intercept_scope {tabId, capturePatterns}"| BE
    Store --> TR
    Poll --> Store
    UI["QueuePanel / RequestPanel / Toolbar"] --> Store
```

- **Selection sync** (`syncSelectedRequest`): every mutation recomputes the selected row — falls back to the tab's first request; if the loaded id changed, `rawRequest` is rebuilt from `buildRawPausedMessage`; if unchanged, the user's in-progress edits are preserved.
- **Forward semantics**: `forwardRequest(request)` uses the editor's `rawRequest` (re-parsed via `parseRawHttpRequest`/`parseRawHttpResponse` from `@/lib/http-message`, with `fallbackUrl` for requests) only when the row being forwarded **is the selected one**; otherwise it forwards the original untouched message (`forwardPausedItem`). Responses are decoded from `number[]` bodies with `TextDecoder`.
- **Tab close cascades**: closing a tab (or tabs to the left/right) first calls `forward_intercepted_tab` to flush its paused items, then prunes them from the store, fixes the active tab, and re-syncs scope + refresh. Never allows zero tabs (recreates `Tab 1`).
- **`addTabForHost`** (used by HTTP History "send to intercept"-style flows): reuses a tab whose capture pattern already matches (`*.domain` wildcards supported by `capturePatternMatchesHost`), else appends the host to the current tab, else creates a tab named after the host.
- **Bypass patterns** (`get/set/add/remove_intercept_bypass_pattern` in `api.ts`) are exported but unused by this page's UI.

## 4. State Management — `state/intercept-store.ts`

| Field | Type | Persisted | Notes |
| :-- | :-- | :-- | :-- |
| `tabs: InterceptTab[]` | `{ id, name, captureHosts }` | ✅ | Tab ids: `intercept-tab-${Date.now()}-${rand5}` |
| `activeTabId`, `nextTabNumber` | string/number | ✅ | `merge` validates activeTabId, defaults nextTabNumber to tabs.length+1 |
| `status: InterceptStatus \| null` | `{ mode: 'Enabled'\|'Disabled', paused_count }` | ❌ | snake_case straight from Rust |
| `requests: PausedRequest[]` | `{ id, timestamp, client_addr, server_addr, tab_id, request, response }` | ❌ | `request/response` bodies are `number[]` (byte arrays) |
| `selectedRequestId`, `loadedRequestId` | string \| null | ❌ | `loadedRequestId` guards in-progress edits |
| `rawRequest` | string | ❌ | The editor draft (request OR response, by direction) |
| `selectedDirection` | `'request' \| 'response' \| null` | ❌ | Drives the panel label |
| `isBusy`, `isRefreshing` | boolean | ❌ | Busy gates every mutation |

Persist key **`halo-intercept-tabs`**, `partialize` = tabs/activeTabId/nextTabNumber only. Actions: `setActiveTabId`, `addTab`, `addTabForHost`, `renameTab`, `closeTab`, `closeTabsToLeft/Right`, `addCaptureHost`, `removeCaptureHost`, `removeCaptureHostAndForward`, `setRawRequest`, `setSelectedRequestId`, `refresh`, `syncActiveScope`, `toggleIntercept`, `forwardSelectedRequest`, `forwardRequest`, `forwardRequestAndInterceptResponse`, `dropRequest`. Host normalization strips scheme/path/default ports (`:80`/`:443`) and lowercases.

## 5. IPC Contracts

### Commands

| Command | Args | Returns | Where |
| :-- | :-- | :-- | :-- |
| `get_intercept_status` | — | `InterceptStatus` | `refresh` (1 s poll) |
| `get_paused_requests` | — | `PausedRequest[]` | `refresh` |
| `set_intercept_scope` | `{ tabId, capturePatterns }` | `void` | `syncActiveScope` (every tab/host/selection change) |
| `set_intercept_enabled` | `{ enabled }` | `InterceptStatus` | `toggleIntercept` |
| `forward_intercepted_request` | `{ requestId, request: HttpRequestMessage \| null, interceptResponse }` | `void` | `forwardRequest`/`forwardRequestAndInterceptResponse` |
| `forward_intercepted_response` | `{ requestId, response: HttpResponseMessage }` | `void` | response forwards |
| `drop_intercepted_request` | `{ requestId }` | `void` | `dropRequest` (backend returns 502 "Dropped by intercept") |
| `forward_intercepted_tab` | `{ tabId }` | `usize` | tab close flows |
| *(unused here)* `get/set/add/remove_intercept_bypass_pattern` | — | `string[]` | `api.ts` exports only |
| *(related)* `open_intercept_browser`, `trust_intercept_ca` | — | — | triggers `openBrowser`/`trustCA` (used by toolbar-adjacent flows / settings) |

### Events

None — pure 1-second polling (`window.setInterval` in `useInterceptPage`).

### AI tool layer (`triggers/intercept/ai-tool.ts`)
`executeForwardPausedRequestAiTool({ waitSeconds })` polls `forwardPaused()` (up to 60 s, clamped; immediate by default) and returns instruction strings — covered by its own vitest suite with fake timers.

## 6. UI Layout & Component Breakdown

```
┌─ InterceptPage ─────────────────────────────────────────────────────────────────────┐
│ [⚠ Proxy is not running — Start Proxy]           (when disconnected)                │
│ ┌─ TabbedPageLayout (m-2, rounded-lg border; tabs addable/renameable/closable) ────┐ │
│ │ ┌─ InterceptToolbar (border-b bg-muted/20) ─────────────────────────────────────┐ │ │
│ │ │ Capture Hosts: [api.test.com ✕][*.example.com ✕] none(capturing nothing)      │ │ │
│ │ │                [Add host... (w-32)] [+]     [⏸ Intercept On] [3 paused requests]│ │ │
│ │ └───────────────────────────────────────────────────────────────────────────────┘ │ │
│ │ ┌─ Queue (35%, min 20%) ─────────────┬─ handle ─┬─ RequestPanel (65%, min 30%) ─┐ │ │
│ │ │ ┌ rounded-md border, divide-y ───┐ │          │ Raw Request        [</> Format]│ │ │
│ │ │ │[GET]↗ host.example.com  12:03:44│ │         │ ┌─ TextEditor (readOnly if no  ││ │ │
│ │ │ │        /api/users?id=1          │ │         │ │  selection) rounded-md border││ │ │
│ │ │ │   (hover → [⏸ Intercept][➤ Forward])      │ └──────────────────────────────┘│ │ │
│ │ │ └────────────────────────────────┘ │          │                                │ │ │
│ │ │ "Add a capture host to this tab…"  │          │                                │ │ │
│ │ └────────────────────────────────────┴──────────┴────────────────────────────────┘ │ │
│ └───────────────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

### `ProxyAlert` (`proxy-alert.tsx`)
Same warning `Alert` pattern as api-override/browser (`border-warning/40 bg-warning/10`), `PlugsIcon`, "Proxy is not running", description "Start the proxy to intercept HTTP requests", `AlertAction` outline `xs` button labelled `Starting…` while in flight. Comments document both the Alert-grid fix and the contrast decision: body copy uses `--warning-foreground` because the `--warning` tint on `bg-warning/10` measures **2.86:1** in light theme; `styles/globals.contrast.test.ts` guards the pairing.

### `InterceptToolbar` (`intercept-toolbar.tsx`)
Uses `data-slot` attributes (`capture-host`, `capture-hosts-empty`, `intercept-toggle`, `paused-count`) so tests assert on semantics, not Tailwind strings. Left: `Capture Hosts:` mono label + scrollable chip row (`max-w-[280px]`, hidden scrollbars) of `Badge variant="secondary" rounded-sm text-2xs` with `XIcon` remove buttons (`aria-label="Remove {pattern}"`, `animate-in fade-in zoom-in-95`); empty → italic "none (capturing nothing)". Add: mono `Input h-6 w-32` (`aria-label="Add a capture host"`, Enter submits via `useInterceptToolbar`) + `PlusIcon` outline button. Right: **Intercept On/Off** button (`PauseIcon`/`PlayIcon`, `variant="default"` when on / `"outline"` when off, titles explain pause-vs-passthrough) and, when `pausedCount > 0`, a mono counter chip "N paused request(s)".

### `InterceptQueuePanel` + `QueueRow`
List is a `rounded-md border` scroll area with `divide-y` rows. Empty states distinguish "Waiting for matching hosts in this tab…" (enabled + hosts) vs "Add a capture host to this tab to pause live requests."
**Row anatomy** (documented fix: `flex-row`, not the old `flex-col` that dropped the timestamp below the path):
- Badge `h-5 px-1.5 rounded border text-3xs font-mono font-semibold`: method (uppercase) for requests or status code for responses, colored by `getMethodTone`/`getStatusTone`.
- Host line (`text-xs font-medium truncate`) + **direction icon** (`ArrowRightIcon` sky "Outgoing request" / `ArrowLeftIcon` emerald "Returning response", `role="img"` + aria-label).
- Path line (mono `text-2xs` muted).
- Right block `min-w-[172px]`: timestamp (`text-2xs`, fades out on `group-hover`) with **absolutely-positioned hover actions fading in** (`group-hover` and `group-focus-within`): for requests an outline **Intercept** button (`PauseIcon`, "Intercept the response to this request too") and a default **Forward** (`PaperPlaneTiltIcon`).
- Row is `role="button" tabIndex={0}` with Enter/Space select; removal state applies `pointer-events-none animate-slide-out-right`.
- **`ContextMenu`** (`w-52`): Send to Repeater (`PaperPlaneTiltIcon`) / Intercept response (`FlagIcon`, requests only) / **Drop** (`TrashIcon`, `variant="destructive"`) / **Don't capture this host** (`ShieldSlashIcon`).

### `InterceptRequestPanel`
Header: `Label` "Raw Request"/"Raw Response" (from `selectedDirection`) + ghost **Format** button (`CodeIcon`, disabled without a selection or empty text) → `formatRawMessage`. Body: full-height kit `TextEditor` with `options={{ readOnly: !selectedRequestId }}` and `theme` from `useTheme` — no `language` prop (plain text editing of the full wire format).

### `lib.ts` utilities
`decodeRequestBody` (byte[]→string), `getPausedDirection`, `buildRawPausedRequest/Response/Message` (via `@/lib/http-message` builders, `addHostHeader: false`), `getRequestHost`/`getRequestPath` (URL-parse with raw fallback), `formatRequestTime` (HH:MM:SS), and **`formatRawMessage`**: tries whole-text JSON first; else splits at `\r\n\r\n` (or `\n\n`), pretty-prints a JSON body and **rewrites the `content-length` header** to the new byte length — mirroring the backend's `sync_content_length` concern on the client side.

### `lib/request-styles.ts` — the palette
Seven spelled-out `Tone` pairs (pill + text), each with light+dark halves (e.g. `border-emerald-500/25 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400`); neutral falls back to tokens (`border-border/60 bg-muted`). `METHOD_TONES`: GET emerald, POST sky, PUT amber, PATCH violet, DELETE rose, OPTIONS teal, HEAD indigo — deliberately matching `pages/repeater`'s palette. `getStatusTone`: <200 sky, <300 emerald, <400 amber, <500 orange, else rose (same thresholds as repeater's `status-styles.ts`, duplicated on purpose because that module is page-private). Direction: outbound sky (matches POST), inbound emerald (matches GET). The file's header documents why it does **not** reuse `lib/status-colors.ts#getMethodBadgeColor` (its solid `bg-green-500` + `text-white` measures 2.28:1) and why classes are spelled out (Tailwind v4 scanning + a past light-mode regression).

## 7. UI Libraries & Primitives Used

| Category | Items (actual imports) |
| :-- | :-- |
| UI kit | `Alert`, `AlertAction`, `AlertDescription`, `AlertTitle`, `Badge`, `Button`, `ContextMenu`, `ContextMenuContent`, `ContextMenuItem`, `ContextMenuSeparator`, `ContextMenuTrigger`, `Input`, `Label`, `TextEditor` |
| Local primitives | `ResizablePanel/Handle/Group` (`@/components/ui/resizable`, horizontal 35/65) · `TabbedPageLayout` (`@/layout/tabs-layout`, with add/rename/close/close-left/right callbacks) |
| Icons (`@phosphor-icons/react`) | `PauseIcon`, `PlayIcon`, `PlusIcon`, `XIcon`, `PlugsIcon`, `ArrowLeftIcon`, `ArrowRightIcon`, `FlagIcon`, `PaperPlaneTiltIcon`, `ShieldSlashIcon`, `TrashIcon`, `CodeIcon` |
| Editor | Kit `TextEditor`: `value`, `onChange`, `options={{ readOnly }}`, `theme` (no language) |
| State/IPC | `zustand` + `persist` (page-local store), `invoke` via `@/triggers/intercept` + page `api.ts` |
| Cross-page | `sendRawToRepeater` (`@/triggers/repeater`), `parseRawHttpRequest/Response` + `buildRawHttp*` (`@/lib/http-message`), `useProxyStart`, `cleanUrl` |
| Tests | `vitest` + jsdom (`queue-row.render.test.tsx`, `request-styles.test.ts`, `triggers/intercept/ai-tool.test.ts`) |

## 8. Keyboard Shortcuts & Interactions

- **Enter** in the capture-host input adds the host; **Enter/Space** on a queue row selects it (row is a focusable `role="button"`; `group-focus-within` also reveals the action buttons for keyboard users).
- Context menu (right-click) carries the full action set; hover reveals Intercept/Forward inline.
- Format button is disabled without a selection; editor is read-only without a selection.
- No global shortcuts; the toolbar toggle and every mutation toast via `sonner` on failure ("Failed to forward intercepted item." etc.).

## 9. Styling Details

- The 35/65 split uses local resizable primitives (not the kit) with `withHandle`.
- Accessibility-driven palette (see §6/request-styles): tinted pills with hairline borders instead of solid badges; every color has an explicit dark variant; `data-slot` attributes for test stability.
- Removal animation: `animate-slide-out-right` + `removingIds` set so rows slide out instead of the list reflowing on the next 1 s poll.
- `animate-in fade-in zoom-in-95 duration-150` on host chips.

## 10. Integration Points

- **Proxy engine** (`docs/04-intercept.md`): scope matching, pausing, Content-Length/gzip repair, 502-on-drop, and the managed browser (`open_intercept_browser` with NSS CA injection) all live backend-side; this page is the control surface.
- **HTTP History**: `addTabForHost` exists for cross-page "capture this host" flows; capture hosts per tab map 1:1 to backend `tab_scopes`.
- **Repeater**: queue-row context menu "Send to Repeater" builds the raw request (`buildRawPausedRequest`) and calls `sendRawToRepeater({ raw, url: cleanUrl, name: "METHOD path" })`.
- **Desktop AI assistant**: `triggers/intercept/ai-tool.ts` exposes forward-paused/drop as tools with an optional bounded wait (`waitSeconds`, clamp 60 s).
- **Settings/CA**: `trustCA()`/`openBrowser()` triggers support the managed-browser workflow.

## 11. Notable Findings

1. **Polling, not events**: paused requests are fetched every 1 s; there is no push channel for intercept (contrast with browser page's 9 event listeners). Backend intercept holds requests up to 120 s, so a 1 s poll is the freshness bound on the UI side.
2. **Contrast engineering is contractual**: `request-styles.ts` and `proxy-alert.tsx` cite measured ratios (2.4:1, 2.28:1, 2.86:1) and name the guard tests (`styles/globals.contrast.test.ts`, `styles/light-mode-shades.test.ts`); `request-styles.test.ts` enforces "no bare text shades" on the palette.
3. **Deliberate duplication over coupling**: status/method palettes duplicate repeater's thresholds with explanatory comments; `lib/status-colors.ts` is explicitly rejected as an import.
4. **Dead IPC surface**: four bypass-pattern commands are wrapped in `api.ts` but never called from UI; `forwardSelectedRequest` is only reachable through the trigger/AI-tool layer, not the page's own buttons (rows always call `forwardRequest(request)`).
5. **Store-as-orchestrator**: the store performs multi-step flows (close-tab flush → prune → rescope → refresh) with `isBusy` gating; hooks are thin and the page entry is composition only — the in-code comment records the refactor from a 351-line `index.tsx`.

## 12. Source Files Documented

`index.tsx` · `types.ts` · `api.ts` · `lib.ts` · `lib/request-styles.ts` · `lib/request-styles.test.ts` · `state/intercept-store.ts` · `hooks/use-intercept-page.ts` · `components/proxy-alert.tsx` · `components/intercept-toolbar.tsx` · `components/queue-panel.tsx` · `components/queue-row.tsx` · `components/queue-row.render.test.tsx` (header) · `components/request-panel.tsx` · `components/hooks/{use-intercept-toolbar,use-queue-panel,use-request-panel}.ts` · `src/triggers/intercept/{ui,forwarding,lifecycle,ai-tool}.ts`
