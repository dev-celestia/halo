# Port Scanner (`port-scanner`) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/port-scanner/` (15 files, ~2,330 lines)
> Exported page component: `PortScannerPage`

## 1. Overview & Purpose

A TCP **connect-scan** front end over the backend scanner engine (`scan_ports` / `stop_port_scan`). The user enters a target host, picks a port preset (Quick / Web / Top 100 / Full / Custom), tunes timeout, concurrency, banner grabbing, and a **Stealth vs "Noisy - High Performance"** mode (delay, jitter, port randomization), then watches open ports stream in live via per-scan Tauri events. Results are a sorted table (host, port, state, service, latency meter, banner) with copy-to-clipboard; exports (JSON/CSV) are plumbed but unwired. The page ships a dedicated welcome/onboarding state with one-click quick scans, and a heavyweight **Custom Port List Editor** dialog with a tolerant mini-language parser (`parsePorts`).

## 2. Annotated File Tree

```
port-scanner/
├── index.tsx                              (94)  Page shell: ScannerToolbar + ScanResults; quickStart handler
├── types.ts                               (11)  PortScanState, PortScanResult (host, port, state, service, banner, response_time_ms, error)
├── constants.ts                           (16)  PORT_PRESETS (Quick/Web/Top100/Full strings), PRESET_OPTIONS, PortPreset type
├── hooks/
│   └── use-port-scanner-page.ts          (209)  Page controller: per-scan event listeners, scan lifecycle, notification alerts
├── lib/
│   ├── port-helpers.ts                   (102)  parsePorts mini-language, formatPortsSummary, sortScanResults, describePortPreset
│   └── port-helpers.test.ts              (113)  vitest suite for parsePorts edge cases (reversed ranges, clamping, keywords, comments)
└── components/
    ├── scanner-toolbar.tsx               (479)  Toolbar: target+Start/Stop, preset Select, custom-ports inline, progress, stealth switch, config popover
    ├── scanner-config-popover.tsx        (290)  Popover: timeout, concurrency, banner checkbox, collapsible stealth options
    ├── custom-ports-dialog.tsx           (240)  Multi-line port editor: quick-add chips, syntax guide, live summary badge
    ├── scan-results.tsx                  (115)  Orchestrator: welcome vs (header + error + table)
    ├── scan-results-header.tsx           (172)  "Scanned <target> [N open]" + Copy/JSON/CSV/Clear
    ├── scan-results-table.tsx            (238)  Kit Table with sticky header, latency bar, banner code chip
    ├── scan-welcome-state.tsx            (181)  Onboarding: 3 quick-scan cards (Quick/Web/Top100)
    └── hooks/
        ├── use-scanner-toolbar.ts         (36)  showAdvanced, dialog open state, progress percentage
        └── use-scan-results.ts            (31)  copied flag (2s), getLatencyColor (>800 red / >300 yellow / else emerald)
```

## 3. Architecture & Dataflow

```mermaid
sequenceDiagram
    participant U as User
    participant H as usePortScannerPage
    participant S as usePortScannerStore
    participant N as useNotificationStore
    participant B as Backend scan_ports

    U->>H: Start Scan (or welcome-card QuickStart)
    H->>N: warning alert if NOT stealthMode ("Noisy - High Performance Scan Started…")
    H->>S: reset results, progress {0, N}, isRunning, hasRun
    H->>H: scanId = crypto.randomUUID(); listen port-scan-result-{id} + port-scan-progress-{id}
    H->>B: invoke scan_ports { request: { scan_id, target, ports, timeout_ms, concurrency, banner_grab, scan_type:'connect', stealth_mode, delay_ms?, jitter_ms?, randomize_ports? } }
    B-->>H: event port-scan-result-{id} (open only → append + sort)
    B-->>H: event port-scan-progress-{id} (Update{current,total} | Complete | Cancelled)
    B-->>H: final PortScanResult[] (filter open, sort, replace)
    U->>H: Stop Scan → invoke stop_port_scan { scanId }
```

