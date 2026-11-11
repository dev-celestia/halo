# Comparer (`comparer`) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/comparer/` (10 files, ~1,200 lines)
> Exported page component: `ComparerPage`

## 1. Overview & Purpose

A side-by-side text differ (Burp Comparer analog) built on the npm **`diff`** library. Two monospace input panes (Original **A** / Modified **B**, each with clipboard Paste/Clear buttons) feed a unified two-column diff table (A line numbers + Original, B line numbers + Modified) supporting three granularities — **Lines / Words / Chars** — with inline word/char highlighting inside changed line pairs. The diff is computed client-side on every keystroke (`useMemo`), both inputs and the mode persist to `localStorage`, and the toolbar offers Swap A/B, Hide Inputs (full-width diff), Copy A/B/Diff (unified `+/-/space` text), and Clear All. Fully client-side: **zero Tauri commands, no store, no IPC**.

## 2. Annotated File Tree

```
comparer/
├── index.tsx                            (141)  Shell: toolbar + vertical 35/65 split (inputs | diff) with hide-inputs mode
├── types.ts                               (1)  DiffMode = 'lines' | 'words' | 'chars'
├── constants.ts                          (13)  DIFF_MODE_OPTIONS, MODE_LABELS (MODE_LABELS unused)
├── hooks/
│   └── use-comparer-page.ts              (81)  localStorage-backed state, diff computation, unified-text copy
└── components/
    ├── comparer-toolbar.tsx             (260)  Mode select, Hide Inputs, Swap, Copy A/B/Diff, Clear All
    ├── comparer-inputs.tsx              (241)  A/B textarea cards with Paste/Clear headers
    ├── comparer-diff-view.tsx           (288)  Unified diff <table> + InlineRenderer
    └── hooks/
        ├── use-comparer-diff-view.ts    (123)  buildLines: change[] → aligned row model + inline parts
        ├── use-comparer-toolbar.ts       (35)  toggleShowInputs, copyA/copyB wrappers
        └── use-comparer-inputs.ts        (21)  clearA/clearB wrappers
```

## 3. Architecture & Dataflow

```
textarea A ──┐
textarea B ──┴─> useComparerPage (localStorage: comparer-value-a/b, comparer-diff-mode)
                     │ diffFunctions[mode](A, B) via npm 'diff' (diffLines/diffWords/diffChars)
                     ▼
                 Change[] ──> useComparerDiffView.buildLines ──> LineData[]
                     (removed+added pairs zipped row-by-row; words/chars mode adds
                      per-line inlineParts via a second diffWords/diffChars pass)
                     ▼
                 ComparerDiffView <table> (sticky header, row tints, InlineRenderer spans)
```

- **Diff functions**: `lines` uses `diffLines(a, b, { ignoreNewlineAtEof: true })`; `words`/`chars` use plain `diffWords`/`diffChars`.
- **Row alignment** (`buildLines`): pure added blocks emit empty-left rows; pure removed blocks emit empty-right rows; an adjacent removed→added pair is zipped up to `max(len)` rows so removed/added lines sit side by side, and each paired row gets `inlineLeft`/`inlineRight` from a second `diffWords`/`diffChars` pass over the two line strings (only when both non-empty; errors swallowed → no inline).
- **Persistence**: three `localStorage` keys — `comparer-value-a`, `comparer-value-b`, `comparer-diff-mode` — read in `useState` initializers and written in `useEffect` on every change.
- **Copy Diff** renders a unified patch (`buildUnifiedText`: `+ ` added, `- ` removed, two-space context, per line) to the clipboard.

## 4. State Management

All local (`useState` in `useComparerPage` + one `showInputs` boolean in `index.tsx`): `valueA`, `valueB`, `diffMode` (persisted as above), derived `diffResult: Change[]`, `hasContent = A||B non-empty`, `hasDiff = any added/removed part`. There is **no Zustand store** — the only page family member without one besides sql-injection.

## 5. IPC Contracts

