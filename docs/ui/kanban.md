# Kanban (`kanban`) — UI Deep Dive

> Source: `references/hexbuffer/src/pages/kanban/` (9 files, ~1,210 lines)
> Exported page component: `KanbanPage`

## 1. Overview & Purpose

A task-tracking kanban board (the app's own project-management scratchpad, not pentest data). Cards with priority dots, tags, subtask progress bars, assignee avatars, and due dates are dragged between columns with **`@dnd-kit/core`** (mouse + touch sensors, `closestCorners` collision, `DragOverlay` with a 2°-rotated ghost). Three **group-by modes** — Status (To Do / In Progress / In Review / Done with WIP limits), Priority, and dynamic Assignee columns — re-interpret what "dropping a card on a column" means (status change / priority change / reassign). State is **in-memory `useState` only** — nothing persists, and there are no backend calls; the two modals (Create, Details) are plain controlled forms.

## 2. Annotated File Tree

```
kanban/
├── index.tsx                          (145)  DndContext wiring (sensors, collision, measuring), board scroll, DragOverlay, both modals
├── types.ts                            (28)  Priority, GroupBy, SubTask, KanbanCard, KanbanColumn
├── constants.ts                        (28)  PRIORITY_CONFIG, STATUS_COLUMNS (wip limits), PRIORITY_COLUMNS, GROUP_OPTIONS
├── hooks/
│   └── use-kanban-page.ts             (134)  All board state: cards, groupBy, dragging, modals, CRUD, subtasks
└── components/
    ├── kanban-toolbar.tsx               (57)  done/total progress bar, Group-by Tabs, Add card button
    ├── kanban-column-panel.tsx          (89)  Droppable column: header (color dot, WIP), drop zone, add-card strip
    ├── kanban-card-item.tsx            (123)  Draggable card: priority dot, description, tags, subtask bar, avatar, due date
    ├── kanban-add-modal.tsx            (289)  Create dialog (title/desc/status/priority/assignee/date/tags/subtasks)
    └── kanban-detail-modal.tsx         (321)  Edit dialog (same fields + subtask toggles + confirm()-delete)
```

## 3. Architecture & Dataflow

```
useKanbanPage (useState only)
  cards[] ── groupBy ──> columns (STATUS/PRIORITY constants | buildAssigneeColumns(cards))
  getColumnCards(colId): status → c.columnId === colId · priority → c.priority === colId
                          assignee → c.assignee === colId (undefined ⇒ 'Unassigned')
DndContext
  onDragStart(cardId) → draggingId
  onDragEnd(cardId, overId?) → patch the ONE field implied by groupBy:
      status   → columnId = overId
      priority → priority = overId
      assignee → assignee = overId ('Unassigned' ⇒ undefined)
  DragOverlay renders KanbanCardItem isOverlay (pointer-events-none, rotate-[2deg], w-[262px])
```

- **Drop semantics per mode** is the page's central trick: the same drop target mutates `columnId`, `priority`, or `assignee` (`handleDragEnd` in `use-kanban-page.ts`).
- **Assignee columns are derived**: first-seen order from cards, plus a guaranteed trailing `Unassigned` column (color fallback `oklch(0.6 0.04 240)`).
- **No persistence whatsoever**: reload wipes the board; ids are `crypto.randomUUID()`; the only "integration" is none.

## 4. State Management

All `useState` inside `useKanbanPage` (no Zustand, no localStorage, no IPC):

| State | Type | Notes |
| :-- | :-- | :-- |
| `cards` | `KanbanCard[]` | `{ id, title, description?, assignee?, assigneeColor?, dueDate?, priority, tags?, subtasks: SubTask[], columnId }` |
| `groupBy` | `'status' \| 'assignee' \| 'priority'` | default `'status'` |
| `draggingId` | `string \| null` | set at drag start, cleared at end |
| `isAddModalOpen`, `activeAddColumnId` | — | `openAddModal(colId?)` defaults to `'todo'` |
| `selectedCardId` | `string \| null` | presence **is** the detail modal's open flag (`isOpen={!!selectedCardId}`) |

Derived: `totalCards`, `doneCards` (count of `columnId === 'done'`). Card mutation helpers: `addCard` (trims title, requires it), `updateCard(id, patch)`, `deleteCard` (also closes detail), `toggleSubtask` (board-level quick toggle from the card... actually only wired from column→card item; detail modal edits its own local subtask list until Save).

## 5. IPC Contracts

**None.** Zero `invoke`, zero `listen`, zero stores. The page is entirely client-side and ephemeral.

## 6. UI Layout & Component Breakdown

```
┌─ KanbanPage ────────────────────────────────────────────────────────────────────────┐
│ ┌ KanbanToolbar (h-10 border-b bg-muted/40) ───────────────────────────────────────┐ │
│ │ 3/12 done ▁▁▁▁░░░░ (h-1 w-20 progress)    Group by: [Status|Assignee|Priority] [+]│ │
│ └──────────────────────────────────────────────────────────────────────────────────┘ │
│ Board (overflow-x-auto, minWidth: max-content, p-4 gap-4)                           │
│ ┌─ Column w-[280px] ─┐ ┌─ Column ─┐ ┌─ Column ─┐ ┌─ Column ─┐                        │
│ │ ● To Do      5     │ │ In Prog  │ │ In Review│ │ Done     │   ← WIP: 3/2 orange   │
│ │ ┌ drop zone ─────┐ │ │ 2/3      │ │ 1/2      │ │ 4        │      when violated    │
│ │ │[card][card]    │ │ │          │ │          │ │          │                        │
│ │ │Drop cards here │ │ │          │ │          │ │          │                        │
│ │ └────────────────┘ │ │          │ │          │ │          │                        │
│ │ [+ Add card]       │ │          │ │          │ │          │                        │
│ └────────────────────┘ └──────────┘ └──────────┘ └──────────┘                        │
│ (dragging) → source card opacity-40 scale-97; zones go dashed border-primary/30;     │
│              hover zone border-primary/60 bg-primary/5 ring-2; overlay card +2°     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

### `KanbanToolbar`
Left: mono `{doneCards}/{totalCards} done` + a mini progress bar (`h-1 w-20 rounded-full bg-muted`, fill `bg-primary transition-all duration-300`). Right: `Group by:` mono label + kit `Tabs` (one `TabsTrigger` per `GROUP_OPTIONS` entry) + **Add card** primary button (`PlusIcon weight="bold"`).

### `KanbanColumnPanel`
`useDroppable({ id: column.id })` on the outer `w-[280px]` wrapper. Header: column color dot (inline `backgroundColor` from the column's **oklch** color), title, and count — `{n}` or `{n}/{wipLimit}` turning orange (`text-orange-600 dark:text-orange-400` + `WarningIcon aria-label="WIP limit exceeded"`) when `cards.length > wipLimit` (In Progress = 3, In Review = 2). Drop zone styling has three states: idle (`border-border/40 bg-muted/10`), dragging-anywhere (`border-dashed border-primary/30 bg-primary/2`), hover-with-card (`border-primary/60 bg-primary/5 ring-2 ring-primary/10 shadow-sm`). Empty zone: dashed `h-20` "Drop cards here". Footer: full-width dashed **Add card** strip that opens the add modal pre-targeted at this column. `scrollbar-thin` on the card list.

### `KanbanCardItem`
`useDraggable({ id: card.id })` — attributes/listeners spread only when not the overlay copy (`isOverlay` disables dragging and skips the ref). Anatomy:
- **Priority dot** (`h-1.5 w-1.5` rounded, `PRIORITY_CONFIG[...].dot`: critical red-500 / high orange-400 / medium yellow-400 / low muted) + title (`text-[13px] font-medium`) + `DotsSixVerticalIcon` drag affordance.
- Description (`text-2xs line-clamp-2`), tag chips (`bg-muted text-3xs` with `TagIcon`).
- **Subtask progress**: `CheckCircleIcon` (all done, primary) vs `CircleNotchIcon` + `{done}/{total} subtasks` + `h-0.5` fill bar.
- **Footer**: assignee avatar (`h-5 w-5 rounded-full` colored via `assigneeColor ?? '#00c950'`, two-letter initials `slice(0,2).toUpperCase()`, black text) + due date mono with `CalendarBlankIcon`, **red when overdue** (`new Date(dueDate) < new Date()`).
- Dragging source: `opacity-40 scale-[0.97] border-primary/40`; hover: `hover:border-primary/50 hover:bg-muted`. Whole card is clickable → detail modal.
- Left padding motif: secondary rows indent `ml-[18px]` to align past the priority dot.

### `KanbanAddModal` / `KanbanDetailModal`
Near-twin `Dialog` forms (`max-w-md`): Title (required — `aria-invalid` + "Card title is required." inline error that clears as you type), Description `Textarea`, and a `grid-cols-2` field block: Status (`STATUS_COLUMNS`), Priority (`PRIORITY_CONFIG`), Assignee (**hard-coded `ASSIGNEE_OPTIONS`**: Unassigned / "AR (Arham)" #00c950 / "MK (Max)" #818cf8 / "JS (John)" #f472b6), Due Date (native `type="date"` input). Tags as comma-separated text. **Subtasks section** (add via Enter or + button, `crypto.randomUUID()` ids, list with remove `TrashIcon`; detail modal adds a native `checkbox` toggle with `line-through` done styling). Submit builds the `KanbanCard` payload (`unassigned` → `undefined` assignee; empty tags → `undefined`).
Differences: detail pre-fills from `card` on open (`useEffect [isOpen, card]`), can **toggle** subtasks, footer splits Delete (destructive, guarded by a **native `confirm()`**) from Cancel/Save Changes; add modal resets all fields on open.

## 7. UI Libraries & Primitives Used

| Category | Items (actual imports) |
| :-- | :-- |
| UI kit | `Button`, `Dialog`, `DialogContent`, `DialogFooter`, `DialogHeader`, `DialogTitle`, `Input`, `Label`, `Select*`, `Tabs`, `TabsList`, `TabsTrigger`, `Textarea` |
| Drag & drop | **`@dnd-kit/core`**: `DndContext`, `useSensor`/`useSensors` (`MouseSensor` distance 8, `TouchSensor` delay 200/tolerance 8), `DragOverlay` (`dropAnimation={null}`), `closestCorners`, `MeasuringStrategy.WhileDragging`, `useDroppable`, `useDraggable` |
| Icons (`@phosphor-icons/react`) | `PlusIcon`, `WarningIcon`, `CalendarBlankIcon`, `CircleNotchIcon`, `CheckCircleIcon`, `DotsSixVerticalIcon`, `TagIcon`, `TrashIcon` |
| Web APIs | `crypto.randomUUID()`, native `confirm()` (delete), native `<input type="date">` and `<input type="checkbox">` inside dialogs |
| Misc | `cn` |

No editor, no IPC, no store, no router interactions — the smallest dependency surface of any page.

## 8. Keyboard Shortcuts & Interactions

- **Enter** in the subtask input adds a subtask (both modals).
- Drag activation: 8 px mouse threshold, 200 ms touch hold (both tuned so clicks vs drags don't collide — noted "ponytail" comments).
- Delete uses the browser-native `confirm()` dialog — the only page doing so.

## 9. Styling Details

- Column colors are **oklch strings** applied via inline `style` (`STATUS_COLUMNS`: neutral 240 / cyan 200 / violet 280 / green 145; `PRIORITY_COLUMNS`: red 20 / orange 55 / yellow 90 / neutral 240) — a different color system than the Tailwind-token pages.
- `bg-primary/2` (2% tint) on dragging drop zones — an unusual alpha step.
- Fixed card width `w-[262px]` for the drag overlay matches the 280 px column minus padding.
- Avatar colors and the `#00c950` default are raw hex, not tokens.

## 10. Integration Points

- **None.** This page is an island: no HTTP history sends cards, no store is shared, nothing persists. It is the reference implementation's own planning tool, wired for demo data (`ASSIGNEE_OPTIONS` lists the project's developers).

