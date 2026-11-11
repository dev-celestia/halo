# Intruder (intruder) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/intruder/` (21 files, ~5,700 lines incl. shared deps read)
> Related shared modules documented for usage: `src/stores/intruder.ts`, `src/stores/app.ts`, `src/triggers/intruder/*`, `src/layout/tabs-layout/*`, `src/components/ui/resizable.tsx`, `src/components/theme-provider.tsx`, `src/lib/{http-message,ipc,utils}.ts`, `src/pages/file-explorer/{data/bundled-wordlists.ts,constants.ts,hooks/use-local-storage.ts,lib/format.ts}`, `src-tauri/src/commands/invoker.rs`, `src-tauri/src/ai/mod.rs`

## 1. Overview & Purpose

Intruder (internally also called **Invoker** — the codebase is mid-rename, almost every export has a dual `Intruder*`/`Invoker*` alias) is Halo's Burp-Suite-style automated payload-attack workbench at route `/intruder` (alias route `/invoker` renders the same component, `src/App.tsx:132-133`). It fuzzes `§`-marked positions in a raw HTTP request with per-position payload sets and renders a results table with a request/response inspector. Structure, top to bottom:

- **Safety banner** — a dismissible (persisted) "Authorized targets only" `Alert` above the workspace.
- **Attack tabs** — every tab is an independent attack (`IntruderTab` in the store) rendered through the shared `TabbedPageLayout`/`PageTabBar` with rename/close context menus.
- **Run toolbar** — status dot ("Running"/"Ready"), a start-blocked reason chip, Start/Stop button, and a `current / total (n%)` progress readout with a 2px progress line pinned to the toolbar's bottom border.
- **Config panel (left)** — `IntruderConfigDialog` with two internal tabs: **Request** (raw-request `TextEditor` + § marker tooling + AI auto-mark) and **Config** (`AttackTab` delay field + `PayloadsTab` per-position payload editors).
- **Results + inspector (right)** — `IntruderResultsPanel` (filter toolbar + plain HTML `<table>`) and `IntruderResultInspector` (read-only side-by-side or stacked request/response `TextEditor`s).

State lives in one non-persisted Zustand store, `useIntruderStore` (`src/stores/intruder.ts`), plus one persisted flag on `useAppStore` (`intruderSafetyAlertDismissed`). Attacks run in Rust: `invoke('start_invoker_attack')` returns a UUID `attackId`; results stream back over per-attack Tauri events `invoker-result-{id}` / `invoker-progress-{id}` and are batched into the store on a 50 ms flush timer.

## 2. Annotated File Tree — every file with a one-line description of what it does

```
src/pages/intruder/
├── index.tsx                                  IntruderPage: safety Alert, TabbedPageLayout, run toolbar (status/Start/Stop/progress), split vs full-width workspace via ResizablePanelGroup.
├── types.ts                                   Domain model + pure helpers: HttpRequest/AttackMode/PayloadType/PayloadConfig/PayloadPosition/AttackConfig/AttackProgress/AttackResult, default factories, §-marker scan/replace, raw request build/parse adapters.
├── constants.ts                               PAYLOAD_TYPES ('SimpleList'|'RuntimeFile'|'NumberRange') and INTRUDER_STATUS_FILTERS (2xx/3xx/4xx/5xx/errors with descriptions).
├── hooks/
│   ├── use-page.ts                            useIntruderPage: store wiring, startBlockedReason computation, pendingRequest ingestion effect, handleStartAttack validation.
│   └── use-filters.ts                         useIntruderFilters: reads per-tab filter fields, memoized filterResults pipeline.
├── lib/
│   └── utils.ts                               formatPayloadValues ("k=v, k=v"), getResultUrl (response.final_url), filterResults (search + status buckets + grep/error toggles).
└── components/
    ├── intruder-config/
    │   ├── index.tsx                          IntruderConfigDialog: pulsing progress Badge, amber start-blocked warning, Request|Config Tabs.
    │   └── config/
    │       ├── index.ts                       Barrel re-exporting AttackTab, RequestTab, PayloadsTab, NumberInputField.
    │       ├── attack-tab.tsx                 AttackTab: single "Delay (ms)" NumberInputField bound to config.delay_ms.
    │       ├── request-tab.tsx                RequestTab: raw request TextEditor, "Add Mark §"/"Clear §" buttons, AI marker-suggestion confirm Dialog with HighlightedRequestPreview.
    │       ├── payloads-tab.tsx               PayloadsTab: per-position Tabs, SimpleList TextEditor, NumberRangePayloadEditor (start/end/step/padding/format + preview), Browse Presets + Load from File.
    │       └── number-input-field.tsx         NumberInputField: Label + <Input type="number"> generic field.
    ├── payload-preset-dialog.tsx              IntruderPayloadPresetDialog: 3-column Dialog (categories | preset list + search | preview) with Use Payload action.
    ├── result-inspector.tsx                   IntruderResultInspector: header (Result #id, status chip, Match tag, payload summary, time, maximize/stack/close) + Modified Request / Captured Response TextEditors.
    ├── results-panel.tsx                      IntruderResultsPanel: toolbar (count, debounced search, Status dropdown filter, Match/Errors toggles, Reset, full-width toggle, Clear) + results <table>.
    └── hooks/
        ├── use-attack-tab.ts                  useAttackTab: config + delay_ms + handleDelayChange (parseInt || 0).
        ├── use-payload-preset-dialog.ts       usePayloadPresetDialog: bundled + downloaded wordlist catalog, search, lazy file preview (PAYLOAD_PREVIEW_LIMIT=500).
        ├── use-payloads-tab.ts                usePayloadsTab, useNumberRangePayloadEditor, usePayloadFileButton + number-range formatting/validation/count helpers.
        ├── use-request-tab.ts                 useRequestTab: raw draft sync, editor ref, mark/clear §, suggest_invoker_markers AI flow, suggestion selection set.
        ├── use-result-inspector.ts            useResultInspector: buildModifiedRequest/buildRawAttackResponse memos, payloadSummary, isStacked local state.
        └── use-results-panel.ts               useResultsPanel + getStatusStyle: 200 ms debounced search, statusCounts, grepMatchCount, row selection.
```

