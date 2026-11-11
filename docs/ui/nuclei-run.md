# Nuclei Runner (nuclei-run) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/nuclei-run/` (47 files, ~13,840 lines — the largest page module in Halo)

## 1. Overview & Purpose

`nuclei-run` implements Halo's Nuclei vulnerability-scanner UI, a Burp-Suite-style "Scanner" page driven by a Rust Nuclei engine over Tauri IPC. The module covers:

- **Template Hub** — browsing/filtering/selecting Nuclei YAML templates from the bundled catalogue (26 built-in `DEFAULT_TEMPLATES`) or from a synced copy of the official `projectdiscovery/nuclei-templates` GitHub repository (fetched via backend, cached on disk).
- **Visual Flow DAG** — a read-only React Flow canvas that parses template YAML into a node graph (template info → request probes → extractors/matchers → v3 flow logic), with a hand-written YAML line parser (`ast-translator.ts`), deterministic hierarchical layout (`dag-layout.ts`), and graph validation (`graph-validator.ts`).
- **Staging step** — grouping selected templates (directory/severity/category/saved sets), editing scan targets, saving reusable template sets to `localStorage`, and firing all/group/single on-demand scans.
- **Scan Cockpit** — live telemetry (progress %, RPS, elapsed), findings table with severity/protocol filters, streaming console log, attack-flow DAG of the first selected template, and a finding detail drawer (cURL reproduction, "Send to Repeater"/"Comparer" integration).
- **Legacy satellite components** — an older toolbar, template hub, template studio (Monaco-based authoring + simulated test harness), template selector, stats banner, config dialog and export dialog. These are still fully implemented and exported but **not mounted** by the current 3-step page (see §6.8).

The page is registered lazily in `src/App.tsx` for routes `/nuclei-run`, `/nuclei` and `/scanner`, and in `src/routes/page-resolver.tsx` as `case "nuclei-run"` inside `StandaloneLayout` with title `"Scanner"`.

All state lives in a single Zustand store `src/stores/nuclei.ts` (`useNucleiStore`). Icons are **Phosphor** (`@phosphor-icons/react` v2.1.10), not lucide-react. The UI kit is an internal package `@celestia-project/ui` v0.4.0 (shadcn-style components). Editors come from the same kit's `TextEditor` (Monaco 0.55.1 under the hood per `package.json`). List virtualization is `@tanstack/react-virtual` v3.14.2; the flow canvas is `@xyflow/react` v12.11.0.

## 2. Annotated File Tree — every file with a one-line description

```
src/pages/nuclei-run/
├── index.tsx                                  (197 L)  NucleiRunPage: 3-step workflow shell — header bar, TabbedPageLayout, legacy-tab alias normalization, quick nav buttons
├── types.ts                                   (165 L)  All domain types: Severity, ProtocolType, ScanStatus, ScanPreset, NucleiTab, NucleiFinding, TemplateItem, NucleiScanConfig, ScanProgress, ScanLogEntry, ScanSummaryStats, TemplateValidationResult, TemplateTestResult
├── constants.ts                               (160 L)  SEVERITY_CONFIG (per-severity Tailwind classes), PROTOCOL_BADGES, PRESET_OPTIONS (5 scan presets), TEMPLATE_CATEGORIES, DSL_HELPERS_REFERENCE (12 Nuclei DSL helpers)
├── constants/
│   └── categories.ts                          (263 L)  Hub taxonomy: NUCLEI_DIRECTORIES (13), POPULAR_TAGS (18), TOP_AUTHORS (10), PROTOCOL_TYPES (11), RESEARCHER_STRATEGIES (5 phased strategies), TECH_STACKS (16 stack defs)
├── hooks/
│   ├── use-nuclei-scan.ts                     (316 L)  Scan lifecycle: parseTargets, startScan/stopScan/pauseScan/resumeScan, event subscription wiring to store + notifications
│   ├── use-nuclei-run-page.ts                 (223 L)  LEGACY aggregate hook (exported as useNucleiPage; not used by current page) — filters, finding actions, send-to-Repeater navigation
│   └── use-template-studio.ts                 (115 L)  LEGACY studio hook — live YAML validation, example loading, placeholder insertion, simulated (setTimeout) template test
├── lib/
│   ├── nuclei-ipc.ts                          (204 L)  Tauri IPC façade: invoke wrappers for 8 commands + subscribeNucleiEvents / listenToSyncProgress for 6 events
│   ├── template-validator.ts                  (212 L)  Line-based YAML linter: requires top-level id + info block + protocol block; emits ValidationDiagnostic[] and metadata
│   ├── formatters.ts                          (180 L)  Report generators: SARIF v2.1.0, JSONL, CSV, Markdown summary; generateCurlCommand fallback
│   ├── default-templates.ts                   (905 L)  DEFAULT_TEMPLATES: 26 fully-YAML-embedded TemplateItem entries (CVEs, exposures, misconfigs, default-logins, DAST probes)
│   ├── template-validator.test.ts             (370 L)  Vitest: validator acceptance/empty/mandatory-fields/severity/id-format/defaults/protocol/info-scoping
│   ├── formatters.test.ts                     (277 L)  Vitest: curl/jsonl/csv/escaping/SARIF rules & results/markdown
│   └── default-templates.test.ts              (187 L)  Vitest: catalogue well-formedness (unique TS & YAML ids, agreement), all 26 pass validator + parser
├── components/
│   ├── nuclei-template-hub-step.tsx           (1134 L)  Step 1: 3-pane hub — GitHub sync card, 6 taxonomy tabs, virtualized template list (64 px rows), YAML/Flow inspector (440 px)
│   ├── nuclei-selected-templates-step.tsx     (681 L)   Step 2: target input, rate/threads badges, grouping modes (directory/severity/category/saved), saved-set cards, scan-all/group/single, inspector (420 px)
│   ├── nuclei-scan-cockpit.tsx                (388 L)   Step 3: scan controls (run/pause/resume/stop), telemetry strip, findings table, collapsible console/flow bottom panel (h-64/h-8), finding drawer
│   ├── nuclei-run-findings-table.tsx          (409 L)   Findings table: search input, protocol filter chips, severity dot badges, extracted-evidence badges, Repeater/cURL row actions
│   ├── nuclei-run-finding-detail-drawer.tsx   (492 L)   Finding side drawer: severity/protocol/CVE header, Repeater/Comparer/cURL bar, matched endpoint, extracted evidence, remediation, tags, metadata footer
│   ├── nuclei-run-console-stream.tsx          (235 L)   Terminal console: log filter input, auto-scroll Switch, clear button, per-level colored lines
│   ├── nuclei-run-toolbar.tsx                 (172 L)   LEGACY toolbar (view tabs Templates/Flow + target input + run toggle)
│   ├── nuclei-run-template-hub.tsx            (387 L)   LEGACY template hub (category chips, search, card grid)
│   ├── nuclei-run-template-studio.tsx         (575 L)   LEGACY template studio: Monaco YAML editor + DSL placeholder chips + diagnostics footer + Target Test Harness (Monaco http/json result panes)
│   ├── nuclei-template-selector.tsx           (660 L)   LEGACY selector: strategy/tech-stack sidebar, severity pills, virtualized card list (104 px rows, measureElement), footer selection bar
│   ├── nuclei-run-stats-banner.tsx            (231 L)   LEGACY HUD strip: status pulse, Progress bar, RPS/time, clickable severity-count filter buttons
│   ├── nuclei-run-config-dialog.tsx           (316 L)   LEGACY config dialog: concurrency (-c), rate limit (-rl), timeout, retries, upstream proxy, exclusions Textarea, headless & follow-redirects Switches
│   └── nuclei-run-export-dialog.tsx           (279 L)   LEGACY export dialog: SARIF/JSONL/CSV/Markdown tabs, Monaco TextEditor preview, Copy + Blob Download
└── flow/
    ├── index.ts                               (11 L)    Barrel: re-exports types, constants, nodes, edge, canvas, toolbar, inspector, hook, libs
    ├── types.ts                               (161 L)   NucleiNodeType/'NucleiEdgeType unions, per-node data interfaces, NucleiFlowNodeOf correlated node type, FlowDiagnostic, PaletteItem
    ├── constants.ts                           (108 L)   PALETTE_ITEMS (5), EDGE_STYLES per edge type, DEFAULT_* node blueprints
    ├── hooks/
    │   └── use-nuclei-flow.ts                 (81 L)    Read-only flow state: useNodesState/useEdgesState, YAML→graph+auto-layout effect, validator effect, selectedNode, templateName, autoLayout
    ├── components/
    │   ├── nuclei-flow-canvas.tsx             (119 L)   NucleiFlowCanvas: ReactFlowProvider + shared FlowCanvas (read-only: nodesConnectable=false, snapGrid [16,16], minimap/controls/background), fitView on YAML change, slide-over inspector
    │   ├── nuclei-flow-toolbar.tsx            (145 L)   Flow toolbar: template name, error/warning/"DAG Flow Ready" badge, Auto Layout + Fit buttons
    │   └── nuclei-flow-inspector.tsx          (314 L)   Read-only slide-over per-node inspector (templateInfo/requestNode/matcherNode/extractorNode/flowNode branches), InspectableNucleiNode correlated type
    ├── nodes/
    │   ├── index.ts                           (22 L)    NUCLEI_NODE_TYPES map for React Flow
    │   ├── template-info-node.tsx             (164 L)   Root metadata node: severity badge, name/id, description, author, first-2 tags, bottom source handle
    │   ├── request-node.tsx                   (171 L)   Probe node: METHOD_COLORS map, first path, header count/has-body, Stop-First flag, top+left target handles
    │   ├── extractor-node.tsx                 (162 L)   Extractor node: pattern preview (regex/json/dsl/kval), part, Internal (EyeClosedIcon) flag, right "variable-out" handle
    │   ├── matcher-node.tsx                   (167 L)   Matcher node: status/word/regex value preview, condition+part, Inverted flag, dual bottom handles (condition-true 32% / condition-false 68%)
    │   └── flow-node.tsx                      (143 L)   Nuclei v3 flow node: flowCode block, "v3 Flow" badge, JS Flow Expression footer
    ├── edges/
    │   └── nuclei-flow-edge.tsx               (86 L)    NucleiFlowEdgeView: BaseEdge + getBezierPath, EDGE_STYLES, selected glow (drop-shadow), EdgeLabelRenderer chip; NUCLEI_EDGE_TYPES map (4 aliases)
    └── lib/
        ├── ast-translator.ts                  (672 L)   nucleiYamlToGraph (line-based YAML parser → nodes/edges) and graphToNucleiYaml (serializer back to Nuclei v3 YAML)
        ├── dag-layout.ts                      (117 L)   calculateDagLayout: BFS ranking (templateInfo = rank 0), cycle-safe bound, column-centering coordinates
        ├── graph-validator.ts                 (147 L)   validateNucleiGraph: root/probe presence, unresolved {{var}} warnings vs builtin list, disconnected nodes, DFS cycle detection
        ├── ast-translator.test.ts             (949 L)   Vitest: empty input, metadata extraction, reference/description block-scalar regressions, protocol detection, round-trip, all built-ins
        ├── dag-layout.test.ts                 (137 L)   Vitest: layout determinism/rank assignment
        └── graph-validator.test.ts            (~240 L)  Vitest: structure, duplicate roots, unresolved variables, disconnected, cycles (diamond OK, 2-cycle/self-loop errors)
```

