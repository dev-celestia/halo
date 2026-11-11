# SQL Injection (sql-injection) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/sql-injection/` (9 files, ~1,400 lines)
> Related shared modules documented for usage: `src/components/ui/resizable.tsx`, `src/lib/{ipc,utils}.ts`, `src/routes/page-resolver.tsx`, `src/App.tsx`, `src-tauri/src/sqli/mod.rs`

## 1. Overview & Purpose

SQL Injection Helper (route `/sql-injection`, standalone title "SQLi Helper") is a form-driven SQLi scanner front end: the user enters a target URL + method, declares HTTP parameters with per-parameter injection flags, picks a risk level and technique set, and launches a Rust scan. Unlike Intruder/Repeater, **this page has no Zustand store and no persistence** — all state is `useState` inside the single `useSqliPage` hook, so configuration and results are lost when the window navigates away.

Structure, top to bottom:

- **`ScanToolbar`** — two stacked rows: (1) method `Select` fused to the URL `Input` plus error chip, live phase chip, JSON/CSV export, Clear, and Start/Stop Scan; (2) "Scan Parameters" settings row with Risk `Select` and technique toggle buttons.
- **Progress strip** — a 2px `bg-primary` progress bar with glow shadow over a `phase … %` mono caption, only rendered when `progress.total > 0`.
- **Left panel (25%, min 18, max 45)** — `ParametersPanel`: parameter cards with inject `Checkbox`, location Badge, value `Input`, delete button; sticky "Add Custom Parameter" form.
- **Right panel (75%)** — `Tabs` with two results tabs: **Vulnerabilities** (`VulnerabilitiesTab`: findings table + details pane with PoC copy) and **Data Extraction** (`ExtractionTab`: databases → tables → rows three-column browser).

The scan itself runs in Rust (`SqliDetector`): `invoke('start_sqli_scan', { config })` awaits the full scan and returns the final `SqliScanResult`, while live updates stream over the `sqli-progress-{scanId}` event.

## 2. Annotated File Tree — every file with a one-line description of what it does

```
src/pages/sql-injection/
├── index.tsx                          SqlInjectionPage: ScanToolbar + progress strip + horizontal ResizablePanelGroup (ParametersPanel | results Tabs).
├── types.ts                           SqliParam, SqliVulnerability, SqliExtracted{Column,Table,Database}, SqliScanResult, SqliProgressEvent union, SqliRiskLevel/SqliTechnique/SqliSeverity/SqliParamLocation unions.
├── constants.ts                       TECHNIQUE_LABELS (technique → display name) and SEVERITY_COLORS (per-severity Tailwind class strings).
├── hooks/
│   └── use-sqli-page.ts               useSqliPage: ALL page state (config + scan + selection), add/remove/toggle parameter, startScan/stopScan IPC, clearResults, export handlers.
├── lib/
│   └── export-helpers.ts              downloadFile (Blob + anchor click), exportAsJson (pretty JSON), exportAsCsv (quoted rows).
└── components/
    ├── scan-toolbar.tsx               ScanToolbar: method+URL fused control, error/phase chips, export/clear/Start/Stop, risk select, technique toggles.
    ├── parameters-panel.tsx           ParametersPanel: "Target Parameters" header with {n} Active badge, ScrollArea card list, sticky add-parameter form.
    ├── vulnerabilities-tab.tsx        VulnerabilitiesTab: findings Table (Parameter/Location/Technique/DBMS/Severity/PoC) + details panel with clipboard copy button.
    └── extraction-tab.tsx             ExtractionTab: 3-panel ResizablePanelGroup — Databases list, Tables list (with row-count badges), Data table (rows sliced to 100).
```

## 3. Architecture & Dataflow

```mermaid
flowchart TD
    subgraph Page["SqlInjectionPage"]
        TB[ScanToolbar]
        PP[ParametersPanel]
        VT[VulnerabilitiesTab]
        ET[ExtractionTab]
    end

    HOOK[useSqliPage - all useState, no store]

    TB -->|url/method/risk/techniques/ start-stop-export| HOOK
    PP -->|add/remove/toggle/value| HOOK
    HOOK -->|vulnerabilities + selection| VT
    HOOK -->|databases + selection| ET
    HOOK -->|invoke start_sqli_scan config| BE[(Rust start_sqli_scan / SqliDetector)]
    BE -->|emit sqli-progress-{scanId}| EVT[SqliProgressEvent listener]
    EVT -->|Update / VulnerabilityFound / Complete / Error / Cancelled| HOOK
    BE -->|SqliScanResult return value| HOOK
    HOOK -->|Blob download| FS[(browser download: JSON/CSV)]
```