## 3. Architecture & Dataflow

```mermaid
flowchart TD
    subgraph UI["IntruderPage tree"]
        PAGE[useIntruderPage]
        CFG[IntruderConfigDialog / RequestTab / PayloadsTab]
        RP[IntruderResultsPanel]
        RI[IntruderResultInspector]
        PD[PayloadPresetDialog]
    end

    subgraph Store["useIntruderStore (zustand, not persisted)"]
        TABS[tabs[] per-tab config/results/filters/flags]
        PEND[pendingRequest]
        ACTIONS[startAttack / stopAttack / updateConfig / ...]
    end

    subgraph Triggers["src/triggers/intruder"]
        ORCH[startAttack(config) / stopAttack(attackId)]
        SEND[sendToIntruder]
        AIT[executeStartInvokerAttackAiTool]
    end

    subgraph BE["Rust backend"]
        CMD1["invoke('start_invoker_attack') -> attackId (uuid)"]
        CMD2["invoke('stop_invoker_attack')"]
        CMD3["invoke('suggest_invoker_markers')"]
        RUN[run_invoker_attack tokio task]
        EVT1["emit invoker-result-{id}"]
        EVT2["emit invoker-progress-{id}"]
    end

    PAGE -->|updateConfig / setBaseRequest| TABS
    PAGE -->|handleStartAttack| ACTIONS
    CFG -->|updateConfig / updatePositionPayload| TABS
    RP -->|setSelectedResult / filter actions| TABS
    RI -->|toggleInspectorMaximized| TABS
    PD -->|updatePositionPayload| TABS
    ACTIONS -->|orchStartAttack| ORCH --> CMD1 --> RUN
    ACTIONS -->|orchStopAttack| CMD2
    RUN --> EVT1 & EVT2
    EVT1 -->|50ms batch flush| TABS
    EVT2 -->|progress / Complete| TABS
    SEND -->|addAttackTab + triggerNavBlink/openWindow| Store
    AIT -->|startIntruderAttack via ui.ts| ACTIONS
    RequestTab -->|Auto mark| CMD3
```

Key flows:

- **Start attack** (`hooks/use-page.ts` `handleStartAttack`): re-scans § positions from the raw request (`findRequestPayloadPositions`), syncs `position_payloads` (`syncPositionPayloads`), toasts + aborts on missing URL/positions/payloads, then `startAttack()`. The store validates the URL, calls `orchStartAttack(tab.config)` (which re-forces `mode: 'Sniper'`), receives the `attackId`, resets `results/progress/selectedResult`, then subscribes to the two per-attack events.
- **Results batching** (`stores/intruder.ts:496-510`): each `invoker-result-{id}` event payload is pushed to `resultBuffersByTab` and a 50 ms `setTimeout` flushes the batch into `tab.results`; `invoker-progress-{id}` with `type: 'Complete'` forces a final flush, clears `isRunning/progress/attackId`.
- **Send-to ingestion**: two independent paths populate tabs — (a) HTTP history's "Send to Intruder" (`live-traffic/http-history/.../use-log-entry-view.ts:52-70`) builds a config and calls `useIntruderStore.getState().addAttackTab(config)` directly with a toast; (b) the AI assistant's `send_to_intruder` tool calls `sendToIntruder()` (`triggers/intruder/send-to.ts`), which resolves `logId` → `getHttpLogDetail(logId)` or parses `rawRequest`, creates the tab, then navigates with `useNavStore.triggerNavBlink/openWindow/focusWindow('/intruder')`. A third, older channel — `store.setPendingRequest(...)` — is consumed by the `useIntruderPage` effect (`use-page.ts:78-96`) which converts it into `setBaseRequest` + positions and clears it.

## 4. State Management

### `useIntruderStore` (`src/stores/intruder.ts`, zustand `create`, **no persist middleware**)

Module-level (non-reactive) maps: `unlistenProgressByTab`, `unlistenResultByTab`, `resultBuffersByTab`, `flushTimersByTab`.

`IntruderTab` fields the page touches:

| Field | Type | Written by | Read by |
|---|---|---|---|
| `id` | `` `intruder-tab-${Date.now()}-${rand}` `` | `createAttackTab` | tabs, layout |
| `name` | `string` (tab index or attack name) | `renameTab`, `addAttackTab` | `PageTabBar` |
| `config` | `AttackConfig` | `updateConfig`, `setBaseRequest`, `updatePositionPayload`, `updatePayloadType/Values`, `updateNumberRange`, `add/removeProcessingStep`, `updateGrepMatch/Extract`, `updateSessionHandling` | all config tabs, filters, inspector |
| `results` | `AttackResult[]` | `startAttack` (reset), batch flush, `clearResults` | results panel, filters |
| `isRunning` | `boolean` | `startAttack`/`stopAttack`/Complete event | toolbar, buttons |
| `attackId` | `string \| null` | `startAttack`/`stopAttack`/Complete | `stopAttack` |
| `progress` | `{current,total} \| null` | `invoker-progress-{id}` handler | toolbar, dialog badge |
| `selectedResult` | `AttackResult \| null` | `setSelectedResult`, `clearResults` | inspector, table highlight |
| `startError` | `string \| null` | start failure, `clearStartError` | `startBlockedReason` |
| `filterSearch` / `filterStatusCodes` / `filterOnlyGrepMatch` / `filterOnlyErrors` | filter state | `setFilterSearch`, `toggleFilterStatusCode`, `clearFilterStatusCodes`, `setFilterOnlyGrepMatch`, `setFilterOnlyErrors`, `clearAllFilters` | `useIntruderFilters` |
| `isFullWidthResults` / `isInspectorMaximized` | layout flags | toggles + setters | page layout, inspector |
| `payloadDialogOpen`, `payloadDialogPositionName`, `rawRequestDialogOpen`, `rawRequestContent` | legacy dialog state | setters exist (`setPayloadDialogOpen`, `setRawRequestDialogOpen`, `setRawRequestContent`) but **no current page component uses them** | — |

Top-level state: `tabs`, `activeTabId`, `nextAttackTabNumber` (starts 2), `pendingRequest`, `bypassPatterns` (intercept passthrough patterns fetched via `getInterceptBypassPatterns` from `@/pages/intercept/api`; store-only, unused by page UI).

Notable store behaviors:

- `updateConfig` **always re-forces `mode: 'Sniper'`** and re-runs `syncPositionPayloads` over `positions`/`position_payloads`/`payload_config` (`stores/intruder.ts:266-279`). The other `AttackMode` values (`BatteringRam`, `Pitchfork`, `ClusterBomb`) exist in types and the AI-tool map but no UI selects them.
- `closeTab`: unregisters listeners/buffers via `cleanupTabListeners`; closing the last tab replaces it with a fresh `createAttackTab(1)` and resets `nextAttackTabNumber` to 2; closing the active tab selects the previous index (`Math.max(0, closedTabIndex - 1)`).
- `startAttack` calls `cleanupTabListeners(tab.id)` first (no duplicate listeners across restarts) and surfaces failures via `startError` + `toast.error`.

### `useAppStore` (`src/stores/app.ts`, `persist` middleware)

| Field | Used for |
|---|---|
| `intruderSafetyAlertDismissed` (fallback `invokerSafetyAlertDismissed`) | hides the safety `Alert`; both keys are kept in sync by the store's setters/merge |

`useIntruderPage` reads them defensively: `s.intruderSafetyAlertDismissed ?? s.invokerSafetyAlertDismissed`.

### Page-local state

| Hook | Local state |
|---|---|
| `useRequestTab` | `rawRequestDraft`, `autoMarkLoading`, `suggestionsDialogOpen`, `suggestions`, `selectedSuggestionIds: Set<string>`, `rawRequestEditorRef`, `editRef` (suppresses store→editor echo) |
| `usePayloadsTab` | `presetDialogOpen`, `activePositionName` |
| `useResultInspector` | `isStacked` (default `false` = side-by-side) |
| `useResultsPanel` | `localSearch` + 200 ms `debounceRef` |
| `usePayloadPresetDialog` | `selectedCategory`, `selectedPayloadId`, `search`, `downloadedEntries`, `filePreview`, `previewLoading` |

## 5. IPC Contracts

### Commands invoked

| Command | Args (frontend) | Returns | Called from |
|---|---|---|---|
| `start_invoker_attack` | `{ config: AttackConfig }` (trigger re-spreads `{ ...config, mode: 'Sniper' }`) | `Promise<string>` — uuid v4 `attackId` | `src/triggers/intruder/attack.ts` via `store.startAttack` |
| `stop_invoker_attack` | `{ attackId: string }` (note camelCase arg key) | `Promise<void>` | same file via `store.stopAttack` |
| `suggest_invoker_markers` | `{ request: { rawRequest: string } }` | `Promise<InvokerMarkerSuggestionResponse>` — `{ provider, model, suggestions: InvokerMarkerSuggestion[], candidateCount }` where a suggestion is `{ id, start, end, value, category, location, confidence, reason }` | `useRequestTab.handleAutoMark` |
| `get_http_log_detail` (indirect) | `logId` | `ProxyRecord` with `request.{method,uri,headers,body: number[]}` | `sendToIntruder` via `getHttpLogDetail` |
| `get/add/remove_intercept_bypass_patterns` (indirect) | pattern ops | `string[]` | store's `fetch/add/removeBypassPatterns` |

Rust side (`src-tauri/src/commands/invoker.rs`): `start_invoker_attack` validates the config (`validate_invoker_config`: URL required, ≥1 § marker, position ranges valid, payloads resolvable), spawns `run_invoker_attack` in a tokio task with an `Arc<AtomicBool>` cancel flag registered in `InvokerState.cancellations`; `stop_invoker_attack` flips the flag. `suggest_invoker_markers` (`src-tauri/src/ai/mod.rs:119`) delegates to `ai::auto_mark::suggest_invoker_markers_impl`.

### Events listened

| Event | Payload | Handler |
|---|---|---|
| `invoker-progress-{attackId}` | `AttackProgress` = `{ type: 'Update', current, total } \| { type: 'Complete' }` | registered in `store.startAttack`; `Update` sets `tab.progress`, `Complete` flushes buffered results and clears `isRunning/progress/attackId` |
| `invoker-result-{attackId}` | `AttackResult` = `{ id, payload_values: Record<string,string>, status?, response_length?, response_time_ms?, error?, comment?, response?: { status, status_text, headers, body, time_ms, final_url }, grep_match, grep_extracted? }` | pushes into the per-tab buffer; 50 ms timer flushes into `tab.results` |