## 3. Architecture & Dataflow

Three cooperating dataflows: (A) the scan lifecycle over Tauri IPC, (B) the template catalogue sync/cache, (C) the read-only YAML→DAG visualization pipeline (pure client-side, no IPC).

```mermaid
flowchart TD
    subgraph UI["UI layer (src/pages/nuclei-run)"]
        IDX["NucleiRunPage (index.tsx)"]
        HUB["NucleiTemplateHubStep"]
        SEL["NucleiSelectedTemplatesStep"]
        CKPT["NucleiScanCockpit"]
        FT["NucleiRunFindingsTable"]
        DRAW["NucleiRunFindingDetailDrawer"]
        CS["NucleiConsoleStream"]
        FC["NucleiFlowCanvas"]
    end

    subgraph HOOKS["Hooks"]
        SCAN["useNucleiScan"]
        FLOW["useNucleiFlow"]
    end

    subgraph STORE["Zustand (src/stores/nuclei.ts)"]
        NS[("useNucleiStore<br/>status, progress, findings,<br/>logs, templates, selectedTemplateIds,<br/>config, syncStatus, studio*, filters")]
    end

    subgraph BACK["Tauri Rust backend"]
        CMD["commands: start/pause/resume/stop_nuclei_scan,<br/>get_nuclei_status, sync_official_nuclei_templates,<br/>get_cached_official_templates, read_template_yaml"]
        EVT["events: nuclei://scan-started · progress ·<br/>finding · scan-error · scan-completed · sync-progress"]
    end

    subgraph OTHER["Cross-module stores"]
        NOTIF[("useNotificationStore<br/>addAlert")]
        REP[("useRepeaterStore<br/>createWorkspace / setActiveWorkspaceId")]
        ROUTER["react-router navigate('/repeater' | '/comparer')"]
    end

    IDX --> HUB & SEL & CKPT
    HUB & SEL -->|toggleTemplateSelection, syncFromGitHub, readTemplateYaml| NS
    HUB & SEL & CKPT -->|yamlContent| FC
    FC --> FLOW
    FLOW -->|"nucleiYamlToGraph → calculateDagLayout"| FC
    FLOW -->|"validateNucleiGraph → diagnostics"| FC
    CKPT --> SCAN
    SEL --> SCAN
    IDX --> SCAN
    SCAN -->|setStatus/setProgress/addFinding/addLog/setScanId/setActiveTab| NS
    SCAN -->|subscribeNucleiEvents| EVT
    SCAN -->|startNucleiScan(pause/resume/stop)| CMD
    NS -->|syncFromGitHub / checkCachedGitHubTemplates| CMD
    HUB -->|readTemplateYaml(source_path)| CMD
    CMD --> EVT
    EVT -->|payloads| SCAN
    SCAN --> NOTIF
    CKPT --> FT & DRAW & CS
    FT -->|onSelectFinding| NS
    DRAW -->|onSendToRepeater| REP
    REP --> ROUTER
```

**Scan lifecycle detail** (`use-nuclei-scan.ts`):
1. `startScan(overrideTemplateIds?)` parses `targetInput` (`split(/[\n,]+/)`; skips `#` comments; prefixes `http://` for localhost/127.0.0.1/0.0.0.0/::1 hosts, `https://` otherwise), validates targets and templates, then immediately `setActiveTab('results')`.
2. A new `scanId = scan-${Date.now()}` is generated, `status='running'`, `progress` reset with `total_requests = targets.length × selectedTemplates.length`.
3. `subscribeNucleiEvents(...)` registers the 5 event listeners **before** `startNucleiScan` is invoked. The `unlisten` function is kept in `unlistenRef` and re-cleaned on each start/unmount.
4. Selected templates are split by `source_path`: disk templates (non-`memory://`) go to `template_paths`, others contribute `yaml_content` to `raw_templates` (both optional arrays in `UiScanConfig`).
5. `onProgress` recomputes `percentage` and a client-side `elapsed_seconds`; a `[SCANNING] x/y requests (pct%) at N RPS` log line is appended at most every 1500 ms (`lastProgressLogRef`).
6. `onFinding` builds a `NucleiFinding` with client-side `id = finding-${Date.now()}-${rand5}`, hardcoded `protocol: 'http'`, `matched_at = new Date().toISOString()` and a default `curl_command = curl -s "<url>"`; the store's `addFinding` deduplicates on `(template_id, matched_url)`. A `[DISCOVERED] [SEVERITY]` log plus a warning toast are emitted.
7. `onScanCompleted` sets `status='completed'` and logs `Scan finished in Xs. Total vulnerabilities discovered: N.`; errors set `status='error'` after a failed `startNucleiScan`.

**Flow visualization detail** (`flow/`): `NucleiFlowCanvas(yamlContent, hideToolbar?)` wraps `useNucleiFlow({ initialYaml })`. On YAML change: `nucleiYamlToGraph` → `calculateDagLayout` → `setNodes/setEdges`; on every nodes/edges change `validateNucleiGraph` produces `FlowDiagnostic[]` shown in the toolbar badge. Node click sets `selectedNodeId`; pane click clears it; the inspector renders read-only. `fitView({ duration: 300, padding: 0.25 })` runs 100 ms after each YAML change and after Auto Layout (50 ms).

## 4. State Management

Single store: `useNucleiStore` (`src/stores/nuclei.ts`, plain `create`, **no persist middleware**; only `savedGroups` is hand-persisted to `localStorage` under key `nuclei_saved_groups`). Fields the page touches:

| Field | Type / default | Written by | Read by |
|---|---|---|---|
| `activeTab` | `NucleiTab`, default `'templates'` (legacy alias) | `setActiveTab` from index.tsx, hub/selected steps, `useNucleiScan.startScan` (→ `'results'`) | index.tsx `effectiveTab` normalization |
| `targetInput` | `string`, `''` | `setTargetInput` (selected-step header, cockpit header) | `useNucleiScan.parseTargets`, cockpit/selected inputs |
| `preset` | `ScanPreset`, `'quick-triage'` | `setPreset` — copies the preset's `concurrency`/`rateLimit`/`timeout` into `config` | config surfaces |
| `config` | `NucleiScanConfig`, `DEFAULT_CONFIG` (`concurrency: 25`, `rate_limit_rps: 150`, `timeout_seconds: 10`, `retries: 1`, `max_redirects: 10`, `custom_headers: {}`, `excluded_targets: []`, `headless: false`, `follow_redirects: true`, `template_ids: DEFAULT_TEMPLATES ids`) | `setConfig(partial)`, `resetConfig` | badges in selected-step/cockpit, `startScan` payload |
| `status` | `ScanStatus` `'idle' \| 'running' \| 'paused' \| 'completed' \| 'cancelled' \| 'error'` | `useNucleiScan` | cockpit control buttons/status badge, index.tsx alias logic |
| `progress` | `ScanProgress` `{completed_requests, total_requests, rps, percentage, elapsed_seconds}` | `setProgress` from `onProgress` | cockpit telemetry strip, stats banner |
| `scanId` | `string \| null` | `setScanId` (`scan-${Date.now()}`) | surfaced by `useNucleiScan` |
| `findings` | `NucleiFinding[]`, `[]` | `addFinding` (dedupes on `template_id + matched_url`, prepends), `setFindings`, `clearFindings` | findings table, drawer, counts everywhere |
| `selectedFindingId` | `string \| null` | `setSelectedFindingId` | cockpit drawer mount |
| `logs` | `ScanLogEntry[]` | `addLog` — caps at last **1000** entries (`slice(-1000)`), id `log-${Date.now()}-${rand4}`, `timestamp = new Date().toLocaleTimeString()` | console stream |
| `autoScrollConsole` | `boolean`, `true` | `setAutoScrollConsole` (console Switch) | console scroll effect |
| `templates` | `TemplateItem[]`, initially `DEFAULT_TEMPLATES` (26) | `setTemplates` via `syncFromGitHub` / `checkCachedGitHubTemplates` (merged, cached first + non-duplicate defaults appended; converted entries get `protocol: 'http'`, `yaml_content: ''`) | hub list, selected grouping, cockpit first-template YAML |
| `selectedTemplateIds` | `string[]`, initially **all 26** default template ids | `toggleTemplateSelection`, `selectAllTemplates`, `deselectAllTemplates`, `selectTemplatesBySeverity`, `toggleGroupSelection`, `selectCategoryTemplates`, direct `useNucleiStore.setState` in hub/selector "select all filtered" | counts, scan payloads |
| `activeInspectorTemplateId` | `string \| null` | `setActiveInspectorTemplateId` (row click) | hub & selected right panes |
| `savedGroups` | `SavedTemplateGroup[]`, lazily read from `localStorage('nuclei_saved_groups')` | `saveCurrentSelectionAsGroup` (id `group-${Date.now()}`, persists to localStorage), `deleteSavedGroup` (persists) | saved-sets view |
| `activeCategory` | `string`, `'recon-first'` | `setActiveCategory` | hub tech-stack filter, selector strategies |
| `syncStatus` | `{isSyncing, progressMessage, totalTemplates, lastSyncedAt?, cachePath?, error?}` | `setSyncStatus`, `syncFromGitHub(force=false)`, `checkCachedGitHubTemplates()` | hub sync card (button disabled + `animate-bounce` icon while syncing) |
| `studioYaml / studioTarget / studioDiagnostics / studioTestResult / isTestingTemplate` | studio state (`studioYaml` initialized from `DEFAULT_TEMPLATES[0].yaml_content`) | legacy `useTemplateStudio` | legacy template studio |
| `findingSearchQuery`, `severityFilter: string[]`, `protocolFilter: string[]` | filter state | cockpit search input, `toggleProtocolFilter`, legacy page hook | findings table, `getSummaryStats` consumers |
| `getSummaryStats()` | derived `ScanSummaryStats` | — | legacy stats banner/export dialog. Quirk: hardcodes `targets_count: 1` and `templates_count: DEFAULT_TEMPLATES.length` |

**Category selection logic** (`selectCategoryTemplates`): `'all'` → everything; `'recon-first'` → tags in `[tech, panel, exposure, detection, recon]`; `'cves-critical-high'` → severity critical/high AND (`cve_id` or tag contains `cve`); `'recent-cves'` → tag/cve contains `2025`/`2026`; `'dast-fuzzing'` → tags in `[dast, fuzzing, xss, sqli, lfi, ssrf]`; `'cloud-token-leaks'` → tags in `[token, cloud, aws, s3, azure, credentials]`; fallback substring match on tag/name/description. Empty matches leave selection unchanged.

## 5. IPC Contracts

All IPC is funneled through `lib/nuclei-ipc.ts` using `@tauri-apps/api/core` `invoke` and `@tauri-apps/api/event` `listen`.

### 5.1 Commands invoked

| Command | Args (JSON key: type) | Returns | Called from |
|---|---|---|---|
| `start_nuclei_scan` | `config: UiScanConfig { targets: string[], template_paths?: string[], raw_templates?: string[], concurrency: number, rate_limit_rps: number, timeout_seconds: number }` | `Promise<void>` | `useNucleiScan.startScan` (after event subscription) |
| `pause_nuclei_scan` | — | `Promise<void>` | `useNucleiScan.pauseScan` (only when `status === 'running'`) |
| `resume_nuclei_scan` | — | `Promise<void>` | `useNucleiScan.resumeScan` (only when `status === 'paused'`) |
| `stop_nuclei_scan` | — | `Promise<void>` | `useNucleiScan.stopScan` |
| `get_nuclei_status` | — | `Promise<string>` | exported wrapper `getNucleiStatus` (no current UI caller) |
| `sync_official_nuclei_templates` | `force: boolean` | `Promise<SyncResultPayload { total_templates: number, cache_path: string, templates: Array<{ id, name, severity, protocol, tags[], description, author, category, source_path? }> }>` | `useNucleiStore.syncFromGitHub` (hub sync button) |
| `get_cached_official_templates` | — | `Promise<CachedTemplatesPayload { is_cached, total_templates, cache_path?, templates: [...] }>` | `useNucleiStore.checkCachedGitHubTemplates` (hub `useEffect` on mount) |
| `read_template_yaml` | `sourcePath: string` (camelCase arg key) | `Promise<string>` (raw YAML) | `NucleiTemplateHubStep` & `NucleiSelectedTemplatesStep` inspector effect, for templates whose `source_path` exists and does not start with `memory://` |

### 5.2 Events listened

All are subscribed in `subscribeNucleiEvents(listeners): Promise<() => void>` (returns a combined unlisten); the sync event has its own helper `listenToSyncProgress`.