## 11. Notable Findings

1. **Everything evaporates on reload** — cards live only in React state; compare with the persisted stores used everywhere else. Any real usage requires adding persistence first.
2. **Group-by changes mutation semantics**: the identical drag gesture reassigns status, priority, or assignee depending on the mode — powerful, but a drop in Priority mode silently overwrites `priority` with no undo.
3. **`selectedCardId` doubles as modal-open state**, and the detail modal receives `cards.find(...)` — deleting from another surface mid-open is handled by `card === undefined` → `return null`.
4. **Hard-coded assignee roster with personal names/colors** (`AR (Arham)`, `MK (Max)`, `JS (John)`) duplicated in *both* modals rather than shared constants — a refactor smell and a privacy note for the reference dump.
5. **WIP limits exist only visually**: exceeding them shows the orange warning + count tint but does not block drops.
6. **Native `confirm()`/`checkbox`/`date` inputs** mix with the kit — pragmatic, inconsistent with the app's Dialog-based confirmations elsewhere.

## 12. Source Files Documented

`index.tsx` · `types.ts` · `constants.ts` · `hooks/use-kanban-page.ts` · `components/kanban-toolbar.tsx` · `components/kanban-column-panel.tsx` · `components/kanban-card-item.tsx` · `components/kanban-add-modal.tsx` · `components/kanban-detail-modal.tsx`