Also consumed by the payload preset dialog (Tauri fs/path APIs, not events): `documentDir()`, `join()`, `exists()`, `readDir()`, `readTextFile()`, `stat()` from `@tauri-apps/api/path` and `@tauri-apps/plugin-fs`.

## 6. UI Layout & Component Breakdown

```
┌─────────────────────────────────────────────────────────────────────┐
│ [⚠ Authorized targets only ……………………………… (Dismiss)]  ← Alert, p-2   │  ~56px, hidden once dismissed
├─────────────────────────────────────────────────────────────────────┤
│ Tab: Attack 1  ×                                    ← PageTabBar    │  ~36px, border-b border-primary
├─────────────────────────────────────────────────────────────────────┤
│ ● RUNNING │ [progress chip?]              [■ Stop] / [▶ Start]      │  toolbar px-3 py-2, border-b bg-muted/20
│ ‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾ progress: 12 / 50 (24%) ─────────────────────    │  2px absolute bottom bar bg-primary
├───────────────────────┬─────────────────────────────────────────────┤
│ Config (45%)          │ Results (55%)  ← ResizablePanelGroup horiz  │
│ ┌───────────────────┐ │ ┌─────────────────────────────────────────┐ │
│ │[Request][Config]  │ │ │ Results (12/50)  [🔍 search]  [Status▾] │ │ toolbar px-3 py-1.5 bg-muted/40
│ │ Raw Request       │ │ │   [Match(n)] [Errors(n)] [Reset][⤢][🗑] │ │
│ │  {n} marked       │ │ ├─────────────────────────────────────────┤ │
│ │ [Add §][Clear §]  │ │ │ # │ Payload │ URL │Status│Len│Time│Match│ │ sticky thead, text-[12px] mono tbody
│ │ ┌───────────────┐ │ │ │ 1 │ id=1    │ …   │ 200  │1.2│34ms│ ✓  │ │
│ │ │ TextEditor    │ │ │ │ … │         │     │      │   │    │    │ │
│ │ │ h-[460px]     │ │ │ └─────────────────────────────────────────┘ │
│ │ └───────────────┘ │ │ ═ ResizableHandle (50/25 min) ═             │
│ ├─ Config tab ──────┤ │ ┌─────────────────────────────────────────┐ │
│ │ Delay (ms) [100]  │ │ │ Result #abc  200 │Match│ id=1 │ 34ms    │ │ inspector header bg-muted/30
│ │ ── PayloadsTab ── │ │ │ [⤡][⇕][✕]                                │ │
│ │ pos_1: 3 payloads │ │ ├──────────────────┬──────────────────────┤ │
│ │ [pos_1 tabs]      │ │ │ Modified Request │ Captured Response    │ │ two TextEditors, grid-cols-2
│ │ [Simple list]     │ │ │ (readOnly)       │ (readOnly, pretty    │ │ or grid-rows-2 when stacked
│ │ [Number range]    │ │ │                  │  JSON body)          │ │
│ │ [Browse Presets]  │ │ │                  │                      │ │
│ │ [Load from File]  │ │ │                  │                      │ │
│ └───────────────────┘ │ └──────────────────┴──────────────────────┘ │
└───────────────────────┴─────────────────────────────────────────────┘
  Full-width mode (isFullWidthResults): left column is replaced — results take the whole
  width; with a selected result they split vertically 50/50 (min 25) above the inspector;
  maximized inspector (isInspectorMaximized) hides the results panel entirely.
```

### `IntruderPage` (`index.tsx`)

- Props: none; `export const InvokerPage = IntruderPage` alias. Returns `null` until `page.activeTab` resolves.
- **Safety banner**: `Alert` variant tokens `border-warning/40 bg-warning/10 text-warning-foreground` with `ShieldWarningIcon`, `AlertTitle` "Authorized targets only", `AlertDescription` (text-warning-foreground/85), `AlertAction` → ghost `size="xs"` Dismiss button setting the persisted flag. A long comment documents why the old `flex` classes were removed (the kit `Alert` is a grid; tailwind-merge was dropping `display: grid`).
- **Toolbar**: status dot `h-2 w-2 rounded-full` (`bg-emerald-500 animate-pulse` running / `bg-muted-foreground/45` idle) + uppercase `text-2xs` label; blocked-reason chip `text-amber-600 dark:text-amber-400 bg-amber-500/10 border-amber-500/20`; Start (`PlayIcon weight="fill"`, disabled by `startBlockedReason`) / Stop (`SquareIcon`, `variant="destructive"`) with `leading="tight" size="md"`; progress text `font-mono` and the 2px `bg-primary` progress line `transition-all duration-300 ease-out`.
- **Workspace**: standard mode = horizontal `ResizablePanelGroup` (45/30min | 55/30min) with `ResizableHandle withHandle`; right column repeats the results/inspector vertical split used in full-width mode. Inspector is only rendered when `page.activeTab.config` exists.

### `IntruderConfigDialog` (`components/intruder-config/index.tsx`)

Props `{ isRunning, progress, startBlockedReason }`. Shows a pulsing secondary `Badge` `{current} / {total}` while running, an amber warning div when blocked, and a two-trigger `Tabs` (`defaultValue="request"`): **Request** → `RequestTab`; **Config** → `space-y-6` stack of `AttackTab` + `PayloadsTab`.

