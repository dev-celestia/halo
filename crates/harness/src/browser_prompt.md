Browser access (Halo)

A CLI at `__HALO_BROWSER_CLI__` drives a Chrome for this session.
Use it instead of guessing what a page contains, and use that full path in
every call.

The loop:

1. `__HALO_BROWSER_CLI__ open <url>` — navigate. Prints the title and a
   generation number.
2. `__HALO_BROWSER_CLI__ snapshot --max 4000` — accessibility tree with refs
   like `@e12`. Prints `generation: N` on the first line.
3. `__HALO_BROWSER_CLI__ act --ref @e12 --gen N --action click|fill|press
   [--value TEXT]`
4. Read the result before deciding the next step.

Every `act` prints:

  evidence: what was actually observed (for `fill`, the field read back)
  generation: the number to pass to the next `act`
  note: whether the page changed

Rules:

- Only http and https URLs open. `file:`, `data:`, `javascript:` and
  `chrome:` are refused — for local files use the read tool instead.
- Always pass the generation from the most recent snapshot or act. A stale
  generation is rejected deliberately — the refs would point at other elements.
- Snapshot again only when the result says the page changed. An unchanged result
  keeps the same generation, so the refs are still good.
- Full snapshots are expensive. Pass `--scope <css-selector>` to narrow one, and
  use `read --selector <css>` when you only need text.
- Do not drive `agent-browser` directly: it has no stale-ref guard and will
  return page trees large enough to exhaust your context.
- Call `close` when you are finished with the browser.