Key behaviors:
- **Only open ports reach the UI.** The result-event handler drops everything whose `state !== 'open'`; the final invocation result is filtered the same way. Results live-append **then** get replaced by the authoritative final array (both sorted by `host` then `port`).
- **Stealth parameters are only sent when `stealthMode` is true** (`delay_ms`/`jitter_ms` default 300/200, `randomize_ports` flag); `scan_type` is hard-coded `'connect'`.
- **Dual warning path for Noisy mode**: starting a non-stealth scan alerts (id `scan-noisy-alert-*`), and flipping the switch off alerts again (id `noisy-scan-mode-*`) — both via `useNotificationStore.addAlert` (`type: 'warning'`, `source: 'Port Scanner'`).
- `scanIdRef` guards Stop; events are unlistened in `finally`.

## 4. State Management — `src/stores/port-scanner.ts`

| Field | Type | Default | Persisted |
| :-- | :-- | :-- | :-- |
| `target` | `string` | `''` | ✅ |
| `preset` | `PortPreset` | `'Quick'` | ✅ (custom `merge` re-normalizes case-insensitively to the union) |
| `ports` | `string` | `PORT_PRESETS.Quick` | ✅ |
| `timeoutMs` / `concurrency` | `string` | `'800'` / `'100'` | ✅ |
| `bannerGrab` | `boolean` | `true` | ✅ |
| `stealthMode` | `boolean` | `true` | ✅ (merge **forces back to `true`** on rehydrate) |
| `delayMs` / `jitterMs` | `string` | `'300'` / `'200'` | ✅ |
| `randomizePorts` | `boolean` | `true` | ✅ |
| `results` / `hasRun` / `error` / `progress` / `isRunning` | — | empty/false/0 | ❌ (cleared in `merge` too) |

Persist key **`halo-port-scanner`**. Notable: `setStealthMode(false)` fires the noisy-mode notification **inside the store setter** (UI-independent), and `clearResults` resets `hasRun` too (returns to the welcome screen).

## 5. IPC Contracts

### Commands

| Command | Args | Returns | Notes |
| :-- | :-- | :-- | :-- |
| `scan_ports` | `{ request: { scan_id, target, ports: number[], timeout_ms, concurrency, banner_grab, scan_type: 'connect', stealth_mode, delay_ms?, jitter_ms?, randomize_ports? } }` | `PortScanResult[]` (final, authoritative) | Numbers coerced with `Number(x) \|\| default` (800 / 100 / 300 / 200) |
| `stop_port_scan` | `{ scanId }` (camelCase arg) | `void` | No-op if `scanIdRef` is null |

### Events (per scan id, both unlistened in `finally`)

| Event | Payload | Handler |
| :-- | :-- | :-- |
| `port-scan-result-{scanId}` | `PortScanResult` | Append if `state === 'open'`, then `sortScanResults` |
| `port-scan-progress-{scanId}` | `{ type: 'Update', current, total }` \| `{ type: 'Complete' }` \| `{ type: 'Cancelled' }` | Update progress; Complete/Cancelled → `setIsRunning(false)` |

## 6. UI Layout & Component Breakdown