### `RequestTab` + `useRequestTab`

- Sticky header row (`sticky top-0 bg-background z-10`): `Label` "Raw Request", `{n} marked` Badge (`variant` default/secondary), `ButtonGroup` with **Add Mark §** and **Clear §** outline `size="sm"` buttons (the "Auto §" button is commented out in source).
- Editor: 460px bordered box around `TextEditor` (`value=rawRequestDraft`, `onChange=handleEditorChange`, `onMount=setEditorRef`, `theme`, `disableValidation`, `className="text-xs [&_.cm-content]:text-xs [&_.cm-gutters]:text-3xs"`).
- Marker ops against the editor instance (Monaco-style `TextEditorInstance` API: `getSelection`, `getModel`, `executeEdits`, `getPosition/setPosition`, `getFullModelRange`, `getValue`, `focus`):
  - `markRawRequestTarget`: empty selection inserts `§§` and steps the caret back one column; non-empty selection wraps with `§…§`, or unwraps if the selection already starts+ends with `§`; then syncs the store via `updateRawRequest` (which also re-derives `positions`).
  - `clearAllMarkers`: strips every `§` with `replace(/§/g, '')` through a full-range `executeEdits`.
  - `editRef` guard prevents the store round-trip from clobbering the draft while typing.
- **AI auto-mark** (`handleAutoMark`, currently only reachable via code paths that call it — the button is commented out): validates the draft, `invoke('suggest_invoker_markers', …)`, filters suggestions with `validateSuggestions` (bounds check, `text.slice(start,end) === value`, no overlap with existing `§` ranges or previously used ranges), opens the confirm dialog with all suggestion ids preselected.
- **Suggestion dialog**: `DialogContent` `sm:max-w-[960px]`, left `HighlightedRequestPreview` (suggestions rendered as `<mark className="rounded-sm bg-amber-300/70 px-0.5 text-foreground dark:bg-amber-500/40">` inside a `pre`), right checklist (`Checkbox` per suggestion showing `category` Badge, `location`, `value` in `<code>`, `reason`, `Confidence nn%`), header `n / m selected` with All/None ghost buttons, footer Cancel + **Apply selected** (`handleApplyAutoMarkers` re-validates, applies right-to-left via `applyMarkers`, toasts `Applied N marker(s)`).

### `AttackTab` + `useAttackTab` + `NumberInputField`

Config tab currently exposes exactly one control: `NumberInputField label="Delay (ms)"` writing `delay_ms` (`parseInt(value,10) || 0`) into `updateConfig`. `NumberInputField` is a generic `Label` + `<Input type="number">` (`props: label, value, onChange, placeholder?`).

### `PayloadsTab` + `usePayloadsTab`

- Empty state if no positions: "Mark payload positions in the request with § markers before assigning payloads." in a muted bordered box.
- Summary row: one secondary Badge per position `{name}: {count} payloads`.
- Position switcher: controlled `Tabs value={selectedPositionName}` with wrapping `TabsList` (`flex h-auto flex-wrap justify-start`); each trigger shows `{name}{': ' + default_value}`.
- Per position: type toggle buttons **Simple list** / **Number range** (variants default/outline; note `RuntimeFile` has no button — it is only set by the preset dialog for downloaded wordlists or `handleUsePreset`); count Badge; then either:
  - **SimpleList**: 144px (`h-36`) `TextEditor language="markdown"` whose `onChange` splits on `\n` and filters blank lines into `values`; below it **Browse Presets** (opens `IntruderPayloadPresetDialog` for that position), **Load from File** (`PayloadFileButton`: hidden `<input type="file" accept=".txt,.lst,.wordlist">`, `FileReader.readAsText`, fills `values` and sets `file_path` to the file name), and a truncating Badge showing `file_path` when set.
  - **NumberRange** (`NumberRangePayloadEditor`): `grid sm:grid-cols-4` of Start/End/Step/Padding number inputs (`aria-invalid` when invalid), a Format text input (default `{}`), and a preview line rendering the first `NUMBER_RANGE_PREVIEW_LIMIT = 8` values as secondary Badges plus `and N more`, or validation errors (`getNumberRangeValidation`: required start/end/step, step ≠ 0, padding ≥ 0, direction-vs-step mismatch). `formatNumberPayload` supports `{:0N}` zero-padding (negative values keep the sign outside the pad width).

### `IntruderPayloadPresetDialog` + `usePayloadPresetDialog`

Props `{ open, onOpenChange, onUsePayload }`. `DialogContent className="grid max-h-[min(760px,calc(100vh-2rem))] gap-3 p-0 sm:max-w-[960px]"`; body grid `grid-cols-[180px_260px_minmax(0,1fr)]`:
- Left: category list (plain buttons, `FolderIcon`, selected = `bg-background shadow-sm`). Categories come from `BUNDLED_WORDLIST_CATEGORIES` plus a synthetic `Downloaded` category when downloaded wordlists exist.
- Middle: search `Input textSize="xs"` with `MagnifyingGlassIcon` (`h-8 pl-7`) and a `ScrollArea className="h-[480px]"` of preset buttons (`FileTextIcon`, name, `metaLabel`; selected = `border-primary bg-muted`).
- Right: selected payload name/description + secondary `Badge` meta, and a `ScrollArea h-[410px]` `<pre>` preview of up to `PAYLOAD_PREVIEW_LIMIT = 500` lines with `… N more payloads` tail; `Loading preview…` while reading.
- Footer: Cancel + **Use Payload** (disabled without selection).
- Data: bundled presets are inlined at build time via `import.meta.glob('./bundled-wordlists/**/*', { eager: true, query: '?raw' })` (`BUNDLED_WORDLISTS`, metaLabel = `N payloads`); downloaded presets are `.txt` files recursively scanned under `documentDir()/Halo Files/Wordlists` (`LOCAL_STORAGE_DIR_NAME='Halo Files'`, `WORDLISTS_DIR_NAME='Wordlists'`) while the dialog is open, read lazily with `readTextFile` on selection, metaLabel = `formatBytes(size)`.
- `handleUsePreset`: bundled → `payload_type: 'SimpleList'` with `values` and `file_path: 'Preset: {name}'`; file → `payload_type: 'RuntimeFile'` with `file_path`.

