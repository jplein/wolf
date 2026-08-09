# Documentation refactor — miscellaneous recommendations

Findings surfaced while executing [`docs-refactor-plan.md`](docs-refactor-plan.md) that were out of that plan's scope. The plan covered Markdown under READMEs; everything below is either a source change, a build change, or a verification still owed, so none of it was actioned.

The `architecture.md` and `build.md` cleanup is tracked separately and is not repeated here.

Each item is marked with how it was established:

- **Verified** — checked directly against the source or build files during this pass.
- **Reported** — surfaced by a subagent and not independently re-checked. Confirm before acting.

## 1. Font licence obligations are unmet in the shipped app

**Verified. This is a legal obligation, not a style issue, and it is the highest-priority item here.**

`crates/wolf-core/src/markdown.rs` embeds two fonts into every rendered preview document by `include_str!`-ing base64 sidecars:

- `assets/inter-variable.woff2.b64` → `INTER_FONT_FACE`
- `assets/latinmodern-math.woff2.b64` → `MATH_FONT_FACE`

The font bytes therefore ship inside the binary and inside every preview page. The licence texts do not. `crates/wolf-core/BUILD.bazel` sets:

```python
compile_data = glob(["src/markdown/**/*.css", "src/markdown/assets/*.b64"]),
```

`inter-OFL.txt` and `latinmodern-math-GUST-FONT-LICENSE.txt` match neither pattern, are not compiled in, and are not placed in the `.app`. `crates/wolf-macos/bundle_layout_test.sh` checks `Contents/Resources/fonts/OFL.txt` and `Contents/Resources/fonts/LICENSE.md`, which cover JetBrains Mono and iA Writer Mono S — different fonts.

What each licence requires of anyone shipping Wolf:

| Font              | Licence           | Obligation                                                                                                                                                         |
| ----------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Inter             | SIL OFL 1.1       | The copyright notice and licence must accompany every copy of the font, including an embedded one. The font may not be sold on its own, and may not be relicensed. |
| Latin Modern Math | GUST Font Licence | The licence text must accompany redistribution. A modified font must be marked as changed and renamed; GUST also requests a manifest of renamed files.             |

Suggested fix: bundle both licence files into the `.app` alongside the fonts they cover, and extend `bundle_layout_test.sh` with checks for them so the obligation cannot silently regress. The obligations are documented in [`crates/wolf-core/src/markdown/README.md`](crates/wolf-core/src/markdown/README.md), but documenting an obligation does not discharge it.

## 2. A possible latent test failure

**Verified that the mismatch exists; the consequence is unconfirmed.**

`bazel/lighthouse/extensions.bzl` pins commit `514dec2c79701b74082defc19090bd1dae57765b`. `crates/wolf-ffi/tests/lighthouse_load.rs` asserts concrete colours and attributes them, in five comments, to commit `9da2bba190305b2c8d5d300c254a48c487dd4a1e`:

- titlebar bg `#1b1b1b`, fg `#f1f1f1`
- cursor fill `#009fce`, glyph `#000000`

Either those highlight groups are unchanged between the two commits and the comments are merely mislabelled, or the assertions are stale and the test fails. This cannot be settled on a Linux host — it needs a real Neovim with the staged lighthouse tree, so run `bazel test //crates/wolf-ffi:lighthouse_load` on the macOS host to find out.

The same stale hash was removed from `bazel/lighthouse/README.md` and `build.md` during the refactor. The test file is the only place it survives.

## 3. Source comments that contradict the code

Per CLAUDE.md, a divergence between documentation and implementation is a bug. These are all in source comments rather than Markdown, so the docs pass left them alone.

### The `-n` launch flag — verified, and the most consequential

Three comments state the captive Neovim launches with `-n`:

- `crates/wolf-core/src/nvim.rs:5`
- `crates/wolf-core/src/nvim.rs:1502`
- `runtime/nvim/init.lua:6`

The spawn code around `crates/wolf-core/src/nvim.rs:1530` deliberately omits it and says so inline: `-n` forces `updatecount = 0`, which disables swap files entirely. Swap files are the mechanism behind the crash-recovery behaviour the root README advertises. Anyone who "restores" the documented flag would silently remove unsaved-edit recovery.

### Others — reported