Flow detail for `startScan` (`hooks/use-sqli-page.ts:99-176`):

1. Guards: returns early if `!url.trim() || parameters.length === 0` (the Start button is also disabled via `hasUrlAndParams`).
2. Generates `scanId = crypto.randomUUID()`, clears results/selections/errors, initializes progress `total = count(parameters where inject)`, sets `isRunning`.
3. Subscribes `listen<SqliProgressEvent>('sqli-progress-' + scanId, …)` **before** invoking.
4. `await invoke<SqliScanResult>('start_sqli_scan', { config })` — when it resolves, the returned `vulnerabilities`/`databases` overwrite the streamed arrays, first vulnerability/DB get auto-selected, unlisteners run, `isRunning` clears.
5. `stopScan` invokes `stop_sqli_scan` with the remembered `scanId` (held in `scanIdRef` **state**, not a ref, despite the name) and clears `isRunning`; the backend just flags the scan cancelled and the `Cancelled` event confirms.

## 4. State Management

Single hook `useSqliPage` — plain React `useState`, **no Zustand store, no `persist`, nothing survives unmount**.

| State | Type / initial | Consumers |
|---|---|---|
| `url` | `''` | toolbar input, scan config |
| `method` | `'GET' \| 'POST'`, default `'GET'` | method select, new-param location default |
| `headers` | `Array<[string,string]>`, default `[['Content-TextT','application/x-www-form-urlencoded']]` | sent verbatim in scan config; `setHeaders` is exported but **no UI edits headers** (note the `'Content-TextT'` typo in the default) |
| `parameters` | `SqliParam[]` (`{name, value, location: 'url'|'body'|'header', inject}`) | ParametersPanel, config |
| `newParamName` / `newParamValue` | `''` | add-parameter form |
| `riskLevel` | `'low' \| 'medium' \| 'high'`, default `'medium'` | risk select, config |
| `techniques` | `Set<SqliTechnique>`, default `{'boolean_blind','time_based'}` | technique toggles, config |
| `isRunning` | `false` | chips, buttons, empty states |
| `progress` | `{current,total,phase,message}` | progress strip, phase chip |
| `vulnerabilities` / `databases` | `SqliVulnerability[]` / `SqliExtractedDatabase[]` | tabs, tab-count badges |
| `selectedVuln` / `selectedDb` / `selectedTable` | `string \| null` | selection sync |
| `error` | `''` | toolbar error chip |
| `scanIdRef` | `string \| null` | stopScan target |

Derived: `injectCount` (parameters with `inject`), `selectedVulnData`, `selectedDbData`, `tableData` (found by name). Parameter actions: `addParameter` (defaults `location` to `'url'` for GET else `'body'`, `inject: true`, clears the form), `removeParameter(name)`, `toggleParamInject(name)`, `toggleTechnique(tech)` (Set copy). `clearResults` resets results, selections, progress, error — not the config. `handleExportJson`/`handleExportCsv` snapshot current data.

## 5. IPC Contracts

### Commands invoked

| Command | Args (frontend) | Returns | Where |
|---|---|---|---|
| `start_sqli_scan` | `{ config: SqliScanConfig }` with `config = { scan_id, url (trimmed), method (uppercased), headers: Array<[string,string]>, params: Array<{name, value, location, inject}>, risk_level, techniques: string[] (from Set), concurrency: 5 (hardcoded), delay_ms: 0 (hardcoded) }` | `Promise<SqliScanResult>` = `{ scan_id, url, vulnerabilities: SqliVulnerability[], databases: SqliExtractedDatabase[], start_time, end_time? }` | `useSqliPage.startScan` |
| `stop_sqli_scan` | `{ scanId: scanIdRef }` (camelCase arg key) | `Promise<void>` | `useSqliPage.stopScan` |

Rust side (`src-tauri/src/sqli/mod.rs`): `start_sqli_scan` registers the scan in `SqliScanState`, builds an `SqliDetector`, and passes a progress callback that emits `sqli-progress-{scan_id}` per event; the command **awaits the whole scan** and returns the final result; `stop_sqli_scan` calls `state.cancel_scan(&scan_id)`. Errors surface through the generic `toErrorMessage` helper (`src/lib/ipc.ts`).

