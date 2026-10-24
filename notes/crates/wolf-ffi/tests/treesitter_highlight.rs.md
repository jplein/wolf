# `crates/wolf-ffi/tests/treesitter_highlight.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Injection sentinel

The bridge has no Lua-eval accessor, so a typed `:lua` line, sent through the same `send_key` path Swift uses, writes two lines to a sentinel file. Line 1 records whether the highlighter is active on the markdown buffer. Line 2 records whether the top-level langtree has an injected `go` child after `p:parse(true)`, a full parse that resolves injections synchronously.

The probe checks `children()["go"]` and also iterates the children for one whose `child:lang()` is `go`, because the keying of `:children()` can vary. The statements are `;`-separated so the whole probe stays one `:lua` line.