```
┌─ PortScannerPage (p-2, border rounded-md) ──────────────────────────────────────────┐
│ ┌─ ScannerToolbar (border-b bg-muted/20, relative) ────────────────────────────────┐ │
│ │ [target host or IP (w-48, mono)] [▶ Start Scan / ■ Stop Scan] │ [Quick ▾ (105px)] │ │
│ │   (Custom → [80, 443, 1-4… (w-44/56) "18p"] [≡ tooltip])   Progress: 42/100 (42%)│ │
│ │                                        [👁 Stealth / ⚠ Noisy - High Performance ⚙]│ │
│ │ ▁▁▁▁▁▁▁▁▁▁ absolute bottom 2px bg-primary progress line ▁▁▁▁▁▁▁▁                │ │
│ └──────────────────────────────────────────────────────────────────────────────────┘ │
│ ├─ ScanResults ────────────────────────────────────────────────────────────────────┤ │
│ │ (not hasRun) → ScanWelcomeState: "Network Port Scanner" + [Quick][Web][Top100]   │ │
│ │ (hasRun) → Header: Scanned <target> [N open]  [Copy/Copied][JSON][CSV] | [🗑]     │ │
│ │         Error strip (bg-destructive/10) when error                                │ │
│ │ ┌─ ScanResultsTable ───────────────────────────────────────────────────────────┐ │ │
│ │ │ HOST │ PORT │ STATE │ SERVICE │ LATENCY ▁▁▁ │ BANNER                        │ │ │
│ │ │ (empty) BroadcastIcon pulse: "Scanning…"/"No open ports discovered"          │ │ │
│ │ └──────────────────────────────────────────────────────────────────────────────┘ │ │
│ └──────────────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

### `ScannerToolbar` (`scanner-toolbar.tsx`)
- **Target `Input`** (`w-48` mono, `bg-background/50 border-muted-foreground/20`): **Enter starts the scan** when `canScan && !isRunning`.
- **Start Scan** (`PlayIcon weight="fill"`, disabled until `canScan = target && parsedPorts.length > 0`) / **Stop Scan** (destructive, `SquareIcon` fill).
- **Preset `Select`** (`h-7 w-[105px]`): Quick/Web/Top 100/Full/Custom; picking a non-Custom preset overwrites `ports` from `PORT_PRESETS`.
- **Custom inline editor** (only `preset === 'Custom'`, `animate-in fade-in`): ports `Input` (`w-44 sm:w-56 pr-12`) with an absolute right-aligned live count chip (`{n}p` / `0p`, pointer-events-none) + `ListBulletsIcon` button (`TooltipTrigger render={<Button/>}` — kit render prop) opening the **Custom Ports Dialog** ("Open Custom Ports Editor (Multi-line, Ranges & Patterns)").
- **Progress**: inline text `current / total (N%)` (percentage from `useScannerToolbar`) **plus** a 2 px absolute bottom line (`bg-primary`, `transition-all duration-300 ease-out`, width = percentage).
- **Stealth switch**: a `label` wrapped `TooltipTrigger` (render prop) — icon + text swap between amber `EyeSlashIcon` "Stealth" and red `WarningCircleIcon` "Noisy - High Performance"; custom-sized `Switch` (`h-4 w-7`, knob 3) with `data-[state=checked]:bg-amber-500` / `data-[state=unchecked]:bg-red-500/60`; tooltip explains both modes ("SIEM/IDS" language).
- **`ScannerConfigPopover`** (gear, `PopoverTrigger render`, content `w-72 align=end`): Timeout (ms) + Concurrency right-aligned inputs (`w-20`), "Grab Service Banners" `Checkbox`, and a collapsible **Stealth Options** section (caret button, `animate-in fade-in slide-in-from-top-1`): Delay (ms), Jitter (ms), "Randomize Port Order" checkbox.

### `CustomPortsDialog` (`custom-ports-dialog.tsx`)
`DialogContent showCloseButton={false} sm:max-w-lg`. Header: `ListBulletsIcon` + "Custom Port List Editor" + live summary `Badge` (`formatPortsSummary`: "0 ports" / "1 port (#443)" / "Full Range (1-65535)" / "N ports"). **Quick Add** chips append with comma-joining: + Web Ports, + DB Ports, + Top 100 (`PORT_PRESETS.Top100`), + 1-1024; Clear (destructive hover) when non-empty. Draft `Textarea` (rows 7, mono) synced from `ports` on open; **Apply Ports (N)** commits the raw string (parsing happens everywhere downstream). A syntax guide grid (`bg-muted/40 rounded-lg`, `SparkleIcon`) documents: `1,2,3,4`, `1 - 4`, `3000..3010`, `80/tcp`, `web, db`, multiline.

### `parsePorts` mini-language (`lib/port-helpers.ts`) — the page's core utility
Strips `#`/`//` line comments; normalizes range operators `-`, `..`, `...`, `:` and separators (`;`, tab, `|`, newline, whitespace → comma); supports **named keywords** `web|http` (10 ports), `db|database` (3306, 5432, 1433, 1521, 27017, 6379, 9200, 5984), `top100`, `quick`, `all|full` (1–65535); strips `port` prefix and `/tcp|/udp|/http|/https` suffixes and trailing `(...)`; parses ranges (reversed allowed, clamped to 1–65535, ranges starting at 0 are dropped) and single ports; output deduped + sorted. Covered by `port-helpers.test.ts` (vitest).