| Event name | Payload type | Handler behavior |
|---|---|---|
| `nuclei://scan-started` | `RustScanStartedPayload { total_templates: number, total_targets: number }` | log: `Rust scanner active: N templates initialized for M target(s).` |
| `nuclei://progress` | `RustProgressPayload { completed_requests: number, total_requests: number, rps: number }` | recompute `percentage` + client-side elapsed; throttled `[SCANNING]` log (≥1500 ms) |
| `nuclei://finding` | `RustFindingPayload { template_id, template_name, severity: string, matched_url, matched_at, extracted_results: string[] }` | build `NucleiFinding` (protocol forced `'http'`, severity lowercased with `'medium'` fallback), `addFinding`, `[DISCOVERED]` log, warning toast |
| `nuclei://scan-error` | `RustScanErrorPayload { target: string, message: string }` | error log `[ERROR] [target] message` |
| `nuclei://scan-completed` | `RustScanCompletedPayload { elapsed_millis: number, total_findings: number }` | `status='completed'`, success log + success toast |
| `nuclei://sync-progress` | `RustSyncProgressPayload { status: 'downloading' \| 'indexing' \| 'completed' \| 'error', message: string, total?: number }` | exported helper `listenToSyncProgress` (available to the hub; the store's sync path currently awaits the command result directly) |

`toErrorMessage(err, fallback)` from `src/lib/ipc.ts` stringifies sync errors into `syncStatus.error`.

## 6. UI Layout & Component Breakdown

### 6.0 Page shell (index.tsx — `NucleiRunPage`)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ header h-10 px-3 border-b bg-muted/15                                        │
│  [ScanIcon Nuclei Run] | [Badge mono outline: "Step N: …"]   …right: nav     │
│  badge text switches by effectiveTab:                                        │
│    hub → "Step 1: Explore Hub & Flow DAG"                                    │
│    selected → "Step 2: Staging & On-Demand Scan (N)"                         │
│    results → "Step 3: Scan Telemetry & Findings (N)"                         │
│  right side (state machine):                                                 │
│    always: findings>0 → clickable destructive Badge "{n} Discovered" (BugIcon)│
│    hub:   Button xs "Review Selected (n)" + ArrowRightIcon (→ selected)      │
│    selected: outline "Add Templates" (ArrowLeftIcon, → hub)                  │
│              + primary "Scan All (n)" (PlayIcon, startScan())                │
│    results: outline "Selected Templates (n)" (ArrowLeftIcon, → selected)     │
├──────────────────────────────────────────────────────────────────────────────┤
│ TabbedPageLayout → PageTabBar (border-b border-primary)                      │
│  tabs: "1. Template Hub" · "2. Selected Templates (n)" · "3. Scan Results(N)"│
│  all closable:false; content p-0 m-0 border-0                                │
├──────────────────────────────────────────────────────────────────────────────┤
│ body (flex-1 min-h-0): NucleiTemplateHubStep | NucleiSelectedTemplatesStep   │
│                         | NucleiScanCockpit                                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

`effectiveTab` normalizes legacy ids: `'templates'`→`'hub'`, `'scan'`→`'selected'` if templates staged & idle else `'results'`, `'flow'`→`'hub'`. `startScan` from `useNucleiScan` is invoked directly by the header button.

### 6.1 NucleiTemplateHubStep (Step 1) — `components/nuclei-template-hub-step.tsx`

Props: `{ onContinueToSelected?: () => void }`.

```
┌───────────────┬───────────────────────────────────────┬──────────────────────┐
│ aside w-72    │ main flex-1                           │ aside w-[440px]      │
│ bg-muted/10   │ ┌ filter bar p-2.5 (border-b) ┐       │ (only when a         │
│ border-r      │ │ search (pl-8, mono, h-8)    │       │ template is active)  │
│               │ │ Clear Filters · Select All(n)│      │                      │
│ GitHub sync   │ │ Deselect All · Review →     │       │ header: severity     │
│ card p-3:     │ └─────────────────────────────┘       │ badge + id + X       │
│  "Official    │ ┌ filter chips strip ─────────┐       │ view switcher:       │
│   Nuclei Hub" │ │ "Showing X of Y templates"  │       │  [YAML View|Visual   │
│  Badge        │ │ chips: Dir/Tag/Author/Proto/│       │   (Flow)]  Copy YAML │
│  "Community   │ │ sev chips with ✕  | n Staged│       │ body:                │
│   v3"         │ └─────────────────────────────┘       │  YAML → <pre> mono   │
│  Sync GitHub  │ ┌ virtualized list (64 px rows) ┐     │  text-xs zinc-300    │
│  Templates    │ │ checkbox+severity badge+id   │     │  bg-black/90         │
│  ({n} loaded) │ │ name · author · tags(≤3)     │     │  Flow → NucleiFlow   │
│  progress msg │ │ Select/Selected button       │     │  Canvas              │
│               │ └──────────────────────────────┘     │ footer: author/proto │
│ taxonomy pills│ empty state: CirclesFourIcon +       │ + Add to Selection / │
│ (6 buttons) + │ "No templates match…" + reset        │ Remove from Staging  │
│ ScrollArea    │                                      │                      │
│ fill items    │                                      │                      │
└───────────────┴───────────────────────────────────────┴──────────────────────┘
```

Key behaviors:
- **Taxonomy tabs** (`TaxonomyTab`): `directories | tags | tech | severities | protocols | authors`. Each item button is `px-2.5 py-1.5 rounded-md text-xs`; active = `bg-primary/15 text-primary font-semibold border border-primary/30`; live counts are precomputed in `directoryCounts` / `tagCounts` / `authorCounts` / `protocolCounts` `Map`s. Directories default `t.directory || 'http'`.
- **Filter pipeline** (`filteredTemplates` memo): search (name/id/cve/tags/author via `useDeferredValue`), multi-select severities, single directory, single tag, single author, single protocol, and `activeCategory` (tech stack tag-contains OR strategy tag-contains).
- **Virtualization**: `useVirtualizer({ count, getScrollElement, estimateSize: () => 64, overscan: 10 })`; rows absolutely positioned via `translateY(${virtualRow.start}px)`; row click = inspect, checkbox click `stopPropagation()` = toggle select; selected row `bg-muted/15`, inspected row `bg-primary/10 border-primary/30`.
- **Select All (filtered)** writes directly: `useNucleiStore.setState({ selectedTemplateIds: Array.from(new Set([...selectedTemplateIds, ...filteredIds])) })`.
- **Inspector YAML loading effect**: uses `yaml_content` if present; otherwise `readTemplateYaml(source_path)` with `isCancelled` guard and error fallback comment text `# Failed to read file: ${err}`; `memory://` paths render `# No YAML available for template: ${id}`.
- **Inspector Flow mode** mounts `NucleiFlowCanvas yamlContent={inspectorYaml} hideToolbar={false}`.
- Copy YAML uses `navigator.clipboard.writeText` with a 2 s `copiedYaml` flag.

### 6.2 NucleiSelectedTemplatesStep (Step 2) — `components/nuclei-selected-templates-step.tsx`

Props: `{ onBackToHub?, onContinueToResults? }`.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ header p-3 bg-muted/15: [GlobeIcon] target Input (mono, h-8, comma/newline   │
│  placeholder) | hidden sm: Badges "Rate: {rps} RPS" · "Threads: {c}"         │
│  | outline "Add More Templates" + primary "Scan All (n)"                     │
├──────────────────────────────────────────────────────────────────────────────┤
│ sub-toolbar px-3 py-2 bg-muted/5: "Group By:" segmented control              │
│  [Directory|Severity|Category|Saved Sets (n)]  |  Save Set for Reuse /       │
│  inline name Input (Enter=save, Escape=cancel) + Save + ✕ ; Clear Staging    │
├──────────────────────────────────────────────────────┬───────────────────────┤
│ ScrollArea p-3:                                       │ aside w-[420px]       │
│  empty state (CirclesFourIcon, "No templates selected│ (when inspecting):    │
│  yet" → "Go to Step 1: Template Hub")                 │ severity badge + id   │
│  OR saved-sets grid (md:grid-cols-2): group cards     │ [YAML|Flow DAG]       │
│   {name, n templates Badge, description, Saved date,  │ switcher + "Scan Item │
│    Load into Staging / Scan Set (emerald) / delete ✕} │ Now" primary button   │
│  OR grouped cards: header (FoldersIcon group name,    │ YAML <pre> mono       │
│   count Badge, "Scan Group (n)") + divide-y rows      │ zinc-300 bg-black/90  │
│   row: severity badge, id (+directory chip), name,    │ or NucleiFlowCanvas   │
│   "Scan Item" (emerald hover) + ✕ remove              │                       │
└──────────────────────────────────────────────────────┴───────────────────────┘
```

- `groupedData` builds a `Map<string, TemplateItem[]>`: directory → `(t.directory||'http').toUpperCase()`; severity → fixed order `critical, high, medium, low, info` (only non-empty); category → `t.category.toUpperCase()`. `'saved'` renders the saved-groups grid instead.
- `handleScanAll/handleScanGroup(ids)/handleScanSingleItem(id)` call `startScan(ids)` then `onContinueToResults()`. Note `useNucleiScan.startScan` itself also does `setActiveTab('results')`.
- Saved set cards: `loadSavedGroup` replaces `selectedTemplateIds`; `handleScanGroup(group.templateIds)` scans without loading; `deleteSavedGroup` removes from localStorage.
- Save-group input: `autoFocus`, `onKeyDown` Enter → `handleSaveGroup()`, Escape → cancel. `saveCurrentSelectionAsGroup` ignores empty selections.

### 6.3 NucleiScanCockpit (Step 3) — `components/nuclei-scan-cockpit.tsx`

No props; reads store + `useNucleiScan`.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ header h-11 px-3 bg-muted/15: [GlobeIcon target Input (disabled while       │
│  running)] [n Templates (Change) button → selected tab] [Rate/Threads pills  │
│  hidden lg:] | status Badge (running→info + animate-pulse, paused→warning,   │
│  completed→success, cancelled→muted, error→destructive) | controls:          │
│   running: [Pause (amber PauseIcon)][Stop destructive]                       │
│   paused:  [Resume (sky-600 bg)][Stop destructive]                           │
│   else:    [Run Scan] (disabled if 0 templates selected)                     │
├──────────────────────────────────────────────────────────────────────────────┤
│ telemetry strip px-3 py-1.5 bg-muted/5 mono text-xs:                         │
│  {pct}% ({done}/{total} reqs) [1.5 h-1.5 progress bar bg-primary width%]     │
│  RPS / Elapsed | Badge "{n} Vulnerabilities" (destructive border when n>0)   │
├──────────────────────────────────────────────────────────────────────────────┤
│ NucleiRunFindingsTable (flex-1) ─ click row → setSelectedFindingId           │
├──────────────────────────────────────────────────────────────────────────────┤
│ bottom panel: h-64 open / h-8 collapsed (transition-all, border-t)           │
│  h-8 bar: [TerminalWindowIcon Console Logs][TreeStructureIcon Attack Flow    │
│  DAG] toggle buttons; right "Minimize ▾"/"Expand ▴"                          │
│  body: NucleiConsoleStream(logs, clear, autoScroll) | NucleiFlowCanvas with  │
│  yaml of templates.find(id===selectedTemplateIds[0])?.yaml_content || first  │
└──────────────────────────────────────────────────────────────────────────────┘
  + {selectedFinding && <NucleiRunFindingDetailDrawer …/>} (right side panel)
```

Local state: `activeBottomView: 'console' | 'flow'` (default `'console'`), `isBottomOpen` (default `true`). Handlers: `handleCopyCurl` copies `finding.curl_command` via clipboard; `handleSendToRepeater` and the drawer's `onSendToComparer` are **intentional no-op stubs** in the cockpit (`// Optional integration handler`), while the legacy page hook contains the real Repeater integration (§6.8).

### 6.4 NucleiRunFindingsTable — `components/nuclei-run-findings-table.tsx`

Props: `{ findings, selectedFindingId, onSelectFinding, searchQuery, onSearchChange, protocolFilter, onToggleProtocolFilter, onSendToRepeater, onCopyCurl, isRunning }`. Exported alias `NucleiFindingsTable`.

- Toolbar: search input (`pl-8`, placeholder "Filter findings by CVE, template, keyword, or URL…") + protocol chip buttons for the fixed list `['http','dns','ssl','websocket','tcp','headless']` using `PROTOCOL_BADGES` colors; inactive-but-filtered chips get `opacity-35`.
- Empty state: `ShieldCheckIcon` in a muted circle; heading switches between "Scanning in progress..." and "No Vulnerability Findings" based on `isRunning`.
- Table (`Table/TableHeader/TableBody/...`, `text-xs`): sticky header `sticky top-0 bg-muted/40 backdrop-blur z-10`; columns Severity (w-24) / Vulnerability-Template (min-w-[220px]) / Protocol (w-20) / Matched Location (min-w-[240px], `select-all` mono) / Extracted Evidence (min-w-[160px], secondary Badges capped `max-w-[180px]`, "—" when none) / Actions (w-24, right).
- Selected row: `bg-primary/10 hover:bg-primary/15 border-l-2 border-l-primary`.
- Row actions (wrapped in a `stopPropagation` div): Tooltip'd ghost button `PaperPlaneTiltIcon` "Send to Repeater" (hover `text-emerald-500`) and `quiet` button `CopyIcon` "Copy cURL Command".
- `cve_id` renders as an outline Badge `text-warning border-warning/30` (`h-3.5 px-1 text-4xs`).

### 6.5 NucleiRunFindingDetailDrawer — `components/nuclei-run-finding-detail-drawer.tsx`

Props: `{ finding: NucleiFinding | null, onClose, onSendToRepeater, onSendToComparer, onCopyCurl }` (alias `NucleiFindingDetailDrawer`). Rendered as an `<aside>` (not a Radix sheet) `w-96 lg:w-[460px] border-l bg-background/95`:

- Header: severity pill (dot + uppercase), protocol chip, optional CVE badge (`text-warning`), template name (`text-sm font-semibold`), template id mono, quiet X button.
- Action bar: three outline buttons — "Repeater" (`PaperPlaneTiltIcon`, hover emerald), "Comparer" (`GitDiffIcon`, hover sky), "cURL" (`CopyIcon`).
- Scrollable body sections (each `text-xs`, label `text-2xs uppercase tracking-wider text-muted-foreground`): Matched Endpoint (mono box `bg-muted/40 border border-border/50`), Extracted Evidence (amber boxes `bg-amber-500/10 border-amber-500/20 text-amber-500`), Description, Remediation (emerald panel `bg-emerald-500/5 border-emerald-500/20` with `CheckCircleIcon`), Tags (`#{tag}` secondary Badges), cURL Probe (`<pre>` mono with a Copy link), metadata footer (`mt-auto`): `ClockIcon` Discovered → `new Date(finding.matched_at).toLocaleString()`; `UserIcon` Author when present.

### 6.6 NucleiRunConsoleStream — `components/nuclei-run-console-stream.tsx`

Props: `{ logs, onClearLogs, autoScroll, onAutoScrollChange }` (alias `NucleiConsoleStream`). Terminal look: root `font-mono text-xs bg-black/95 text-zinc-300`.

- Toolbar (`border-zinc-800 bg-zinc-950/80`): `TerminalWindowIcon` (emerald) + "Telemetry Stream" + Badge `{filteredLogs.length} events` (`border-zinc-700 text-zinc-400`); filter Input `w-44 h-6 bg-zinc-900 border-zinc-700` (matches `message`, `target`, `template_id`); `Switch` `id="autoscroll-toggle"` + label; ghost Trash button (`hover:text-red-400 hover:bg-zinc-800`).
- Body: `select-text` scroll container; auto-scroll effect sets `scrollTop = scrollHeight` when `autoScroll && logs` change. Empty state: "Awaiting engine telemetry and probe events...".
- Line rendering by `log.level`: `vuln` → `text-red-400 bg-red-950/20 border-red-900/40` with level chip `bg-red-500 text-black`; `success` → `text-emerald-400` chip `bg-emerald-500 text-black`; `error` → `text-rose-400` chip `bg-rose-600 text-white`; `warn` → `text-amber-400` chip `bg-amber-500 text-black`; `info` → chip `bg-zinc-800 text-zinc-300`. Timestamp `[HH:MM:SS]` in `text-zinc-500`.

### 6.7 Flow subsystem — `flow/`

**NucleiFlowCanvas** (`components/nuclei-flow-canvas.tsx`): props `{ yamlContent: string; hideToolbar?: boolean }`. Wraps inner content in `ReactFlowProvider`. Uses the shared `FlowCanvas` (`src/components/flow`) with: `nodesConnectable={false}`, `nodesDraggable={true}`, `elementsSelectable={true}`, `nodeTypes={NUCLEI_NODE_TYPES}`, `edgeTypes={NUCLEI_EDGE_TYPES}`, `onNodeClick` → select, `onPaneClick` → deselect, `fitView`, `fitViewOptions={{padding: 0.25}}`, `snapToGrid`, `snapGrid={[16,16]}`, `showMiniMap`, `showControls`, `showBackground`, `className="nuclei-flow-canvas"`. The shared canvas adds space-pan (`useFlowKeyboard`), `panOnScroll`, and delete key codes `['Backspace','Delete']`.

**NucleiFlowToolbar**: template name (truncated `max-w-xs`), badge state machine — errors → `variant="destructive"` `{n} Error(s)`; else warnings → outline `text-warning border-warning/40` `{n} Warning(s)`; else outline `text-success border-success/40` `DAG Flow Ready` with `CheckCircleIcon`. Buttons: "Auto Layout" (`LightningIcon` amber, `title="Hierarchical DAG Auto-Layard"`— actually `title="Hierarchical DAG Auto-Layout"`) and "Fit" (`ArrowsInIcon`, `title="Fit View to Canvas"`).

**Node palette** (`PALETTE_ITEMS`): templateInfo "Template Metadata" (core), requestNode "Protocol Request Probe" (probe), extractorNode "Dynamic Variable Extractor" (analysis), matcherNode "Response Matcher Assertion" (analysis), flowNode "Nuclei v3 Flow Logic" (orchestration).

**Node visual language** (all `rounded-lg border-2 bg-card/95 shadow-md backdrop-blur-xs`; selected adds `ring-2 ring-{color}/20 shadow-{color}/10`; header `bg-muted/30`):

| Node | Width | Accent | Header icon | Handles |
|---|---|---|---|---|
| TemplateInfoNode | `w-72` | primary | `ShieldWarningIcon` | bottom source `!bg-primary` |
| RequestNode | `w-72` | sky-500 | `GlobeIcon` | top target (sky), **left** target `variable-in` (purple), bottom source (sky); body shows first path (mono chip), `{n} Headers • Has Body`, optional amber `StopCircleIcon` "Stop-First" |
| ExtractorNode | `w-68` | purple-500 | `TreeStructureIcon` | top target, **right** source `variable-out` (purple), bottom source; body = first `regex/json/dsl/kval` pattern in purple chip; `EyeClosedIcon` "Internal" when `internal` |
| MatcherNode | `w-68` | emerald-500 | `CheckCircleIcon` | top target; **two** bottom sources: `condition-true` at `left: 32%` (emerald) and `condition-false` at `left: 68%` (rose); body shows value preview (`HTTP 200, 301` for status else first word/regex/dsl) + `{CONDITION} • Part: {part}`; `ProhibitIcon` "Inverted" when `negative` |
| FlowNode | `w-72` | amber-500 | `LightningIcon` | top target + bottom source (amber); body = `flowCode` in amber chip `max-h-24 overflow-y-auto`; footer "JS Flow Expression" |

`METHOD_COLORS` (request node method chip): GET sky, POST emerald, PUT amber, DELETE red, PATCH purple (fallback `bg-muted`).

**Edges** (`NucleiFlowEdgeView`): `getBezierPath` + `BaseEdge`; style from `EDGE_STYLES` — `default` `var(--muted-foreground, #71717a)` w2; `condition-true` `#10b981` w2.5; `condition-false` `#f43f5e` w2.5; `variable-pipe` `#8b5cf6` w2 dashed `5,5`. Selected edges get `strokeWidth + 1` and `filter: drop-shadow(0 0 3px currentColor)`. Labels render via `EdgeLabelRenderer` chips tinted per type. `NUCLEI_EDGE_TYPES` maps all four type names to the same component.

**YAML → graph** (`nucleiYamlToGraph`): line-based parser. Info block extraction handles `description: |`/`>` block scalars and stops `reference:` lists on any non `- ` line; protocol keys must be at indent 0 (`http/dns/tcp/ssl`, plus `flow:` enters flow-code capture before protocol checks so `flow:` followed by `http:` doesn't swallow requests). Builds node ids `template-root`, `req-{n}` (stepId `http-{n}`), `{req}-ext-{n}`, `{req}-mat-{n}`, `flow-v3-engine`; matcher node name defaults to first word or `match-{type}`; extractor default name `var_{n}`; all positions computed by `calculateDagLayout`. Edges are `default` type, except the trailing edge into the flow node which is `condition-true`.

**Graph → YAML** (`graphToNucleiYaml`): emits `id`, `info` (name/author/severity/description/reference/tags), optional `flow: |` block, then one `{protocol}:` section per request node with `method`, `path:` list, `headers`, `body: |`, `stop-at-first-match: true`, `matchers-condition: and` + matchers (status/words/part/condition/negative; part omitted for status type), and `extractors` (type/name/part/internal/regex). Request-owned matchers/extractors are resolved from edges (`e.source === req.id`).

**Layout** (`calculateDagLayout`): options `nodeWidth: 290, nodeHeight: 170, rankSep: 90, nodeSep: 50`. BFS rank assignment seeded from `templateInfo` (rank 0) then all in-degree-0 nodes; relaxations bounded by `newRank < nodes.length` to terminate on (model-possible) cycles; unvisited nodes get rank 1; each rank row is centered horizontally: `x = startX + idx*(nodeWidth+nodeSep)`, `y = r*(nodeHeight+rankSep) + 40`.

**Validation** (`validateNucleiGraph`) diagnostics: missing templateInfo (error `missing-template-info`), >1 templateInfo (warning, points at `[1]`), zero request nodes (error), `{{Var}}` references in request paths/body/header values not produced by any extractor and not in the builtin set `BaseURL, RootURL, Hostname, Host, Port, Path, File, Scheme, randstr, rand_int, rand_text_alphanumeric` (warning per variable), disconnected nodes (info), cycles via DFS with `inStack` (single error `cycle-detected`).

**Inspector** (`NucleiFlowInspector`, `w-80 lg:w-96 border-l shadow-md`): header picks icon per type (templateInfo `ShieldWarningIcon` primary, request `GlobeIcon` sky, extractor `TreeStructureIcon` purple, matcher `CheckCircleIcon` emerald, flow `LightningIcon` amber) + node id; branches render read-only fields (template name/author/severity badge/description/tags; request stepId + method badge + path chips + body pre; matcher type/part + status badges or pattern rows; extractor name/type/part/patterns; flow `flowCode` pre). The `InspectableNucleiNode` mapped type re-correlates `type` and `data` so each branch reads narrowed data without casts.

### 6.8 Legacy / superseded components (still exported, not mounted)

Verified by cross-referencing imports: `NucleiRunToolbar`/`NucleiToolbar`, `NucleiRunTemplateHub`/`NucleiTemplateHub` (only self-file match), `NucleiRunTemplateStudio`/`NucleiTemplateStudio`, `NucleiTemplateSelector`, `NucleiRunStatsBanner`/`NucleiStatsBanner`, `NucleiRunConfigDialog`/`NucleiConfigDialog`, `NucleiRunExportDialog`/`NucleiExportDialog`, and `useNucleiRunPage`/`useNucleiPage` are **not imported anywhere outside their own files** — they belong to a previous single-screen layout (each carries a `// ponytail:` marker comment noting a "decluttered" redesign). They remain functional:

- **NucleiRunToolbar**: view-switch buttons Templates (with count Badge)/Flow, target Input `h-6 w-48`, run toggle that flips to `PauseIcon` "Scanning..." while running.
- **NucleiRunTemplateHub**: `TEMPLATE_CATEGORIES` chips, `All/None/Critical & High` batch buttons (`selectTemplatesBySeverity(['critical','high'])`), card grid (`md:grid-cols-2 xl:grid-cols-3`), per-card Flow button → `onOpenInStudio(id)` + `onNavigateTab('flow')`.
- **NucleiTemplateSelector**: sidebar with `RESEARCHER_STRATEGIES` (per-strategy icon: ScanIcon sky, ShieldCheckIcon rose, FlameIcon amber, BugIcon purple, CloudArrowDownIcon teal) and `TECH_STACKS` (CpuIcon), "Select Suite" (`selectCategoryTemplates`), "Select Filtered"/"Deselect" via direct `useNucleiStore.setState`, severity pills, strategy rationale banner showing `recommendedNoise` ("Noise Profile"), virtualized rows with dynamic `measureElement` (`estimateSize: 104`, overscan 5, O(1) `selectedSet`), footer bar with "Continue to Scan (n)".
- **NucleiRunStatsBanner**: status indicator (running = animated ping dot `animate-ping bg-emerald-400`, Paused `PulseIcon` amber, Completed `CheckCircleIcon`, Stopped `WarningCircleIcon`), `Progress` component + `%`, RPS (`LightningIcon` amber), elapsed mm:ss (`ClockIcon` sky), and 5 severity-count buttons that dim to `opacity-35 grayscale` when filtered out.
- **NucleiRunConfigDialog**: `Dialog` with numeric Inputs (Concurrency `-c` min 1 max 200; Rate Limit `-rl` min 1 max 1000; Timeout min 1 max 120; Retries min 0 max 5), proxy Input (placeholder `http://127.0.0.1:8080`), exclusions Textarea (newline-separated), two `Switch` rows (Headless Browser Mode; Follow HTTP Redirects), footer Reset (calls `onResetDefaults`)/Cancel/Save (emerald Save button). `handleSave` parses ints with fallbacks (25/150/10/1) and splits exclusions on newlines.
- **NucleiRunExportDialog**: format buttons SARIF v2.1.0 (`FileCodeIcon` sky) / JSONL (`FileCodeIcon` emerald) / CSV (`TableIcon` amber) / Markdown (`FileTextIcon` purple); `TextEditor` preview with `language` mapped `sarif|jsonl → json`, `markdown`, `csv → plaintext`; footer Badge `{n} findings included`; Copy (`CheckIcon` flash 1.8 s) and Download via `Blob` + anchor click — file `nuclei-report-${Date.now()}.${sarif.json|jsonl|csv|md}` with MIME `application/json` / `text/csv` / `text/markdown`.
- **useNucleiRunPage**: composes store + scan + studio; contains the only real cross-module finding actions — `copyFindingCurl` (clipboard + info toast "cURL Command Copied") and `sendToRepeater` → `useRepeaterStore.getState().createWorkspace(`Vuln: ${finding.template_id}`)` + `setActiveWorkspaceId` + success toast + `navigate('/repeater')`; also `sendToComparer` → `navigate('/comparer')`.
- **NucleiRunTemplateStudio**: Monaco `TextEditor` (yaml) with validation status pill (`{n} Error(s)` red / "Valid Schema" emerald), `DSL_HELPERS_REFERENCE.slice(0, 7)` insert chips (`{{BaseURL}}`…), diagnostics footer (`max-h-28`, red/yellow tinted), right "Target Test Harness" (`lg:w-[440px]`) with target input, Test button (emerald, `ArrowsClockwiseIcon animate-spin` while testing), MATCH TRIGGERED / NO MATCH banner, extracted values, and request (`language="http"`, h-24) / response (`json`, h-32) Monaco panes. Backed by `useTemplateStudio`, whose `runTestAgainstTarget` is a **simulated** test: blocks invalid templates, then a `setTimeout(600)` returns a fixed mock (`matched: true`, `status_code: 200`, `extracted: ['matched_version="2.4.1"', 'server_header="Apache/2.4"']`, fabricated request/response samples) — no IPC.

### 6.9 Built-in template catalogue (`DEFAULT_TEMPLATES`, 26 entries)

Every entry ships full `yaml_content`; the store pre-selects all 26 ids. This catalogue doubles as the regression fixture for the validator and the YAML→graph parser (`default-templates.test.ts`, `ast-translator.test.ts` "built-in templates" block).

| # | id | severity | category | notes |
|---|---|---|---|---|
| 1 | `cve-2023-46805-ivanti-auth-bypass` | critical | cves | path traversal to system-info; regex extractor `ivanti_version` |
| 2 | `cve-2024-21887-ivanti-command-injection` | critical | cves | `$(whoami)` in license endpoint; word matchers root + bin/bash |
| 3 | `env-file-disclosure` | high | exposures | `.env`/`.env.local`/`.env.production`; regex extractor `db_host` |
| 4 | `git-config-disclosure` | medium | exposures | `.git/config`, `.git/HEAD` |
| 5 | `spring-boot-actuator-heapdump` | high | exposures | heapdump endpoints; binary matcher `4a4156412050524f46494c45` ("JAVA PROFILE") |
| 6 | `cors-wildcard-origin-misconfig` | low | misconfigurations | Origin `https://evil-attacker.com` reflection |
| 7 | `graphql-introspection-enabled` | info | exposures | POST to /graphql variants |
| 8 | `tomcat-manager-default-login` | high | default-logins | Basic auth probe |
| 9 | `swagger-ui-api-disclosure` | info | exposures | Swagger/OpenAPI paths |
| 10 | `phpinfo-file-disclosure` | low | exposures | phpinfo() markers |
| 11 | `tech-detect-wordpress` | info | exposures | regex extractor `wp_version` |
| 12 | `tech-detect-apache` | info | exposures | regex extractor `apache_version` |
| 13 | `exposed-admin-panels` | low | exposures | common admin paths |
| 14 | `git-config-exposure` | high | exposures | variant of #4 |
| 15 | `env-file-secrets-disclosure` | critical | exposures | variant of #3 |
| 16 | `apache-server-status` | medium | misconfigurations | mod_status leak |
| 17 | `spring-boot-actuator-env` | high | exposures | actuator /env |
| 18 | `jenkins-unauth-access` | medium | exposures | unauthenticated dashboard |
| 19 | `laravel-debug-mode` | critical | cves | Ignition RCE check |
| 20 | `cve-2021-44228-log4j-rce` | critical | cves | Log4Shell JNDI payload |
| 21 | `cve-2025-0282-ivanti-connect-secure-rce` | critical | cves | stack buffer overflow (2025) |
| 22 | `dast-xss-reflection` | high | vulnerabilities | reflected XSS parameter probe |
| 23 | `dast-sql-error-injection` | critical | vulnerabilities | SQL error-based probe |
| 24 | `dast-lfi-traversal` | high | vulnerabilities | LFI traversal probe |
| 25 | `s3-bucket-public-listing` | high | misconfigurations | `<ListBucketResult` word matcher |
| 26 | `aws-secret-access-key-leak` | critical | exposures | regex `(?:A3T[A-Z0-9]|AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[A-Z0-9]{16}` over `/`, `/config.js`, `/app.js` |

The test suite additionally pins two catalogue invariants that once regressed: unique TS **and** YAML ids (a past duplicate `env-file-disclosure` collision), and "the validator reports the template name, never an extractor name" (extractors' own indented `name:` used to override `info.name`).

## 7. UI Libraries & Primitives Used

### 7.1 shadcn/ui-style components from `@celestia-project/ui` (v0.4.0)

| Component | Used in |
|---|---|
| `Button` | every component; observed props: `variant` (`default`, `outline`, `ghost`, `quiet`, `secondary`, `destructive`), `size` (`xs`, `sm`, `md`), custom `leading="tight"`, custom `mono` prop |
| `Badge` | index header, hub, selected, cockpit, table, drawer, console, flow toolbar/inspector; props: `variant` (`outline`, `secondary`, `destructive`), `size="sm"`, custom `mono` |
| `Input` | hub search, selected target/save-group, cockpit target, table search, console filter, config dialog, studio target, legacy toolbar/selector; custom props `textSize="xs"`, `mono` |
| `Checkbox` | hub list rows, template selector rows, legacy template hub cards (`checked` + `onCheckedChange`) |
| `ScrollArea` | hub taxonomy (`<ScrollArea fill>`), selected main area, selector sidebar (`className` variant) |
| `Switch` | console auto-scroll, config dialog headless/redirects |
| `Progress` | legacy stats banner (`value={progress.percentage}`) |
| `Table, TableHeader, TableBody, TableHead, TableRow, TableCell` | findings table |
| `Tooltip, TooltipTrigger, TooltipContent` | findings table row actions |
| `Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter` | config + export dialogs |
| `Textarea` | config dialog exclusions (custom `leading="tight"`, `mono`) |
| `Label` | config dialog field labels (custom `leading="tight"`) |
| `Select, SelectTrigger, SelectValue, SelectContent, SelectItem` | studio example-template picker |
| `TextEditor` (Monaco wrapper) | studio editor (`language="yaml"`), studio request (`"http"`) / response (`"json"`), export preview (`json` / `markdown` / `plaintext`); props `value`, `onChange`, `language`, `height`, `theme` (from `useTheme()`) |

Shared Radix-backed pieces consumed indirectly: `Tabs` (inside `TabbedPageLayout`), `ContextMenu` family (inside shared `PageTabBar`). React Flow primitives used directly: `ReactFlowProvider`, `useReactFlow`, `useNodesState`, `useEdgesState`, `Handle`, `Position`, `NodeProps`, `BaseEdge`, `EdgeLabelRenderer`, `getBezierPath`, plus types `Node`, `Edge`, `EdgeProps`, `ReactFlowProps`.

### 7.2 lucide-react

**Not used.** The entire module imports icons from `@phosphor-icons/react` v2.1.10. Exact icon names imported anywhere under `src/pages/nuclei-run/` (51):

`ArrowLeftIcon`, `ArrowRightIcon`, `ArrowsClockwiseIcon`, `ArrowsInIcon`, `BookmarkSimpleIcon`, `BugIcon`, `CheckCircleIcon`, `CheckIcon`, `CheckSquareIcon`, `CirclesFourIcon`, `ClockIcon`, `CloudArrowDownIcon`, `CodeBlockIcon`, `CodeIcon`, `CopyIcon`, `CpuIcon`, `DownloadSimpleIcon`, `EyeClosedIcon`, `FileCodeIcon`, `FileTextIcon`, `FilesIcon`, `FlameIcon`, `FolderOpenIcon`, `FoldersIcon`, `GearIcon`, `GitDiffIcon`, `GlobeIcon`, `LightningIcon`, `MagnifyingGlassIcon`, `NetworkIcon`, `PaperPlaneTiltIcon`, `PauseIcon`, `PlayIcon`, `PlusIcon`, `ProhibitIcon`, `PulseIcon`, `ScanIcon`, `ShieldCheckIcon`, `ShieldSlashIcon`, `ShieldWarningIcon`, `SlidersHorizontalIcon`, `StopCircleIcon`, `StopIcon`, `TableIcon`, `TagIcon`, `TerminalWindowIcon`, `TrashIcon`, `TreeStructureIcon`, `UserIcon`, `WarningCircleIcon`, `XIcon`.

(Several are imported-but-unused within files, e.g. `PlusIcon`, `SlidersHorizontalIcon`, `ShieldCheckIcon`/`ScanIcon`/`FlameIcon` in `nuclei-template-hub-step.tsx`.)

### 7.3 Other third-party libraries

| Library | Version | Usage |
|---|---|---|
| `@xyflow/react` | ^12.11.0 | flow DAG canvas, custom nodes/edges, layout state |
| `@tanstack/react-virtual` | ^3.14.2 | `useVirtualizer` hub list (64 px fixed) and selector list (104 px + `measureElement`) |
| `@tauri-apps/api` | — | `invoke`, `listen`, `UnlistenFn` via `lib/nuclei-ipc.ts` |
| `monaco-editor` | 0.55.1 | under `TextEditor` (kit package); theme passed from `useTheme` |
| `zustand` | — | `useNucleiStore`, `useNotificationStore`, `useRepeaterStore` |
| `react-router-dom` | — | `useNavigate` in legacy page hook; routes registered in `App.tsx` / `page-resolver.tsx` |
| `vitest` | — | 6 test files (`lib/*.test.ts`, `flow/lib/*.test.ts`) |
| no framer-motion, no charts lib, no CodeMirror | — | animations are Tailwind only (`animate-pulse`, `animate-ping`, `animate-bounce`, `animate-spin`) |

## 8. Keyboard Shortcuts & Interactions

| Input | Where | Behavior |
|---|---|---|
| `Space` (hold) | Flow canvas (shared `useFlowKeyboard`, `enableSpacePan`) | pan mode: `panOnDrag={spacePressed}`, `selectionOnDrag={!spacePressed}`, cursor `grab/grabbing`; ignored while focus is in `input, textarea, select, [contenteditable], .monaco-editor` |
| `Backspace` / `Delete` | Flow canvas | passed as `deleteKeyCode={['Backspace','Delete']}` to React Flow (edges/nodes deletable at model level; canvas is read-only for creation since `nodesConnectable={false}`) |
| `Enter` | Step 2 save-group inline Input | `handleSaveGroup()` |
| `Escape` | Step 2 save-group inline Input | cancel (`setIsSavingGroup(false)`) |
| Row click / Checkbox | hub list, selected groups, findings table | inspect / `toggleTemplateSelection` / `setSelectedFindingId`; nested action buttons all `e.stopPropagation()` |
| Pane click | Flow canvas | clears `selectedNodeId` |
| ✕ on filter chips | hub chips strip | removes that single filter (directory/tag/author/protocol/severity) |
| Console auto-scroll | Switch `id="autoscroll-toggle"` | toggles store `autoScrollConsole` |

No global `keyDown` handlers are registered by this module itself; all other interactions are mouse/keyboard-native form controls.

## 9. Styling Details

- **Severity mapping** (`SEVERITY_CONFIG` in `constants.ts`) — each severity carries `bg`, `text`, `border`, `badgeVariant`, `iconColor`, `dotColor`:

| Severity | bg | text | border | badgeVariant | dot |
|---|---|---|---|---|---|
| critical | `bg-red-500/10 dark:bg-red-500/20` | `text-red-600 dark:text-red-400` | `border-red-500/30` | destructive | `bg-red-500` |
| high | `bg-orange-500/10 dark:bg-orange-500/20` | `text-orange-600 dark:text-orange-400` | `border-orange-500/30` | destructive | `bg-orange-500` |
| medium | `bg-yellow-500/10 dark:bg-yellow-500/20` | `text-yellow-600 dark:text-yellow-400` | `border-yellow-500/30` | secondary | `bg-yellow-500` |
| low | `bg-blue-500/10 dark:bg-blue-500/20` | `text-blue-600 dark:text-blue-400` | `border-blue-500/30` | outline | `bg-blue-500` |
| info | `bg-slate-500/10 dark:bg-slate-500/20` | `text-slate-600 dark:text-slate-400` | `border-slate-500/30` | outline | `bg-slate-400` |

- **Protocol badges** (`PROTOCOL_BADGES`): http `bg-sky-500/15 text-sky-400`, dns `bg-emerald-500/15 text-emerald-400`, ssl "SSL/TLS" `bg-amber-500/15 text-amber-400`, tcp `bg-indigo-500/15 text-indigo-400`, websocket "WS" `bg-purple-500/15 text-purple-400`, headless "HEADLESS" `bg-rose-500/15 text-rose-400`, javascript "JS" `bg-yellow-500/15 text-yellow-400`, code "CODE" `bg-teal-500/15 text-teal-400`, file "FILE" `bg-stone-500/15 text-stone-400`, whois "WHOIS" `bg-fuchsia-500/15 text-fuchsia-400`.
- **Scan presets** (`PRESET_OPTIONS`): quick-triage c50/150 RPS/5 s (critical+high), full-audit c25/100/10 s (all severities), critical-cves c40/120/8 s (critical only), passive-recon c10/15/15 s (medium+low+info), custom c25/100/10.
- **Console palette**: terminal surfaces `bg-black/95` (console) and `bg-black/90` (raw YAML `<pre>` inspectors) with `text-zinc-300`; toolbar `bg-zinc-950/80 border-zinc-800`; log level chips as listed in §6.6.
- **Semantic tokens**: heavy use of `bg-muted/{10,15,20,30,40}` layering, `border-border`, `text-muted-foreground`, `text-foreground`, `bg-primary/10|15`, `border-primary/30`, `text-primary`, success/warning/info/destructive tokens (`border-success/30 text-success`, `text-warning border-warning/30`, `border-info/40 text-info`, `border-destructive/40 text-destructive`).
- **Micro-typography scale**: `text-4xs` (badges), `text-3xs` (mono metadata), `text-2xs` (secondary lines), `text-xs` (body) — a custom 4XS/3XS/2XS extension beyond Tailwind defaults; `font-mono` applied to ids, URLs, counts, telemetry.
- **Spacing/radius conventions**: headers `h-10`/`h-11`, control buttons `h-7` (xs) / `h-8` (inputs), small action buttons `h-6`; cards `rounded-lg border`, chips `rounded`, rows `rounded-md`; active taxonomy items `rounded-md border border-primary/30 bg-primary/15`.
- **Flow canvas styling**: nodes `w-72`/`w-68` cards; handles forced `!h-2.5 !w-2.5` with `!-top-1.5`/`!-bottom-1.5`/`!-left-1.5`/`!-right-1.5` and `!border-2 !border-background`; snap grid 16 px; shared background/controls/minimap components.
- **Custom kit props** seen throughout: `mono` on Badge/Input/Button/TableCell, `leading="tight"` on Button/Input/Label/Textarea/SelectTrigger, `textSize="xs"` on Input — these are `@celestia-project/ui` extensions, not stock shadcn props.
- Comments throughout the module use `// ponytail:` tags marking redesigned/decluttered components (a house convention of the "ponytail" refactor pass).

## 10. Integration Points

| Direction | Module | Mechanism |
|---|---|---|
| Router | `src/App.tsx`, `src/routes/page-resolver.tsx` | lazy `import("@/pages/nuclei-run")` → `m.NucleiRunPage`; routes `/nuclei-run`, `/nuclei`, `/scanner`; resolver case `"nuclei-run"` → `<StandaloneLayout id="/nuclei-run" title="Scanner">` |
| Layout | `src/layout/tabs-layout/` | `TabbedPageLayout` + `PageTabItem {id, name, closable:false}`; the shared tab bar supports rename/close/context-menu but the Nuclei page passes none of those handlers |
| Shared flow kit | `src/components/flow` | `FlowCanvas` (background, controls, minimap, command-help badge, space-pan hook) reused by `NucleiFlowCanvas` |
| Store | `src/stores/nuclei.ts` | single source of truth; store itself imports from the page folder (types, `DEFAULT_TEMPLATES`, `PRESET_OPTIONS`, IPC) — a bidirectional page↔store coupling |
| Notifications | `src/stores/notifications.ts` | `addAlert({id, title, message, type: 'info'|'warning'|'error'|'success', source: 'Nuclei Scanner'})` for scan start/finish, findings, no-target/no-template errors, cURL copy |
| Repeater | `src/stores/repeater.ts` | legacy hook `useNucleiRunPage.sendToRepeater`: `createWorkspace('Vuln: <template_id>')` + `setActiveWorkspaceId` + `navigate('/repeater')` |
| Comparer | router | legacy `sendToComparer` → `navigate('/comparer')` (no payload handoff) |
| Theme | `src/components/theme-provider.tsx` | `useTheme().theme` feeds `TextEditor theme` prop (studio + export) |
| IPC helper | `src/lib/ipc.ts` | `toErrorMessage` for sync failures |
| Backend | Tauri Rust engine | all commands/events in §5; templates cached on disk by the backend (`cache_path` returned and displayed via `syncStatus`) |

## 11. Source Files Documented

Page module (all 47 files read in full):

- `/Users/870041/Desktop/project/hexbuffer-rust/references/hexbuffer/src/pages/nuclei-run/index.tsx`
- `.../nuclei-run/types.ts`
- `.../nuclei-run/constants.ts`
- `.../nuclei-run/constants/categories.ts`
- `.../nuclei-run/hooks/use-nuclei-scan.ts`
- `.../nuclei-run/hooks/use-nuclei-run-page.ts`
- `.../nuclei-run/hooks/use-template-studio.ts`
- `.../nuclei-run/lib/nuclei-ipc.ts`
- `.../nuclei-run/lib/template-validator.ts`
- `.../nuclei-run/lib/template-validator.test.ts`
- `.../nuclei-run/lib/formatters.ts`
- `.../nuclei-run/lib/formatters.test.ts`
- `.../nuclei-run/lib/default-templates.ts`
- `.../nuclei-run/lib/default-templates.test.ts`
- `.../nuclei-run/components/nuclei-template-hub-step.tsx`
- `.../nuclei-run/components/nuclei-selected-templates-step.tsx`
- `.../nuclei-run/components/nuclei-scan-cockpit.tsx`
- `.../nuclei-run/components/nuclei-run-findings-table.tsx`
- `.../nuclei-run/components/nuclei-run-finding-detail-drawer.tsx`
- `.../nuclei-run/components/nuclei-run-console-stream.tsx`
- `.../nuclei-run/components/nuclei-run-toolbar.tsx`
- `.../nuclei-run/components/nuclei-run-template-hub.tsx`
- `.../nuclei-run/components/nuclei-run-template-studio.tsx`
- `.../nuclei-run/components/nuclei-template-selector.tsx`
- `.../nuclei-run/components/nuclei-run-stats-banner.tsx`
- `.../nuclei-run/components/nuclei-run-config-dialog.tsx`
- `.../nuclei-run/components/nuclei-run-export-dialog.tsx`
- `.../nuclei-run/flow/index.ts`
- `.../nuclei-run/flow/types.ts`
- `.../nuclei-run/flow/constants.ts`
- `.../nuclei-run/flow/hooks/use-nuclei-flow.ts`
- `.../nuclei-run/flow/components/nuclei-flow-canvas.tsx`
- `.../nuclei-run/flow/components/nuclei-flow-toolbar.tsx`
- `.../nuclei-run/flow/components/nuclei-flow-inspector.tsx`
- `.../nuclei-run/flow/nodes/index.ts`
- `.../nuclei-run/flow/nodes/template-info-node.tsx`
- `.../nuclei-run/flow/nodes/request-node.tsx`
- `.../nuclei-run/flow/nodes/extractor-node.tsx`
- `.../nuclei-run/flow/nodes/matcher-node.tsx`
- `.../nuclei-run/flow/nodes/flow-node.tsx`
- `.../nuclei-run/flow/edges/nuclei-flow-edge.tsx`
- `.../nuclei-run/flow/lib/ast-translator.ts`
- `.../nuclei-run/flow/lib/ast-translator.test.ts`
- `.../nuclei-run/flow/lib/dag-layout.ts`
- `.../nuclei-run/flow/lib/dag-layout.test.ts`
- `.../nuclei-run/flow/lib/graph-validator.ts`
- `.../nuclei-run/flow/lib/graph-validator.test.ts`

Shared modules opened for usage context:

- `/Users/870041/Desktop/project/hexbuffer-rust/references/hexbuffer/src/stores/nuclei.ts`
- `.../src/stores/notifications.ts` (alert shape)
- `.../src/stores/repeater.ts` (workspace API)
- `.../src/lib/ipc.ts` (`toErrorMessage`)
- `.../src/lib/utils.ts` (`cn`, used everywhere)
- `.../src/layout/tabs-layout/tabbed-page-layout.tsx`, `.../tab-bar.tsx`
- `.../src/components/flow/index.ts`, `.../canvas/flow-canvas.tsx`, `.../hooks/use-flow-keyboard.ts`
- `.../src/components/theme-provider.tsx`
- `.../src/App.tsx`, `.../src/routes/page-resolver.tsx` (route registration)
- `.../package.json` (dependency versions)