### Events listened

| Event | Payload (`SqliProgressEvent` union) | Handler behavior |
|---|---|---|
| `sqli-progress-{scanId}` | `{ type: 'Update', current, total, phase, message }` | replaces `progress` state |
| | `{ type: 'VulnerabilityFound', vulnerability: SqliVulnerability }` | appends to `vulnerabilities` (live results before completion) |
| | `{ type: 'DataExtracted', database, table, row_count }` | **explicitly ignored** — "handled by final result" (comment in source) |
| | `{ type: 'Complete', result: SqliScanResult }` | sets `isRunning = false` (result arrives via the command return) |
| | `{ type: 'Error', message }` | sets `error`, `isRunning = false` |
| | `{ type: 'Cancelled' }` | sets `isRunning = false` |

## 6. UI Layout & Component Breakdown

```
┌──────────────────────────────────────────────────────────────────────┐
│ [GET ▾][http://target.com/search?q=  ]  (err) (◍ phase) │ JSON CSV  │ ScanToolbar row 1, bg-background
│                          ─ divider ─                     🗃 | ▶ Start│ h-8 controls
│ ⚙ SCAN PARAMETERS   Risk: [Medium ▾]   Techniques:              │ row 2, border-t bg-muted/20
│                       [Boolean Blind][Time-Based][UNION-Based][…] │
├──────────────────────────────────────────────────────────────────────┤
│ ▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░ 2px progress bar (glow)   phase…        42%   │ only when progress.total > 0
├──────────────┬───────────────────────────────────────────────────────┤
│ Parameters   │ [Vulnerabilities ②] [Data Extraction ③]  h-9 tab band │
│ (25%)        ├───────────────────────────────────────────────────────┤
│ TARGET PARA- │ Vulns tab:  ┌─ table 68% ─────────┬─ details 32% ───┐ │
│ METERS ②Act. │             │ Param Loc Tech DBMS │ Vulnerability   │ │
│ ┌──────────┐ │             │ Sev  PoC            │ Details + copy  │ │
│ │☑ q   url │ │             └─────────────────────┴─────────────────┘ │
│ │ [value ] │ │ Extraction tab: ┌dbs 20%┬tables 20%┬data 60%───────┐ │
│ │      [🗑]│ │                 │ db1   │ users(5) │ rows ≤100     │ │
│ └──────────┘ │                 │ db2   │ logs (2) │ NULL italics  │ │
│ + Add Param  │                 └───────┴──────────┴───────────────┘ │
└──────────────┴───────────────────────────────────────────────────────┘
```

### `SqlInjectionPage` (`index.tsx`)

Props: none. Single root `flex flex-col h-full bg-background`. Renders `ScanToolbar` with 18 props (config values, counts, `hasUrlAndParams = !!url.trim() && parameters.length > 0`, and all callbacks). The progress strip is a `h-[2px]` bar (`bg-primary/25` track, `bg-primary shadow-[0_0_8px_rgba(var(--primary),0.5)]` fill, `transition-all duration-300`, width clamped to 100%) plus a `text-3xs font-mono` caption row (`progress.message` left, rounded percent right). Workspace: horizontal `ResizablePanelGroup` (25/18–45 | 75/40) with `ResizableHandle withHandle`; right side hosts the kit `Tabs` (`defaultValue="vulnerabilities"`) inside an `h-9 px-3 border-b bg-muted/15` band; each `TabsTrigger` (`h-6 px-3 text-2xs`) carries an optional `Badge size="sm" variant="outline"` count (`text-4xs font-bold`; vulnerabilities badge tinted `border-warning/20 text-warning-foreground bg-warning/5`).

### `ScanToolbar` (`components/scan-toolbar.tsx`)

Props (all 19, fully controlled, `Readonly<>`): config values + setters, `techniques: Set`, `isRunning`, `progress`, `error`, counts, `hasUrlAndParams`, `onStart/onStop/onClear/onExportJson/onExportCsv`.

