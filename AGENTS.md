# UI state

Shared writes use `celestia_ui::state`. The library guide is [Desktop State](../celestia-starter/apps/web/content/docs/desktop-state.mdx): `StoreHandle` for the value, `StoreCommit::on_store_commit` after each committing write.

BridgeState in the store is the source of truth. The shell repaints when a store update commits. Token text is the one update that does not repaint the shell.

## Show or hide a component

Keep the flag on BridgeState. Flip it inside `Bridge::state(cx).update`. Paint it with `.when(flag)`.

```rust
Bridge::state(cx).update(cx, |s| {
    s.coder.agent_context_open = !s.coder.agent_context_open;
});
```

```rust
.when(state.coder.agent_context_open, |el| el.child(render_context_rail(...)))
```

## Change data

Write the new value in that same `update`. Lists, settings, git flags, and toggles all use it. The shell copies the store and repaints. Do not keep a second copy on the view and write only there.

## Streaming text

The event pump holds paint for token-only batches. A view that must move during tokens observes the store itself, the way AgentTranscript does, and reads `Bridge::state(cx)` rather than the shell snapshot.

## Leave these alone

Do not add a field to `coder_chrome_eq` or `suppresses_shell_refresh` to make a toggle visible. Do not call `hold_paint` outside the event pump.
