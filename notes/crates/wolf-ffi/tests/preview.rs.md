# `crates/wolf-ffi/tests/preview.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Scroll-sync join

`WolfCoreHandle::previews()` looks each row's `source_bufnr` up in `wolf_core::Snapshot::markdown_cursors` and exposes the result as `FfiPreview::source_cursor_line()` and `source_cursor_col()`. No other test witnesses this join: the Lua-to-core half is pinned by `//crates/wolf-core:markdown_cursor` and the Swift arithmetic by `PreviewScrollSeamTests`, so without this test the join is covered only by compilation.

The contract pinned is the one the front-end consumes. The line is 1-based, matching the `L` of the `wl-{L}-{C}` marker scheme, and the column is a 0-based character column, its `C`. Both describe the row's own source buffer.

## Cursor-unknown sentinel

The `markdown_cursors` join in `previews()` ends in `unwrap_or_default()`, so a preview whose source buffer has never reported a cursor reads `(0, 0)`. The front-end gates on that, since `CursorPos.isKnown` is `line > 0`, and refuses to scroll. A sentinel of `1` would make every such preview jump to the top of the document instead of staying put.

`preview_file` reaches that state deterministically. It loads the linked file with a background `bufadd`/`bufload` and binds only the preview scratch into the window, so the source never becomes current and never fires the cursor autocmds: `emit_cursor` bails on any buffer that is not `nvim_get_current_buf()`, and the scratch is excluded by the `b:wolf_preview_source` guard. This is why `wolf_preview_file` has no cursor re-emit.