**None.** No `invoke`, no `listen`, no backend involvement. Clipboard via `navigator.clipboard.{readText,writeText}` (page-level `handlePasteA/B` catch permission errors with a toast advising keyboard paste).

## 6. UI Layout & Component Breakdown

```
┌─ ComparerPage (p-2 → border rounded-md bg-card) ────────────────────────────────────┐
│ ┌ ComparerToolbar (h-10 bg-muted/40 rounded-t-md) ─────────────────────────────────┐ │
│ │ Mode: [Lines ▾ (w-20)] | [👁 Hide Inputs] [⇄ Swap A/B]   [⧉ Copy A][⧉ Copy B]     │ │
│ │                                                   [⧉ Copy Diff][🗑 Clear All]     │ │
│ └──────────────────────────────────────────────────────────────────────────────────┘ │
│ ┌ ResizablePanelGroup (vertical) ──────────────────────────────────────────────────┐ │
│ │ ┌ Inputs 35% (grid-cols-2, bg-muted/10) ───────────────────────────────────────┐ │ │
│ │ │ ORIGINAL TEXT (A) [⧉Paste][Clear] │ MODIFIED TEXT (B) [⧉Paste][Clear]        │ │ │
│ │ │ (mono textarea, resize-none)      │ (…)                                     │ │ │
│ │ ├─ handle ─────────────────────────┴─────────────────────────────────────────┤ │ │
│ │ └ Diff 65% ── sticky header [A|Original|B|Modified] ── tinted rows ───────────┘ │ │
│ └──────────────────────────────────────────────────────────────────────────────────┘ │
│ (Hide Inputs → diff occupies the full area; empty → GitDiffIcon + guidance)          │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

### `ComparerToolbar`
Left: `Mode:` label + compact kit `Select` (`h-6 w-20 py-0 text-2xs`, `[&_svg]:size-3` overrides the chevron); separator (`h-4 w-[1px]`); **Hide/Show Inputs** (`EyeSlashIcon`/`EyeIcon`); **Swap A/B** (`ArrowsLeftRightIcon`, disabled without content). Right: ghost **Copy A / Copy B / Copy Diff** (`CopyIcon`, Copy Diff disabled until `hasDiff`) and destructive-tinted **Clear All** (`TrashIcon`, `hover:bg-destructive/10`). All buttons `text-2xs px-2`. `useComparerToolbar` supplies toggle + copy wrappers around the page's `copyPanel(value, label)` helper (toast `Copied {label}`).

### `ComparerInputs`
Two identical cards in `grid grid-cols-2 gap-2` (`focus-within:ring-1 focus-within:ring-ring focus-within:border-ring`). Card header (`h-8 bg-muted/40 border-b`): uppercase `text-3xs` label ("Original Text (A)" / "Modified Text (B)") + ghost **Paste** (`ClipboardIcon` → clipboard.readText, toasts "Pasted into Original (A)" etc.) and **Clear** (destructive tint, disabled when empty). Body is a **raw `<textarea>`** (not the kit): `flex-1 resize-none outline-none text-xs font-mono bg-transparent`, placeholder "Paste or type original/modified text here...".

### `ComparerDiffView`
Empty state: `GitDiffIcon h-8 w-8 opacity-50` + "Enter text in both panels to see the diff". Otherwise an `overflow-auto` mono `text-xs` `<table border-collapse>`:
- **Sticky thead** (`bg-muted/95 backdrop-blur`): columns `A` (w-12) | `Original` | `B` (w-12) | `Modified`, uppercase `text-3xs` headers.
- **Row tints**: paired removed+added row `bg-yellow-500/[0.06]`; removed-only `bg-red-500/[0.07]`; added-only `bg-green-500/[0.07]`.
- **Line numbers**: separate A/B counters incremented only when the respective side isn't `empty` (`text-3xs text-muted-foreground/60`, right-aligned).
- **Content cells**: `whitespace-pre-wrap break-all`; removed text `text-red-600/90 dark:text-red-400/90`, added `text-green-600/90 dark:text-green-400/90`, empty side `opacity-30`, `\u00A0` placeholder for blank lines.
- **`InlineRenderer`**: for words/chars mode, changed pairs render `<span>` segments with `bg-green-400/30` / `bg-red-400/30 rounded-sm` on the precise added/removed substrings.

### `useComparerDiffView` model
`LineData { leftContent, rightContent, leftType: 'unchanged'|'removed'|'empty', rightType: 'unchanged'|'added'|'empty', inlineLeft?, inlineRight? }`; `InlinePart { text, type }`. The zip logic (removed immediately followed by added) is what produces the side-by-side pairing; standalone blocks stack on their own side.

## 7. UI Libraries & Primitives Used

| Category | Items (actual imports) |
| :-- | :-- |
| UI kit | `Button`, `Select`, `SelectContent`, `SelectItem`, `SelectTrigger`, `SelectValue` |
| Local primitives | `ResizablePanel/Handle/Group` (`@/components/ui/resizable`, vertical 35/65) |
| Diff engine | npm **`diff`** (`diffLines`, `diffWords`, `diffChars`, `type Change`) — no Monaco diff editor, no server |
| Icons (`@phosphor-icons/react`) | `GitDiffIcon`, `ArrowsLeftRightIcon`, `TrashIcon`, `CopyIcon`, `EyeIcon`, `EyeSlashIcon`, `ClipboardIcon` |
| Web APIs | `localStorage` (3 keys), `navigator.clipboard.{readText,writeText}` |
| Misc | `sonner` toasts, `cn` |

Notably absent: no `TextEditor`/Monaco — the inputs are raw `<textarea>`s; no Zustand; no icons beyond the seven listed.

## 8. Keyboard Shortcuts & Interactions

- None registered; native textarea editing (keyboard paste is the documented fallback when `clipboard.readText` is denied).
- Resizable split between inputs and diff; Hide Inputs swaps to a single full-height diff.

## 9. Styling Details

- Yellow marks *paired* change rows (both sides changed on the same line), red/green single-sided — a three-tint scheme distinct from the app's token palette.
- Very low-alpha tints (`/[0.06]`–`/[0.07]`) and `/90` text colors keep long diffs readable; `leading-[1.4]` for both header and rows.
- Everything sits on `bg-card` with `rounded-md`/`rounded-t-md` outer chrome.

## 10. Integration Points

- **HTTP History / Repeater / others** can copy responses into it manually — there is no programmatic "send to Comparer" bridge in this folder (the floating-link-card/components layer may add one; nothing here consumes it).
- Pure utility page: no persistence beyond localStorage, no backend state, safe to use offline.

## 11. Notable Findings

1. **Re-diff on every keystroke**: `diffResult` recomputes in a `useMemo` on each edit — with `Chars` mode on large inputs this can get expensive; there is no debounce.
2. **Double diff for inline highlighting**: words/chars mode runs the line diff *and* a per-paired-line second diff — correct results, 2× work.
3. **`localStorage` for potentially sensitive payloads** (requests/responses pasted for comparison) with no size cap or clearing on exit; keys are unprefixed (`comparer-value-a`) unlike the store pages' `halo-*` convention.
4. **Dead code**: `MODE_LABELS` in constants and `onCopyRef`-style patterns elsewhere aren't used here; `handleClearA/B` and `useComparerToolbar` are thin wrappers with single call sites.
5. **Clipboard paste errors are handled** (permission denied → toast suggesting keyboard paste), unlike most pages that assume write access.

## 12. Source Files Documented

`index.tsx` · `types.ts` · `constants.ts` · `hooks/use-comparer-page.ts` · `components/comparer-toolbar.tsx` · `components/comparer-inputs.tsx` · `components/comparer-diff-view.tsx` · `components/hooks/use-comparer-diff-view.ts` · `components/hooks/use-comparer-toolbar.ts` · `components/hooks/use-comparer-inputs.ts`
