# `crates/wolf-gnome/tests/smoke_headless/preview.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Markdown preview fixtures

The scenario opens the other file, splits with `<C-w>v`, opens the Markdown source in the new pane and fires Preview there. `wolf_open_preview` binds in place, replacing the source pane's buffer, so the source's raw text is in no pane. Neovim cannot draw `PREVIEW_OCR_BODY` and neither file name contains it, so an OCR hit can only be WebKit's pixels. A placement that split a third pane would leave the source visible, which is why the persisted layout must hold two leaves and the readback two windows.

The live edit runs after the Edit leg because only a second load into a live web view takes the JSON body-swap branch (`pane_view::preview_swap_payload`); the first goes through `load_html` whatever the encoding does. A single-encoded payload prints `JSON Parse error: Unrecognized token '<'` on every keystroke while the preview keeps the stale document.

Editing needs the source in a window, and Edit in Another Pane puts it in a split, the leg's own discriminator; the in-place Edit replaces the preview it is fired from. The marker is typed there, so the other file is opened over it before capture B, leaving only the preview to draw it. A live edit before the Edit leg needs a `:e` of the source that would satisfy that leg's readback alone.

Neither file name contains a heading term, and both names and the two readbacks are in `RELAUNCH_FIXTURE_NAMES`. `PREVIEW_OCR_STATUS` is an ordinary word, so the fixture guard also checks that nothing this smoke puts on screen spells it.

## Preview link click fixtures

The one pointer smoke whose click target is something WebKit drew. It shares `POINTER_PREAMBLE` with the other `pointer_scenario` bodies.

The cwd is not the directory the link resolves in. With `load_html(html, None)`, WebKitGTK hands the handler the raw relative href (`uri="lnkinto.md"`), `preview_link::classify` takes its schemeless row, and `WolfCore::preview_file` passes the relative path to Lua's `filereadable`, which resolves it against the captive Neovim's working directory. With the fixtures at the project root that is the source's directory, so the follow succeeds by coincidence and the base URI is invisible. `LINK_FIXTURE_DIR` separates the two answers.

No fixture path contains an `ASSERTED_OCR_TERMS` entry, `preview` included. The registered names carry the `LINK_FIXTURE_DIR` prefix, so the guard's substring loop covers the directory with no exemption block. The smoke's observables are raw pixels in the preview pane and a `tabpagebuflist()` readback, which no sidebar row can move.

## Fire scope arithmetic

Assertions 12 and 13 read `prev.render`, whose rows exist whether or not the sample sat in a `pane.prev` region, so deleting the `frame_metrics::scope(Bucket::PanePrev)` guard from the fire closure leaves them unchanged and makes `frame_stats.rs`'s nesting table false.

A pass is opened by every pane wake that reaches it plus every performing coalescer fire, so `count(pane.prev)` is `count(pane.wake)` minus the wakes that return before the pass, plus the fires. The subtracted term is zero here: with the fire's scope deleted the counts are equal (299 against 299). So the comparison is strict, since a `>=` passes the reverted build.

The strict margin is the performing fires, +4 on three green runs (298/294, 303/299, 309/305) against +0 with the scope deleted. Comparing against `prev.render` proves nothing, because the pass runs on nearly every wake and 298 > 7 holds even with the coalescer removed.