- Row 1: `Select value={method}` rendered as an `h-8 w-20` `SelectTrigger leading="tight"` (`rounded-r-none`, `bg-muted/20`, ring suppressed) fused with `-space-x-px` to the URL `Input textSize="xs"` (`rounded-l-none`, `focus-visible:border-primary/50`, placeholder `http://target.com/search?q=`). Error chip: `text-3xs text-destructive font-mono bg-destructive/5 border-destructive/10 max-w-[240px] truncate` with full text in `title`. Running chip: pill `bg-primary/10 text-primary border-primary/20 animate-pulse` with an `animate-ping` 1.5px dot showing `progress.phase || 'Scanning'`. Exports are outline `size="md"` `DownloadIcon` buttons, disabled when `vulnerabilitiesCount === 0`; Clear is `variant="quiet" size="icon"` `h-7 w-7` `TrashIcon` (disabled when no results at all). Start: `size="sm"` with hardcoded emerald classes `bg-emerald-600 hover:bg-emerald-700 text-white … disabled:bg-muted disabled:text-muted-foreground`; Stop: `variant="destructive"` with `SquareIcon fill-current`.
- Row 2: `GearIcon` + "SCAN PARAMETERS" micro-header; Risk `Select` (`h-6 text-3xs w-24`) with options `Low (Few)` / `Medium` / `High (All)`; technique toggles are raw `<button>`s over `Object.keys(TECHNIQUE_LABELS)` — selected: `bg-primary/10 border-primary/30 text-primary hover:bg-primary/15`, unselected: `bg-background border-border hover:bg-muted/50 text-muted-foreground` (`px-2 py-0.5 rounded text-3xs font-medium`).

### `ParametersPanel` (`components/parameters-panel.tsx`)

Props: parameter list + new-param fields + `injectCount` + all mutation callbacks (the inline `onParamValueChange` is defined in `index.tsx` as a `setParameters` map-over-name). Header: "TARGET PARAMETERS" + `Badge` `{injectCount} Active`. Body `ScrollArea` with card rows: stateful styling (`inject` → `bg-muted/30 border-muted-foreground/15 shadow-sm`; off → `bg-muted/5 opacity-70 hover:opacity-100`), `Checkbox h-3.5 w-3.5` (`title="Marked for injection" / "Skip injection testing"`), mono name with location Badge — `getLocationBadgeStyle`: `url` sky, `body` emerald, `header` violet (`bg-{color}-500/10 text-{color}-600 dark:text-{color}-400 border-{color}-500/20`), value `Input textSize="xs" mono h-6`, and a ghost `h-6 w-6` trash button (`hover:text-destructive hover:bg-destructive/5`). Empty state: "No parameters defined" + hint. Sticky footer (`border-t bg-muted/15`): name + value `Input`s (both submit on `Enter` via `onKeyDown`) and full-width **Add Parameter** `Button leading="tight" size="md"` with `PlusIcon`, disabled until the name is non-blank.

### `VulnerabilitiesTab` (`components/vulnerabilities-tab.tsx`)

Props: `{ vulnerabilities, isRunning, selectedVuln, selectedVulnData, onSelectVuln }`. Empty state (no vulns and not running): centered `WarningCircleIcon h-10 w-10 text-muted-foreground/35` + "No vulnerabilities detected". Otherwise a nested horizontal `ResizablePanelGroup` (68/40 | 32/20):

- **Table panel**: kit `Table` (`text-xs`) with sticky header (`bg-muted/95 backdrop-blur-sm shadow-sm`), uppercase `text-3xs` column heads Parameter / Location / Technique / DBMS / Severity / PoC Payload. Rows `cursor-pointer hover:bg-muted/30`, selected `bg-muted/65 font-medium`. Cells: mono parameter name (`max-w-[120px] truncate`), uppercase location, technique via `TECHNIQUE_LABELS[technique] || technique.replace('_', ' ')`, DBMS, severity as a rounded-full `Badge variant="outline"` with `SEVERITY_COLORS[severity]`, PoC truncated `text-3xs mono` with `title`.
- **Details panel** (only when `selectedVulnData`): `bg-card/45` band "VULNERABILITY DETAILS" + severity badge; `ScrollArea` of labeled blocks — Vulnerable Target (mono name + location chip), Attack Vector (technique), DBMS, DBMS Fingerprint (mono `break-all` box, fallback "No fingerprint returned."), Proof of Concept Payload with a copy button toggling `CopyIcon "Copy Payload"` → `CheckIcon "Copied"` (green classes) for 2 s via `setTimeout` — and a `<pre>` (`bg-muted/20 text-3xs font-mono whitespace-pre-wrap break-all max-h-48`) holding `poc_request`. Copy uses `navigator.clipboard.writeText` directly (not the shared clipboard hook).

