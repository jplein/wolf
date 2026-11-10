# Following a link in the preview

Last verified at commit `24af6404466d9cc099af52ccec8d7c89155bc6f5`.

Every link a user clicks inside a preview is cancelled, and the web view never navigates itself. The `decide-policy` handler answers the decision with `ignore()`, acts out of band — repointing the pane through core, scrolling to an anchor, or handing a foreign scheme to the platform launcher — and returns `true`. The decision is answered before anything else runs, because `preview_file` is a blocking RPC and holding WebKit's signal emission open for a whole timeout budget would freeze the frame.

**The return value is a true-handled accumulator, and inverting it fails silently in both directions.** `true` says this handler answered the decision; `false` says the default handler decides, which for a navigation calls `use_()`. Returning `true` unconditionally never answers the initial load's own decision and leaves the preview permanently blank. Returning `false` after `ignore()` lets the default handler answer a decision already answered: the first answer still wins, and the code stops saying what it decided.

Only a `LinkClicked` navigation is touched, and the decision-type check ahead of the downcast is not redundant: a `NewWindowAction` decision is the same class, so a downcast-only filter would treat a `target="_blank"` as an in-page navigation. The initial `load_html` arrives as `NavigationType::Other`, which makes the filter exactly "anything that is not a user click is allowed through untouched".

**Every divergence from macOS narrows what may be launched, and they share one reason.** On GNOME the hand-it-to-the-default-handler path can start an arbitrary process, the trigger is a string in a document the user merely previewed, and there is no Gatekeeper or quarantine counterpart to lean on. So the four shapes [`src/README.md`](../../crates/wolf-gnome/src/README.md#preview_linkrs) names are refused where `PreviewLinkSeam` opens them, and a `file:` link core declines to preview is printed rather than handed to the desktop. A refusal is the reversible end of that decision; a launch is not.

**The base URI names the source file, never its directory, and carries no trailing slash** — `file:///home/u/proj/alpha.md`, percent-encoded. Relative-reference resolution replaces the last segment, so a `beta.md` in the document answers `file:///home/u/proj/beta.md`. Both trailing-slash mistakes fail quietly rather than loudly: a slash after the filename resolves one level too deep, and naming the directory without one resolves against its parent.

**The file origin is a security surface, not only a convenience.** A relative `![](img.png)` resolving from disk is its benign half. The sharp half is that an absolute-path `<script src>` or `<link rel="stylesheet">` naming anywhere on disk both loads and runs, and `wolf_core::markdown` passes raw HTML blocks through verbatim, so a previewed document chooses them.

**The bound is as load-bearing as the hazard.** This crate sets nothing on `webkit6::Settings`, so `allow-file-access-from-file-urls` and `allow-universal-access-from-file-urls` keep their `FALSE` defaults: script-initiated cross-origin access is gated, the declarative loads above are not. Such a document runs local script and style but cannot read local files back out.

An inline `<script>` runs on the full-load path in any case; what the base URI adds is sourcing one from an arbitrary path, and dropping the base URI to be rid of that takes every relative link with it. `load_preview_document`'s doc comment in [`src/pane_view.rs`](../../crates/wolf-gnome/src/pane_view.rs) carries the measurement, including why the termination hazard its own API documentation warns of does not fire on this front-end.

**A source switch needs a full load, and the head-offset guard cannot see one.** The rendered `<head>` is byte-identical across files by design, which is exactly why the in-place body swap's own guard is blind to a link follow. `preview_ui::document_update` is a separate conjunct ahead of it, keyed on the source bufnr; without it a linked file's body lands in a document whose base URI still names the previous file's directory.

The same rule forces a full load whenever a cross-file fragment is waiting, and that coupling is the whole mechanism: `load-changed`/`Finished` fires for a real `load_html` and never for a body swap, so the scroll would otherwise have no edge to fire on. A successful repoint always writes the pending slot, to the clicked fragment or to nothing — writing only a fragment leaves a stale anchor for the next file to be scrolled to.

**macOS's "a load is in flight" rule is deliberately not ported.** Porting it means porting the in-flight counter, both failure callbacks and the superseded-load reasoning, for a hazard that predates the link path and is not made worse by it. It is a known divergence, recorded rather than built.