| Location                                                         | Contradiction                                                                                                                                                 |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/wolf-ffi/src/lib.rs` (near lines 390 and 1208)           | Calls the wake token per-window. It is per-project — one captive Neovim shared by N views.                                                                    |
| `crates/wolf-macos/Sources/WolfMacOS/PickerWakeBridge.swift`     | Same per-window claim, and says the wake refreshes "the right window's picker".                                                                               |
| `crates/wolf-gnome/src/lib.rs`, `src/pane_view.rs`               | Say the cmdline strip paints grid 1. `draw_cmdline` reads `snap.cmdline` / `snap.toast` and never touches grid 1.                                             |
| `crates/wolf-gnome/src/lib.rs`                                   | Says a native command line remains deferred. One exists.                                                                                                      |
| `crates/wolf-gnome/src/cursor.rs`                                | Says `app.rs` is the only GTK glue. `draw_cursor` lives in `pane_view.rs`.                                                                                    |
| `crates/wolf-macos/Sources/WolfKeyMap/PaneGeometry.swift`        | Property docs say `gridRect == paneRect` and the paint offsets are zero. Both are false whenever the column cap centres the grid.                             |
| `crates/wolf-macos/Sources/WolfKeyMap/TreeRefreshGate.swift`     | Header names `FileTreeView` as the only owner. `SearchView.swift` also constructs one and uses all four methods.                                              |
| `crates/wolf-macos/Sources/WolfMacOS/WolfWindowController.swift` | Says `EmptyStateView` is added first so the picker overlays sit above it. The pickers are `PickerPanelHost` child panels, so that ordering carries no weight. |
| `bazel/nvim-treesitter/extensions.bzl`                           | Overlay comment calls the plugin highlighting-disabled. Its own `BUILD.bazel` and `init.lua` enable highlighting.                                             |
| `runtime/nvim/init.lua` (around 1590)                            | Two doc comments have merged into one block: it opens describing `wolf_next_untitled` and switches mid-block to `wolf_untitled_swap_exists`.                  |
| `runtime/nvim/init.lua` (around 1853 and 2036)                   | Carries a "correction" narrative resolving an earlier wrong claim. The measured fact is worth keeping; the narrative is history.                              |
| `crates/wolf-ffi/tests/commands.rs`                              | Const docs carry `renumbered from 18 to 20` and `renamed from Close File`.                                                                                    |

## 4. Stale planning scaffolding in source files

**Verified by count.** 61 source files still carry comments addressed to an implementer of work that has since shipped — `does not exist yet`, `will not compile until that lands`, `NOTE to coder`, `NOTE(test-writer)`. Several embed whole BUILD stanzas that already exist. `crates/wolf-core/src/liveness.rs` opens with roughly 37 lines of it, including an "ambiguity flagged for the coder" that was resolved.

| Directory                   | Files affected |
| --------------------------- | -------------- |
| `crates/wolf-macos/Tests`   | 22 / 40        |
| `crates/wolf-core/tests`    | 19 / 45        |
| `crates/wolf-ffi/tests`     | 12 / 37        |
| `crates/wolf-core/src`      | 4 / 26         |
| `crates/wolf-gnome/src`     | 3 / 12         |
| `crates/wolf-macos/Sources` | 1 / 79         |

This is the single largest cleanup available and it is almost entirely deletion. It is also actively misleading: a reader who believes the comments will think the feature is unimplemented and the test unrunnable.

## 5. Dead or orphaned code

| Item | Status |
| --- | --- |
| `_G.wolf_tab_cwd(handle)`, `runtime/nvim/init.lua:1215` | **Verified orphaned.** No Rust or Swift caller. `cwd_for_tabpage` reads the `wolf_tab_cwds` snapshot map instead, and `api.rs:961` describes the singular RPC as superseded. Deletion candidate. |
| `ShortcutSeam` | **Verified unwired.** No production call site; the only references in `Sources/` are two comments. Its tests are its sole exercise. New Window is a menu key equivalent, and menu equivalents pre-empt `keyDown`. Either wire it or delete it — but see the defect below before wiring. |
| `PaletteSeam.isPaletteTrigger` | Reported dormant — no caller outside its own test. |
| `PickerView.init` | Reported to store `paneContainer` and never read it. |
| `FfiWinInfo::hidden` | Reported dead, retained deliberately for ABI stability. Worth leaving, worth knowing. |
| `regenerate_png_golden` / `tests/testdata/paint_golden.png` | Reported write-only: the file is written and never read, so it gates nothing despite the name. |

## 6. Possible defects worth a look

- **`ShortcutSeam.isNewWindowTrigger` cannot do its job.** Verified: it takes `charactersIgnoringModifiers` and `command` and never consults shift, so it returns true for both `⌘N` and `⇧⌘N`. Wiring it into `keyDown` as-is would make New and New Window indistinguishable.
- **`STATIC_COMMAND_COUNT` is a hand-maintained mirror.** `crates/wolf-core/tests/palette_rank.rs:52` hardcodes `25`, which currently matches `COMMAND_SPECS.len() - 1` (26 specs, one excluded) — so this is a latent hazard, not a live failure. Adding a command breaks two assertions until someone bumps it. Deriving it from `COMMAND_SPECS.len() - 1` removes the trap.
- **Sidebar search toggles may be seeded inconsistently.** Reported: `WolfWindowController.showSidebar(panel: .search)` focuses the field but never calls `seedToggles()`, which only the header magnifier action does. Worth confirming the intent.
- **`DividerDragSeam` takes `leadingOverhead` and `trailingOverhead` separately**, but `PaneContainerView.mouseDragged` passes the same value to both. Reported. If the asymmetry is deliberate, it needs a comment; otherwise it reads as dead generality a future refactor would collapse.
- **`crates/wolf-ffi/tests/common/mod.rs:97` hardcodes its skip banner** to `SKIP: wolf-ffi bridge_smoke`. Verified. Roughly thirty real-nvim tests share the helper, so every one of them prints the wrong test name when it skips, which makes a skipped suite hard to read.
- **`crates/wolf-gnome/testdata/fonts.conf` is a template, not a usable config.** Reported: `flake.nix` substitutes `@DEJAVU_FONTS@` and points `FONTCONFIG_FILE` at a temporary copy, so editing the committed file has no effect until the next `nix develop`.

## 7. Verification still owed

- **`bazel test //tools/lint:markdown` has never run through Bazel.** Bazel is not available on the Linux host this work was done on, so the checker was verified by running `tools/lint/markdown_check.sh` directly over all 35 in-scope docs (no findings) and by parsing every touched `BUILD.bazel`. The Bazel wiring — 16 `markdown_srcs` filegroups, the `sh_test`, and its place in `:check` — is unexercised. Run it on the macOS host before relying on it.
- **`bazel test //crates/wolf-ffi:lighthouse_load`**, per section 2.
- The `BUILD.bazel` and `extensions.bzl` comments under `bazel/*` carry the same all-caps voice the READMEs did. Not covered by any linter, since `//tools/lint:markdown` only reads Markdown.