### `ExtractionTab` (`components/extraction-tab.tsx`)

Props: `{ databases, isRunning, selectedDb, selectedTable, selectedDbData, tableData, onSelectDb, onSelectTable }`. Empty state: pulsing `DatabaseIcon` + "No databases extracted". Otherwise a 3-panel horizontal `ResizablePanelGroup` (20/15 | 20/15 | 60/30):

- **Databases**: header band with count `Badge size="sm" mono variant="secondary"`; column of full-width buttons (`DatabaseIcon`, selected → `bg-muted/70 font-semibold text-primary` icon); selecting a DB resets `selectedTable` to `null`.
- **Tables**: same pattern with `TableIcon` plus a row-count Badge per table; "Select database" placeholder when none.
- **Data**: header shows `Data: {selectedTable || 'Select Table'}` and a `{n} rows loaded` badge (`bg-primary/5 text-primary border-primary/20`); body renders kit `Table` with per-column heads `{col.name} ({col.data_type})` (type in `text-4xs mono`), rows **sliced to the first 100** (`tableData.rows.slice(0, 100)`), mono `text-2xs` cells where empty/null render as italic `text-muted-foreground/40` "NULL"; "No data records in this table." / "Select a table from the sidebar to display records." placeholders. There is no pagination control beyond the 100-row slice.

## 7. UI Libraries & Primitives Used

### `@celestia-project/ui` components (exact imports)

| Component | Used in |
|---|---|
| `Badge` (props used: `size="sm"`, `variant` `outline/secondary`, `mono`) | counts, location/severity, rows loaded |
| `Button` (props used: `variant` `outline/quiet/destructive`, `size` `md/sm/icon`, `leading="tight"`) | toolbar + add parameter |
| `Checkbox` | per-parameter inject toggle |
| `Input` (props used: `textSize="xs"`, `mono`) | URL, parameter values, add form |
| `Label` (props used: `size="2xs"` in risk row; plain elsewhere) | details labels, Risk |
| `ScrollArea` | parameter list, details, db/table/data columns |
| `Select`, `SelectContent`, `SelectItem`, `SelectTrigger`, `SelectValue` (props used: `leading="tight"`) | method + risk |
| `Table`, `TableBody`, `TableCell` (props used: `size="sm"`, `mono`), `TableHead`, `TableHeader`, `TableRow` | vulnerabilities + data tables |
| `Tabs`, `TabsContent`, `TabsList`, `TabsTrigger` | results tabs |

### Radix primitives

None imported directly by page code (the kit wraps Radix for Select/Tabs/Checkbox/ScrollArea internally). The resizable splitter is **not** Radix — see below.

### Phosphor icons (`@phosphor-icons/react`, exact names)

| Icon | Used in |
|---|---|
| `DownloadIcon`, `PlayIcon`, `SquareIcon`, `TrashIcon`, `GearIcon` | `scan-toolbar.tsx` |
| `TrashIcon`, `PlusIcon` | `parameters-panel.tsx` |
| `WarningCircleIcon`, `CopyIcon`, `CheckIcon` | `vulnerabilities-tab.tsx` |
| `DatabaseIcon`, `TableIcon` | `extraction-tab.tsx` |

### Editors, virtualization, other third-party

| Library | Usage |
|---|---|
| `react-resizable-panels` v4 via `@/components/ui/resizable` (`ResizablePanelGroup/Panel/Handle withHandle`, string sizes `"25"`/`minSize`/`maxSize`, `DotsSixVerticalIcon`/`DotsSixIcon` handles) | page split, vulns split, extraction 3-column split |
| `clsx` + `tailwind-merge` (`cn`), template-literal class strings | styling |
| `@tauri-apps/api/core` (`invoke`), `api/event` (`listen`, `UnlistenFn`) | IPC |
| `crypto.randomUUID` | client-side `scan_id` generation |
| Browser APIs: `Blob` + `URL.createObjectURL` + anchor download (exports), `navigator.clipboard` (PoC copy) | no Tauri dialog/fs plugins used here |
| `toErrorMessage` (`@/lib/ipc`) | error normalization |