### `IntruderResultsPanel` + `useResultsPanel` + `useIntruderFilters`

- Toolbar: `Results ({filtered}/{total})` label (the `/{total}` suffix only appears when filtered); search box `w-44 focus:w-56 transition-all` with absolute `MagnifyingGlassIcon` and clear `XIcon`; `ButtonGroup` containing the Status dropdown (`DropdownMenuTrigger render={<Button …/>}` — the kit's `render` prop pattern instead of Radix `asChild`) listing `INTRUDER_STATUS_FILTERS` as `DropdownMenuCheckboxItem` with per-bucket live counts, plus "Clear status filter" item; a **Match (n)** toggle (only when `grep_match` configured or matches exist) tinted emerald when on; an **Errors (n)** toggle (only when `statusCounts.errors > 0`) tinted destructive when on; a **Reset** quiet button when `hasActiveFilters`; full-width toggle (`ArrowsOutSimpleIcon`/`ArrowsInSimpleIcon`); **Clear** (disabled at 0 results).
- Table: plain `<table className="w-full text-[12px] border-collapse">` (**not virtualized**), sticky thead (`bg-muted/50`), columns `#` (w-12), Payload (w-1/4), URL (w-1/3), Status (w-20), Length (w-20, right), Time (w-20, right), Match (w-16, conditional). tbody is `font-mono divide-y divide-border/40`. Rows: `cursor-pointer hover:bg-muted/30`; selected `bg-primary/10 … dark:bg-primary/15 font-medium`; error rows `bg-destructive/5 text-destructive`. Status cell renders a bordered chip with `getStatusStyle(result)`; error chip `bg-destructive/15 border-destructive/20`; Match cell a `variant="success"` Badge "Match" or a dim dash. Empty-state row spans 6 or 7 columns with "Running attack…" / "No results match the current filter criteria." / "No results available.".
- `filterResults` matches `search` against status string, `formatPayloadValues`, URL, error, `grep_extracted`; `onlyErrors`/`onlyGrepMatch` toggles; status buckets `2xx/3xx/4xx/5xx/errors` or exact numeric status equality.
- Search is debounced 200 ms into the store (`setFilterSearch`); `statusCounts` and `grepMatchCount` are computed over **all** results, not the filtered set.

### `IntruderResultInspector` + `useResultInspector`

- Props `{ selectedResult: AttackResult, config: AttackConfig, onClose }`.
- Header: `Result #{id}`, status chip (same `getStatusStyle` classes) or Error chip, optional emerald Match tag with `CheckCircleIcon`, `|` separators, truncated `max-w-[280px] font-mono` payload summary (values joined `', '`), `{time}ms`; right side: maximize (`CornersOutIcon`/`CornersInIcon`, `title="Maximize/Restore inspector height"`, `size="icon"` `size-7`), stack toggle (`ArrowsInIcon`/`ArrowsOutIcon`, local `isStacked` state), close (`XIcon`, `hover:text-destructive`).
- Content: CSS grid `grid-cols-2 divide-x` (side-by-side) or `grid-rows-2 divide-y` (stacked), each pane a labeled band (`Modified Request` / `Captured Response` + byte count) over a read-only `TextEditor language="markdown"` with `disableValidation`.
- `buildModifiedRequest`: `replaceRequestMarkedValues(config.base_request, result.payload_values)` substitutes each `§…§` in URL → headers (Host excluded, headers sorted case-insensitively for deterministic substitution order) → body, then `buildRawHttpRequest` renders the raw text (payload markers re-decoded from `%C2%A7`). `buildRawAttackResponse`: error text, "No response captured.", or `buildRawHttpResponse(response, { prettyJsonBody: true })`.

## 7. UI Libraries & Primitives Used

### `@celestia-project/ui` components (exact imports)

| Component | Used in |
|---|---|
| `Alert`, `AlertAction`, `AlertDescription`, `AlertTitle` | `index.tsx` safety banner |
| `Badge` | results table Match cell, config progress, `{n} marked`, preset meta, position counts, number-range preview |
| `Button` (props used: `variant` `default/outline/ghost/quiet/destructive`, `size` `xs/sm/md/icon`, `leading="tight"`, `render` passthrough) | everywhere |
| `ButtonGroup` | results toolbar, request-tab marker group |
| `Checkbox` | AI suggestion checklist |
| `Dialog`, `DialogContent`, `DialogFooter`, `DialogHeader`, `DialogTitle` | payload presets, AI suggestions |
| `DropdownMenu`, `DropdownMenuCheckboxItem`, `DropdownMenuContent`, `DropdownMenuGroup`, `DropdownMenuItem`, `DropdownMenuLabel`, `DropdownMenuSeparator`, `DropdownMenuTrigger` | status filter |
| `Input` (props used: `textSize="xs"`, `mono`) | search, number fields, param inputs |
| `Label` | field labels |
| `ScrollArea` | preset dialog lists/preview |
| `Tabs`, `TabsContent`, `TabsList`, `TabsTrigger` | config dialog tabs, position tabs, layout wrapper |
| `TextEditor`, `TextEditorInstance` (type) | raw request + simple-list + inspector editors; props `value/onChange/onMount/options({readOnly})/language/theme/disableValidation/className` |
| `Tooltip`, `TooltipContent`, `TooltipTrigger` | imported in `request-tab.tsx` but **unused in JSX** |

### Radix primitives

None imported directly by page code. The kit components above wrap Radix internally (Dialog/DropdownMenu/Tabs/Checkbox/Tooltip/ScrollArea); the `render={<Button/>}` pattern on `DropdownMenuTrigger` is the kit's replacement for Radix's `asChild`.

### Phosphor icons (`@phosphor-icons/react`, exact names)

| Icon | Used in |
|---|---|
| `PlayIcon`, `ShieldWarningIcon`, `SquareIcon` | `index.tsx` (weight="fill" on Play/Square) |
| `TrashIcon`, `MagnifyingGlassIcon`, `XIcon`, `CaretDownIcon`, `ArrowsOutSimpleIcon`, `ArrowsInSimpleIcon`, `FunnelSimpleIcon`, `CheckCircleIcon`, `WarningCircleIcon` | `results-panel.tsx` |
| `XIcon`, `ArrowsInIcon`, `ArrowsOutIcon`, `CornersOutIcon`, `CornersInIcon`, `CheckCircleIcon`, `WarningCircleIcon` (import only, unused in JSX) | `result-inspector.tsx` |
| `FileTextIcon`, `FolderIcon`, `MagnifyingGlassIcon` | `payload-preset-dialog.tsx` |
| `AsteriskIcon`, `InfoIcon`, `SpinnerGapIcon`, `TargetIcon` (Asterisk/Spinner live only inside commented-out code; Info/Target unused) | `request-tab.tsx` |
| `DotsSixIcon`, `DotsSixVerticalIcon` | `@/components/ui/resizable` handle |
| `WarningCircleIcon`, `CheckCircleIcon`, `SpinnerGapIcon`, `PlusIcon`, `XIcon` | shared `PageTabBar` status icons / add / close |

### Editors, virtualization, other third-party

| Library | Usage |
|---|---|
| `TextEditor` (kit; CodeMirror-flavored — page CSS overrides `[&_.cm-content]`, `[&_.cm-gutters]`) with a **Monaco-style instance API** (`getSelection/getModel/executeEdits({range,text,forceMoveMarkers})/getPosition/setPosition/getFullModelRange/getValue/focus`) | request editing, payload lists, response inspection |
| `react-resizable-panels` v4 (`Group/Panel/Separator`) via `@/components/ui/resizable` (`orientation` prop with string sizes `"45"`, `minSize`, `maxSize`) | workspace + inspector splits |
| `zustand` + `zustand/react/shallow` (`useShallow`) | store |
| `sonner` (`toast.error/info/success`) | validation + marker/attack feedback |
| `@tauri-apps/api/core` (`invoke`), `api/event` (`listen`, `UnlistenFn`), `api/path` (`documentDir`, `join`), `@tauri-apps/plugin-fs` (`exists`, `readDir`, `readTextFile`, `stat`) | IPC + wordlist scanning |
| `clsx` + `tailwind-merge` (`cn`) | all class composition |
| `import.meta.glob` eager raw (Vite) | bundled wordlist inlining |

## 8. Keyboard Shortcuts & Interactions

The page defines no global hotkeys. Observable keyboard interactions:

| Where | Key | Behavior |
|---|---|---|
| Tab rename input (`PageTabBar`) | `Enter` | commits rename (`finishEditingTab`) |
| Tab rename input | `Escape` | cancels (`cancelEditingTab`) |
| Tabs (rename) | double-click | starts inline rename |
| Raw-request `TextEditor` | typing | `handleEditorChange` → parse → `updateConfig` (invalid raw text keeps the draft, skips config update) |
| Payload simple-list editor | newline | line becomes a payload value (blank lines dropped) |
| Number-range inputs | typing | live re-validation + preview |

Pointer interactions of note: row click selects result → inspector opens; `ResizableHandle withHandle` drag on all splits; dropdown/checkbox filters; all destructive-ish buttons carry `title` tooltips (Clear, Reset, Match, Errors, layout toggles, inspector buttons).

## 9. Styling Details

- **Status colors** (`getStatusStyle`, `use-results-panel.ts`): 2xx `bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20`; 3xx `bg-sky-500/10 text-sky-600 dark:text-sky-400 border-sky-500/20`; ≥400 `bg-destructive/10 text-destructive border-destructive/20`; fallback `bg-muted text-muted-foreground border-muted-foreground/10`. Error rows/chips use `destructive/5…15`.
- **Semantic tokens**: `warning` family for the safety banner and blocked chips (`border-warning/40 bg-warning/10 text-warning-foreground`, amber chips `text-amber-600 dark:text-amber-400 bg-amber-500/10`); `primary` for progress line, selection (`bg-primary/10`), active toggles; `muted/N` for every tool band (`bg-muted/20`, `/30`, `/40`, `/50`).
- **Type scale**: custom steps `text-2xs`, `text-3xs`, `text-4xs` (tab labels, chips, badges); `text-[12px]` table; `font-mono` for payloads/URLs/status/progress; `uppercase tracking-wider` micro-headers.
- **Motion**: `animate-pulse` on the running dot and config progress Badge; progress line `transition-all duration-300 ease-out`; search box `focus:w-56 transition-all duration-150`.
- **Editor typography**: `[&_.cm-content]:text-xs [&_.cm-gutters]:text-3xs` descendant overrides on every `TextEditor`.
- Code comments in `index.tsx` document a real layout bugfix: the kit `Alert` is a grid (icon col 1 spanning rows) and a previous `flex items-center justify-between` wrapper made tailwind-merge drop the grid, breaking icon alignment.

## 10. Integration Points

- **Routes**: `/intruder` and `/invoker` both render `IntruderPage` (`src/App.tsx:132-133`, `layout/desktop-workspace/page-lazy-imports.ts:35-36`); standalone mode via `routes/page-resolver.tsx` (`StandaloneLayout id="/intruder" title="Intruder"`).
- **HTTP history → Intruder**: "Send to Intruder" context-menu/detail-view actions (`live-traffic/http-history/components/log-table/constants.ts:228`, `log-detail-view.tsx:277,473`) call `handleSendToIntruder` (`use-log-entry-view.ts:52-70`), which builds an `AttackConfig` and calls `useIntruderStore.getState().addAttackTab(config)` + `toast.success("Sent to Intruder")`.
- **AI assistant tool**: `send_to_intruder` → `sendToIntruder` (`triggers/intruder/send-to.ts`; resolves `logId` via `getHttpLogDetail`, names the tab `{METHOD} {uri}`, optionally seeds `updatePayloadValues`, then `useNavStore.triggerNavBlink/openWindow/focusWindow('/intruder')`); `start_invoker_attack`/`stop_invoker_attack` tools (`triggers/intruder/ai-tool.ts`) wrap `startIntruderAttack/stopIntruderAttack`, register a job in the assistant's job registry, and enforce `assertHostInScope(baseUrl, …)` before launching — a scope gate that the manual UI Start button does **not** have.
- **`pendingRequest` channel**: `store.setPendingRequest(request)` is consumed by `useIntruderPage`'s effect (sets base request, derives positions, syncs payloads, clears).
- **Wordlists Hub**: the preset dialog reads the same `Halo Files/Wordlists` tree managed by the file-explorer page (`WORDLISTS_DIR_NAME`, `LOCAL_STORAGE_DIR_NAME`, `formatWordlistName`, `formatBytes` reused from `src/pages/file-explorer/`).
- **Intercept bypass patterns**: `useIntruderStore` embeds `bypassPatterns` + fetch/add/remove via `@/pages/intercept/api` (passthrough during attacks); not surfaced in this page's UI.
- **Naming migration**: every module exports `Invoker*` aliases (`useInvokerStore`, `InvokerPage`, `InvokerResultsPanel`, …) — both spellings must be kept in sync when refactoring.

## 11. Source Files Documented

Page folder (all read in full):

- `src/pages/intruder/index.tsx`
- `src/pages/intruder/types.ts`
- `src/pages/intruder/constants.ts`
- `src/pages/intruder/hooks/use-page.ts`
- `src/pages/intruder/hooks/use-filters.ts`
- `src/pages/intruder/lib/utils.ts`
- `src/pages/intruder/components/results-panel.tsx`
- `src/pages/intruder/components/result-inspector.tsx`
- `src/pages/intruder/components/payload-preset-dialog.tsx`
- `src/pages/intruder/components/intruder-config/index.tsx`
- `src/pages/intruder/components/intruder-config/config/index.ts`
- `src/pages/intruder/components/intruder-config/config/attack-tab.tsx`
- `src/pages/intruder/components/intruder-config/config/request-tab.tsx`
- `src/pages/intruder/components/intruder-config/config/payloads-tab.tsx`
- `src/pages/intruder/components/intruder-config/config/number-input-field.tsx`
- `src/pages/intruder/components/hooks/use-attack-tab.ts`
- `src/pages/intruder/components/hooks/use-payload-preset-dialog.ts`
- `src/pages/intruder/components/hooks/use-payloads-tab.ts`
- `src/pages/intruder/components/hooks/use-request-tab.ts`
- `src/pages/intruder/components/hooks/use-result-inspector.ts`
- `src/pages/intruder/components/hooks/use-results-panel.ts`

Shared modules read for usage documentation:

- `src/stores/intruder.ts`, `src/stores/app.ts` (safety flag fields), `src/stores/nav.ts` (blink/window API)
- `src/triggers/index.ts`, `src/triggers/intruder/{index,attack,ui,send-to,ai-tool}.ts`
- `src/lib/http-message.ts`, `src/lib/ipc.ts`, `src/lib/utils.ts`
- `src/layout/tabs-layout/{tabbed-page-layout,tab-bar}.tsx`
- `src/components/ui/resizable.tsx`, `src/components/theme-provider.tsx`
- `src/pages/file-explorer/data/bundled-wordlists.ts`, `constants.ts`, `hooks/use-local-storage.ts` (constants), `lib/format.ts`
- `src/pages/live-traffic/http-history/components/log-table/components/hooks/use-log-entry-view.ts` (send-to path), `src/pages/live-traffic/http-history/api.ts` (`getHttpLogDetail`)
- `src/layout/desktop-workspace/page-lazy-imports.ts`, `src/routes/page-resolver.tsx`, `src/App.tsx` (routing)
- `src-tauri/src/commands/invoker.rs` (command signatures, validation, event emission), `src-tauri/src/ai/mod.rs` (`suggest_invoker_markers`), `src-tauri/src/sqli/mod.rs` (cross-check)
