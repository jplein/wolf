# Documentation refactor — miscellaneous recommendations

Last verified at commit `359483870068335bda022a3a0e3533bfa65f422a`.

Findings surfaced while executing [`docs-refactor-plan.md`](docs-refactor-plan.md) that were out of that plan's scope. The plan covered Markdown under READMEs; everything below is either a source change, a build change, or a verification still owed, so none of it was actioned in that pass. Each section states its own status.

The `specs/architecture.md` and `build.md` cleanup is tracked separately and is not repeated here.

Each item is marked with how it was established:

- **Verified** — checked directly against the source or build files during this pass.
- **Reported** — surfaced by a subagent and not independently re-checked. Confirm before acting.

## 1. Font licence obligations — discharged in the Rust tree

**Verified. Closed on `linux-feat/typography`, with one optional macOS-only residual.**

The four bundled fonts' licence texts are compiled into `wolf-core` by `src/licenses.rs` and reach a reader two ways: `render_html` emits them as an HTML comment in every rendered preview document's `<head>`, and `wolf-gnome`'s About dialog shows the same block. `crates/wolf-core/BUILD.bazel`'s `compile_data` glob was widened to `src/markdown/assets/*.txt` and `src/licenses/*.txt` to match, which is mandatory rather than tidiness: without it the `rust_library` does not compile under Bazel at all.

The registry covers Inter and Latin Modern Math, which ship as base64 faces inside every preview document, and JetBrains Mono and iA Writer Mono S, which ship as editor faces. Its tests pin that each text is the licence its row names, that the rendered block carries all four, and that no text can terminate the HTML comment it is emitted inside. What the two preview faces' licences require of anyone shipping Wolf is unchanged, and the two editor faces ship under the same SIL OFL 1.1 as Inter:

| Font              | Licence           | Obligation                                                                                                                                                         |
| ----------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Inter             | SIL OFL 1.1       | The copyright notice and licence must accompany every copy of the font, including an embedded one. The font may not be sold on its own, and may not be relicensed. |
| Latin Modern Math | GUST Font Licence | The licence text must accompany redistribution. A modified font must be marked as changed and renamed; GUST also requests a manifest of renamed files.             |

The residual is macOS-only and optional: whether the two preview fonts' texts should _also_ ship as files in `Contents/Resources/` with matching `crates/wolf-macos/bundle_layout_test.sh` checks, given both already ship compiled into the binary. That script checks `Contents/Resources/fonts/OFL.txt` and `Contents/Resources/fonts/LICENSE.md`, which are JetBrains Mono's and iA Writer Mono S's. It is tracked in [`macos-followups.md`](macos-followups.md), on that branch's `licenses`-module row.

## 2. A possible latent test failure

**Verified that the mismatch exists; the consequence is unconfirmed.**

`bazel/lighthouse/extensions.bzl` pins commit `514dec2c79701b74082defc19090bd1dae57765b`. `crates/wolf-ffi/tests/lighthouse_load.rs` asserts concrete colours and attributes them, in five comments, to commit `9da2bba190305b2c8d5d300c254a48c487dd4a1e`:

- titlebar bg `#1b1b1b`, fg `#f1f1f1`
- cursor fill `#009fce`, glyph `#000000`

Either those highlight groups are unchanged between the two commits and the comments are merely mislabelled, or the assertions are stale and the test fails. This cannot be settled on a Linux host — it needs a real Neovim with the staged lighthouse tree, so run `bazel test //crates/wolf-ffi:lighthouse_load` on the macOS host to find out.

The same stale hash was removed from `bazel/lighthouse/README.md` and `build.md` during the refactor. The test file is the only place it survives.

## 3. Source comments that contradict the code

Per CLAUDE.md, a divergence between documentation and implementation is a bug. These are all in source comments rather than Markdown, so the docs pass left them alone.

### The `-n` launch flag — verified, resolved in the comments

`nvim::spawn` in `crates/wolf-core/src/nvim.rs` deliberately omits `-n`, and both its doc comment and the comment in its body say so: `-n` forces `updatecount = 0`, which disables swap files entirely. Swap files are the mechanism behind the crash-recovery behaviour the root README advertises. No comment in `nvim.rs` or `runtime/nvim/init.lua` claims the flag. Anyone who "restores" it would silently remove unsaved-edit recovery.

### Others — reported