No editor component (Monaco/CodeMirror `TextEditor`) and no virtualization library are used on this page — tables render plain mapped rows.

## 8. Keyboard Shortcuts & Interactions

No global or page-level hotkeys. Keyboard interactions:

| Where | Key | Behavior |
|---|---|---|
| "Add Custom Parameter" name/value `Input`s | `Enter` | calls `onAddParameter` (adds the parameter if name non-blank) |
| All kit `Select`/`Tabs`/`Checkbox` | native Radix keyboard support | inherited from the UI kit |

Pointer interactions: row clicks select a vulnerability; db/table buttons update selection (table selection also drives the data grid); technique chips toggle; resizable handles on all three splits.

## 9. Styling Details

- **Severity colors** (`constants.ts` `SEVERITY_COLORS`): `critical` → `border-red-500/20 text-red-600 dark:text-red-400 bg-red-500/5`; `high` → orange; `medium` → yellow; `low` → blue (same `/20 /5` alpha pattern).
- **Location badge colors** (`getLocationBadgeStyle`): `url` sky, `body` emerald, `header` violet, fallback muted — same `500/10 + 600 dark:400 + 500/20` recipe as Intruder's status chips.
- **Technique labels** (`TECHNIQUE_LABELS`): `boolean_blind` → "Boolean Blind", `time_based` → "Time-Based", `union` → "UNION-Based", `error_based` → "Error-Based".
- Semantic tokens: `primary` for progress bar/fill/phase chip/selected icons; `warning` for the vulnerability tab count badge; `destructive` for errors/Stop/delete; `muted/N` banding (`bg-muted/15`, `/20`, `/95` sticky headers with `backdrop-blur-sm`); `bg-card`/`bg-card/45` surfaces.
- Type scale: heavy use of `text-2xs`/`text-3xs`/`text-4xs` micro type with `uppercase tracking-wider font-bold` section headers; `font-mono` for identifiers, PoC payloads, table cells, error/phase chips.
- Custom shadows: progress fill `shadow-[0_0_8px_rgba(var(--primary),0.5)]` (references the `--primary` CSS variable), sticky table headers `shadow-sm`.
- Source comments marked `// ponytail:` appear in three components as simplification notes (toolbar, parameters panel, extraction/vulns empty states).

## 10. Integration Points

- **Routes**: `/sql-injection` renders `SqlInjectionPage` via `src/App.tsx:143`, the desktop workspace lazy map (`layout/desktop-workspace/page-lazy-imports.ts:46`), and standalone mode `routes/page-resolver.tsx:210-211` (`StandaloneLayout id="/sql-injection" title="SQLi Helper"`).
- **No send-to integration**: unlike Repeater/Intruder, there is no "Send to SQLi" from HTTP history or any trigger module — `grep` finds no references to this page in `src/triggers/` or the HTTP-history code. Requests are configured entirely by hand in this page.
- **Backend coupling is limited to the two commands and one event** in section 5; the scan's actual detection logic (`SqliDetector`, `payloads`, `types`) lives in `src-tauri/src/sqli/` and mirrors the frontend `SqliProgressEvent`/`SqliScanResult` shapes.
- **No shared stores**: the page touches neither `useAppStore` nor any feature store; nothing from `@/triggers`, no toast (`sonner`) usage — errors surface only in the toolbar chip.

## 11. Source Files Documented

- `src/pages/sql-injection/index.tsx`
- `src/pages/sql-injection/types.ts`
- `src/pages/sql-injection/constants.ts`
- `src/pages/sql-injection/hooks/use-sqli-page.ts`
- `src/pages/sql-injection/lib/export-helpers.ts`
- `src/pages/sql-injection/components/scan-toolbar.tsx`
- `src/pages/sql-injection/components/parameters-panel.tsx`
- `src/pages/sql-injection/components/vulnerabilities-tab.tsx`
- `src/pages/sql-injection/components/extraction-tab.tsx`

Shared modules read for usage documentation:

- `src/components/ui/resizable.tsx`, `src/lib/ipc.ts`, `src/lib/utils.ts`
- `src/App.tsx`, `src/routes/page-resolver.tsx`, `src/layout/desktop-workspace/page-lazy-imports.ts` (routing)
- `src-tauri/src/sqli/mod.rs` (command signatures + event emission)