### `ScanResults` family
- **`ScanWelcomeState`**: centered hero (`max-w-2xl`, `text-xl font-semibold` title, muted explainer) + 3 quick-scan buttons (`grid sm:grid-cols-3`, `bg-card/45`, `hover:border-primary/30 active:scale-[0.97]`): Quick Scan ("top 18 common ports (web, ssh, db)"), Web Ports, Top 100. `handleQuickStart` in `index.tsx` fills target with `127.0.0.1` if empty, applies the preset, and starts immediately.
- **`ScanResultsHeader`** (`h-9 border-b bg-muted/10`, only when `!isRunning`): `Scanned <code>target</code>` + `{openCount} open` outline Badge; actions (when results): Copy (`CopyIcon`→`CheckIcon` emerald "Copied", 2 s), optional JSON (`DownloadSimpleIcon`) and CSV (`FileCsvIcon`) buttons — **props are optional and the page passes neither**, so they never render; separator; clear `TrashIcon` quiet icon ("Clear results").
- **Error strip**: `bg-destructive/10 text-destructive font-mono text-2xs` with Dismiss ghost button.
- **`ScanResultsTable`**: kit `Table` (`text-xs`), sticky header (`bg-background border-b shadow-sm`, `h-8 py-0`), columns Host / Port (w-24, `text-primary font-semibold`) / State (outline Badge) / Service (capitalize, 'unknown' fallback) / **Latency** — mono ms + a 12×1 px mini bar (`w-12 h-1 bg-muted rounded-full`, fill width = `min(100, ms/1000*100)%`, color from `getLatencyColor`: >800 `bg-red-500`, >300 `bg-yellow-500`, else `bg-emerald-500`) / **Banner** (`max-w-[400px] truncate`, `<code select-all bg-muted/30 text-3xs>` with `title` tooltip, `-` placeholder at 40% opacity). Empty state: pulsing `BroadcastIcon h-8 w-8 opacity-30` + "Scanning…" / "No open ports discovered". Result rows `animate-in fade-in duration-300` as a block.
- `progress` and `concurrency` props on `ScanResults`/`ScanResultsHeader` are accepted but unused in the render (header shows target + open count only).

## 7. UI Libraries & Primitives Used

| Category | Items (actual imports) |
| :-- | :-- |
| UI kit | `Badge`, `Button`, `Checkbox`, `Dialog*`, `Input`, `Popover`, `PopoverContent`, `PopoverTrigger`, `Select*`, `Switch`, `Table*`, `Textarea`, `Tooltip`, `TooltipContent`, `TooltipTrigger` |
| Icons (`@phosphor-icons/react`) | `PlayIcon`, `SquareIcon`, `EyeSlashIcon`, `WarningCircleIcon`, `ListBulletsIcon`, `CheckIcon`, `CopyIcon`, `DownloadSimpleIcon`, `FileCsvIcon`, `TrashIcon`, `CaretDownIcon`, `CaretRightIcon`, `GearIcon`, `BroadcastIcon`, `SparkleIcon` (several `weight="fill"`) |
| State/IPC | `zustand` + `persist`, `invoke`, `listen` (`@tauri-apps/api/*`), `crypto.randomUUID()`, `navigator.clipboard` |
| Cross-store | `useNotificationStore` (warning alerts) |
| Animations | tailwindcss-animate utilities `animate-in fade-in`, `slide-in-from-top-1`, `duration-150/300` |
| Tests | `vitest` (`port-helpers.test.ts`) |