| Location                                                         | Contradiction                                                                                                                                                 |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/wolf-macos/Sources/WolfMacOS/PickerWakeBridge.swift`     | Same per-window claim, and says the wake refreshes "the right window's picker".                                                                               |
| `crates/wolf-gnome/src/lib.rs`, `src/pane_view.rs`               | Say the cmdline strip paints grid 1. `draw_cmdline` reads `snap.cmdline` / `snap.toast` and never touches grid 1.                                             |
| `crates/wolf-gnome/src/lib.rs`                                   | Says a native command line remains deferred. One exists.                                                                                                      |
| `crates/wolf-gnome/src/cursor.rs`                                | Says `app.rs` is the only GTK glue. `draw_cursor` lives in `pane_view.rs`.                                                                                    |
| `crates/wolf-macos/Sources/WolfKeyMap/PaneGeometry.swift`        | Property docs say `gridRect == paneRect` and the paint offsets are zero. Both are false whenever the column cap centres the grid.                             |
| `crates/wolf-macos/Sources/WolfKeyMap/TreeRefreshGate.swift`     | Header names `FileTreeView` as the only owner. `SearchView.swift` also constructs one and uses all four methods.                                              |
| `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift` | Says `EmptyStateView` is added first so the picker overlays sit above it. The pickers are `PickerPanelHost` child panels, so that ordering carries no weight. |
| `bazel/nvim-treesitter/extensions.bzl`                           | Overlay comment calls the plugin highlighting-disabled. Its own `BUILD.bazel` and `init.lua` enable highlighting.                                             |
| `crates/wolf-ffi/tests/commands.rs`                              | Const docs carry `renumbered from 18 to 20` and `renamed from Close File`.                                                                                    |

## 4. Stale planning scaffolding in source files

**Verified by count** (`git grep -l -i` over `*.rs` and `*.swift` for the four phrases that follow). 30 source files still carry comments addressed to an implementer of work that has since shipped — `does not exist yet`, `will not compile until`, `NOTE to coder`, `NOTE(test-writer)`.

| Directory                   | Files affected |
| --------------------------- | -------------- |
| `crates/wolf-core/tests`    | 15 / 51        |
| `crates/wolf-ffi/tests`     | 9 / 39         |
| `crates/wolf-core/src`      | 2 / 27         |
| `crates/wolf-gnome/src`     | 3 / 54         |
| `crates/wolf-macos/Sources` | 1 / 86         |
| `crates/wolf-macos/Tests`   | 0 / 44         |

This is the single largest cleanup available and it is almost entirely deletion. It is also actively misleading: a reader who believes the comments will think the feature is unimplemented and the test unrunnable.

## 5. Dead or orphaned code

| Item | Status |
| --- | --- |
| `_G.wolf_tab_cwd(handle)` in `runtime/nvim/init.lua` | **Verified orphaned.** No Rust or Swift caller. `cwd_for_tabpage` reads the `wolf_tab_cwds` snapshot map instead, and its doc in `api.rs` describes the singular RPC as superseded. Deletion candidate. |
| `ShortcutSeam` | **Verified unwired.** No production call site; the only references in `Sources/` are two comments. Its tests are its sole exercise. New Window is a menu key equivalent, and menu equivalents pre-empt `keyDown`. Either wire it or delete it — but see the defect below before wiring. |
| `PaletteSeam.isPaletteTrigger` | Reported dormant — no caller outside its own test. |
| `PickerView.init` | Reported to store `paneContainer` and never read it. |
| `FfiWinInfo::hidden` | Reported dead, retained deliberately for ABI stability. Worth leaving, worth knowing. |
| `regenerate_png_golden` / `tests/testdata/paint_golden.png` | Reported write-only: the file is written and never read, so it gates nothing despite the name. |

## 6. Possible defects worth a look

- **`ShortcutSeam.isNewWindowTrigger` cannot do its job.** Verified: it takes `charactersIgnoringModifiers` and `command` and never consults shift, so it returns true for both `⌘N` and `⇧⌘N`. Wiring it into `keyDown` as-is would make New and New Window indistinguishable.
- **`STATIC_COMMAND_COUNT` is a hand-maintained mirror.** `STATIC_COMMAND_COUNT` in `crates/wolf-core/tests/palette_rank.rs` hardcodes `27`, which matches `COMMAND_SPECS.len() - 1` (28 specs, one excluded) — so this is a latent hazard, not a live failure. Adding a command breaks two assertions until someone bumps it. Deriving it from `COMMAND_SPECS.len() - 1` removes the trap.
- **Sidebar search toggles may be seeded inconsistently.** Reported: `WolfWindowController.showSidebar(panel: .search)` focuses the field but never calls `seedToggles()`, which only the header magnifier action does. Worth confirming the intent.
- **`DividerDragSeam` takes `leadingOverhead` and `trailingOverhead` separately**, but `PaneContainerView.mouseDragged` passes the same value to both. Reported. If the asymmetry is deliberate, it needs a comment; otherwise it reads as dead generality a future refactor would collapse.
- **`nvim_bin_or_skip()` in `crates/wolf-ffi/tests/common/mod.rs` hardcodes its skip banner** to `SKIP: wolf-ffi bridge_smoke`. Verified. Roughly thirty real-nvim tests share the helper, so every one of them prints the wrong test name when it skips, which makes a skipped suite hard to read.
- **`crates/wolf-gnome/testdata/fonts.conf` is a template, not a usable config.** Reported: `flake.nix` substitutes `@DEJAVU_FONTS@` and points `FONTCONFIG_FILE` at a temporary copy, so editing the committed file has no effect until the next `nix develop`.

## 7. Verification still owed

- **`bazel test //tools/lint:markdown` has never run through Bazel.** Bazel is not available on the Linux host this work was done on, so the checker was verified by running `tools/lint/markdown_check.sh` directly over all 35 in-scope docs (no findings) and by parsing every touched `BUILD.bazel`. The Bazel wiring — 16 `markdown_srcs` filegroups, the `sh_test`, and its place in `:check` — is unexercised. Run it on the macOS host before relying on it.
- **`bazel test //crates/wolf-ffi:lighthouse_load`**, per section 2.
- The `BUILD.bazel` and `extensions.bzl` comments under `bazel/*` carry the same all-caps voice the READMEs did. Not covered by any linter, since `//tools/lint:markdown` only reads Markdown.
