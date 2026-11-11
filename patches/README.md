# GPUI patches (LSP hover / diagnostic popovers)

`gpui-base` 0.7.2 draws fenced code blocks with a border and a language chip.
Hover popovers already pass a transparent code-block background so content can
sit in the card — without a matching “bare” mode, that produced nested pills
and overlapping `rust` labels.

| Patch | Effect |
| --- | --- |
| `gpui-base-bare-codeblock.patch` | Transparent code-block bg → no border, no language chip |
| `gpui-component-popover-markdown.patch` | Roomier paragraph gap + 12px code in editor popovers |
| `gpui-component-hover-padding.patch` | Hover card padding `px_3` / `py_2` + theme border |
| `gpui-component-diagnostic-code.patch` | Diagnostic popover shows LSP `code` / `source` |

Recreate vendors (gitignored) after `cargo fetch`:

```bash
./scripts/ensure-gpui-vendor.sh
```

Workspace `Cargo.toml` maps both crates via `[patch.crates-io]`.