Kit idioms in heavy use: `render` prop on `DialogTrigger`/`PopoverTrigger`/`TooltipTrigger`, `leading="tight"`, `textSize="xs"`, `mono`, `size="sm"` on `TableCell`.

## 8. Keyboard Shortcuts & Interactions

- **Enter** in the target input starts the scan.
- Copy button flips to "Copied" for 2 s (`setTimeout`).
- Steal… none otherwise: no global hotkeys; the config popover and dialogs are pointer-driven.

## 9. Styling Details

- Input styling motif reused everywhere: `bg-background/50 border-muted-foreground/20 focus-visible:ring-primary/45 focus-visible:ring-1 focus-visible:ring-offset-0`.
- Stealth semantics: amber = stealth (eye-slash), red = noisy (warning) — inverted from the usual "red = dangerous action" since here red labels the *default high-speed* mode.
- Latency bar colors are raw Tailwind hues (red/yellow/emerald-500), not tokens.
- Welcome cards use `rounded-sm` (unusually square for this app) and `bg-card/45`.

## 10. Integration Points

- **Notifications drawer**: both noisy-mode paths push alerts into `useNotificationStore` (`source: 'Port Scanner'`).
- **Backend scanner**: `scan_type: 'connect'` only — SYN/UDP modes are not exposed; banner grabbing is a checkbox that flows straight into the request.
- **No cross-page bridges**: open ports are copied as `host:port` text lines, not pushed to Repeater/other pages. Export handlers exist in the component contract but are unimplemented upstream.

## 11. Notable Findings

1. **Stealth is re-enabled on every app restart** — the store's `merge` hard-codes `stealthMode: state?.stealthMode ?? true`… but since `stealthMode` *is* persisted, the `??` only fires when absent; the real force is `?? true` for missing keys. However `clearResults` resets `hasRun`, so a cleared page shows the welcome screen even after scans.
2. **`scanIdRef` Stop race**: `stopScan` reads the ref, but the ref is nulled in `startScan`'s `finally` — Stop is a no-op after the promise resolves even if events still trickle.
3. **Progress double-display** (text + bottom line) only renders while `progress.total > 0`; for `Full` scans `total` = 65,535 and the percentage bar is the primary feedback.
4. **`parsePorts` is unusually forgiving** (comments, named keywords, protocol suffixes, reversed ranges) — and it's unit-tested; the dialog's Apply sends the *raw string* so re-parsing is consistent everywhere.
5. **Dead props**: `ScanResults.progress/concurrency`, `ScanResultsHeader.onExportJson/onExportCsv` (buttons render only if handlers are passed — they aren't), and the `id` props on several inputs (no label associations).

## 12. Source Files Documented

`index.tsx` · `types.ts` · `constants.ts` · `hooks/use-port-scanner-page.ts` · `lib/port-helpers.ts` · `lib/port-helpers.test.ts` (header + cases) · `components/scanner-toolbar.tsx` · `components/scanner-config-popover.tsx` · `components/custom-ports-dialog.tsx` · `components/scan-results.tsx` · `components/scan-results-header.tsx` · `components/scan-results-table.tsx` · `components/scan-welcome-state.tsx` · `components/hooks/use-scanner-toolbar.ts` · `components/hooks/use-scan-results.ts` · `src/stores/port-scanner.ts` (store)
