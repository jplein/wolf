# Implementation plan — the NixOS derivation (`linux-feat/nixos-package`)

## 0. Orchestrator status

- [x] Branch `linux-feat/nixos-package` created from `linux` in the MAIN working directory (`/home/jplein/dev/jplein/wolf`).
- [x] Architect plan (§ below, from "Verdicts" onward — adopted in full unless a ruling here overrides it)
- [ ] Tests written
- [ ] Implementation
- [ ] Failure evidence
- [ ] Review
- [ ] Docs

## 0.1 Orchestrator rulings — these override the architect where they conflict

1. **The plan is adopted in full.** The architect's verdicts on B (the "two paths, not new Lua" claim is TRUE), C (Neovim need not match Bazel's patch version), E (the drift check lives in `wolf-gnome`), F (no dark icon, no field code, no `MimeType`) and G (an additive `FONTCONFIG_FILE`) all stand.

2. **`pkgsRuntime` supplies Neovim, ripgrep AND `vimPlugins.nvim-treesitter`** — the architect's "recommended, scoped extension" in §3.1, not the minimal variant. Today these resolve to the identical store paths, so nothing observable changes; what it buys is that the treesitter parity claim stops being falsifiable by an unrelated `nixpkgs` bump. Confirm the store paths are unchanged after the swap and say so.

3. **New, and not in the architect's plan: make two existing tests stop being vacuous on Linux.** `crates/wolf-ffi/tests/orlando_load.rs` and `crates/wolf-ffi/tests/lighthouse_load.rs` already drive a real Neovim against a reconstructed sibling layout and prove the load branch — and both are "a loud no-op pass" on Linux today, because no orlando or lighthouse tree is staged there. Once `wolfOrlando` and `wolfLighthouse` exist, export `WOLF_TEST_ORLANDO_DIR` and `WOLF_TEST_LIGHTHOUSE_DIR` from `linuxShell.shellHook` pointing at them, so those two files assert on Linux instead of passing vacuously.
   - **Read both test files first** and make the derivations' output shape match exactly what each expects (`orlando_load.rs` copies `<WOLF_TEST_ORLANDO_DIR>/orlando` — an inner directory, matching the Bazel TreeArtifact — so the exported value may need to be a directory *containing* `orlando/`, not the tree itself).
   - This does **not** endanger P2's control leg: those tests reconstruct the layout in a temp dir and never place a sibling next to the checkout's `runtime/nvim/init.lua`.
   - Report the before/after: the exact line each test prints as a no-op today, and the assertions it runs afterwards.
   - If this turns out to cost more than a small fix, revert the two exports, say so plainly, and carry on — it is an addition, not the unit.

4. **P3 (the packaged preview smoke) is IN**, as the experiment that settles the `WEBKIT_EXEC_PATH` question by measurement. Follow the architect's decision rule exactly. If it cannot be made green in reasonable time, drop the smoke, do **not** add the variable on speculation, and record the gap in `tests/README.md`.

5. **The `v:progpath` red run is best-effort.** If making the two Neovims differ is not cheap, record the assertion as unfalsified in `tests/README.md`'s "What is still unproven" rather than claiming a red run nobody took.

6. **No agent may change or weaken an assertion.** The test-writer owns the assertions; if the coder believes one is wrong it reports to the orchestrator instead of editing it.

7. **Report failures faithfully.** If `nix build` cannot fetch, if `cage` will not start, if a pixel band does not discriminate — say so plainly with the command and its output. Do not smooth anything over, and do not tick a `feat-list.md` row on what a mechanism implies rather than on what was shown.

---

# Implementation plan — the NixOS derivation (`linux-feat/nixos-package`)

## Verdicts on the questions you asked me to answer

**B — the "two paths, not new Lua" claim is TRUE.** `runtime/nvim/init.lua` self-locates both trees from the `init.lua` file's own directory and gates each on an `fs_stat`:

```lua
local this_file = debug.getinfo(1, "S").source:sub(2) -- strip the leading '@'
local this_dir = vim.fn.fnamemodify(this_file, ":p:h")
local orlando_dir = this_dir .. "/orlando"
local uv = vim.uv or vim.loop
if uv and uv.fs_stat(orlando_dir) then
	vim.opt.runtimepath:prepend(orlando_dir)
	vim.cmd.runtime("plugin/orlando.lua")
end
```

and, forty lines later, `local lighthouse_dir = this_dir .. "/lighthouse"` with `prepend` + `vim.o.background = "dark"` + `vim.cmd.colorscheme("lighthouse")`. Installing `init.lua` at `$out/share/wolf/nvim/init.lua` with sibling `orlando/` and `lighthouse/` directories fires both blocks with **zero** Lua edits. `feat-list.md` needs no correction on this point.

Two things the coder must not disturb, both in the same file: the treesitter gate at line 305 probes `this_dir .. "/nvim-treesitter"` and `this_dir .. "/parsers"` **before** reading `WOLF_NVIM_TREESITTER_DIR` / `WOLF_PARSERS_DIR`. We ship no sibling for those, so the environment branch — the branch Linux already tests — stays live. Adding those two as siblings later would silently flip precedence; do not.

**C — no, the derivation's Neovim need not match Bazel's 0.12.3.** I adopt your answer. The property a packaging unit owes is *the artifact's Neovim is the one the Linux evidence was taken against*: with `pkgs.neovim` from a floating `nixpkgs`, a green Linux suite would be evidence about 0.12.2 while the shipped app ran whatever the last `nix flake update` fetched — the evidence would not be about the artifact at all. A dedicated, rev-pinned input closes exactly that. Patch parity with Bazel is a different property: it is about the *shared* `runtime/nvim/init.lua`, whose measured claims are stated per version (`vim.uv on nvim 0.12.3`, `nvim 0.12.2 starts treesitter on a markdown buffer by ITSELF`). A minor-series match preserves the API surface those claims rest on; nothing has shown a 0.12.x patch step changes any of them. Closing the gap costs either building Neovim from source on Linux — off the binary cache, a local compile taxing every `nix develop` — or bumping Linux to 0.12.4 and re-running the whole gate plus re-measuring the treesitter readbacks. Neither belongs here. Recorded in two places: a comment beside the new input in `flake.nix`, and a row in `macos-followups.md`.

**E — the drift check belongs in `wolf-gnome`,** for `release_profile.rs`'s exact reason: this crate has no `BUILD.bazel` to wire a test into, and neither `flake.nix` nor `bazel/*/extensions.bzl` is a Bazel runfile. Putting it in `wolf-core` would mean a `rust_test` plus `data` deps on three files outside that package and a Bazel-side check about a Nix-side fact; putting it in `tools/lint` would make it `manual`-tagged and advisory, so it would never run in the Linux gate. In `wolf-gnome/tests` it runs on every `cargo test`, in microseconds, with no environment.

**F — the dark icon export is not installed** because hicolor has no appearance axis: the icon-theme spec indexes by size, scale and context, not by light/dark. Both exports are full-bleed, self-contained round icons with their own background plate (`#F6F5F4` / `#242424`), so they are not a symbolic/recolourable pair either. Installing the dark set under any name would either shadow the light set or add an icon nothing ever looks up. The honest consequence — the app icon does not follow the desktop's light/dark preference, where macOS gets that from `Assets.car` — is a present-tense limitation to record, not a gap to paper over.

**F — no field code, no `MimeType`.** `app::run` calls `app.run_with_args::<&str>(&[])`, so the process ignores its real argv entirely. A `%f`/`%U` would make the desktop hand Wolf a path it silently drops, and a `MimeType=text/markdown` would put Wolf in the "Open With" list for every `.md` file and then open an empty window — a control that visibly refuses input, the shape `DECISIONS.md` rejects for the scrollbar and the drawer divider. Both stay out; opening a file from the desktop is a unit of its own (it needs `HANDLES_OPEN`, a real argv parse and an `open` handler).

**G — `<dir prefix="xdg">fonts</dir>` covers `XDG_DATA_HOME` only; `XDG_DATA_DIRS` is never scanned by fontconfig.** Verified against this host's `/etc/fonts/fonts.conf`, whose entire directory list is:

```xml
<include ignore_missing="yes">/etc/fonts/conf.d</include>
<dir prefix="xdg">fonts</dir>
<dir>/nix/store/ang6yzsv32vnkdq7bqr41dgna2knkz8w-dejavu-fonts-minimal-2.37</dir>
```

So `--prefix XDG_DATA_DIRS : $out/share` will not make bundled faces resolvable, and fontconfig's whole environment surface is `FONTCONFIG_FILE`, `FONTCONFIG_PATH`, `FONTCONFIG_SYSROOT` — none of them additive. The mechanism specified below is an **additive** `FONTCONFIG_FILE` that `<include>`s the system config and appends the app's own font directory, which is a different thing from the devShell's *exclusionary* pin and does not inherit its objection.

---

## 1. Summary

Add a `packages.<linux-system>.{wolf-gnome,default}` output to `flake.nix` that builds `wolf-gnome` with the workspace release profile, wraps it with `wrapGAppsHook4`, and installs a self-contained Neovim runtime tree at `$out/share/wolf/nvim/` (a copy of `runtime/nvim/init.lua` plus sibling `orlando/` and `lighthouse/` trees pinned to the same commits and hashes Bazel pins), an `org.wolf.Wolf.desktop` entry, the GNOME light hicolor icon set, and the two vendored editor font families. Make the app able to find that tree by replacing `app.rs::init_lua_path`'s compile-time-only resolution with a three-branch resolver in a new pure seam, `init_lua.rs`. Pin Neovim, its treesitter trees and ripgrep to a second, rev-locked `nixpkgs` input so a `nix flake update nixpkgs` for GTK cannot move what the Linux suite tested. Add a Cargo-only drift check tying the three Nix vendored pins to the Bazel ones, and produce evidence about the **installed store artifact** rather than the checkout build, through ignored compositor smokes plus a store-layout test that one committed script always runs with the store path set.

---

## 2. Phase 1 — the `init.lua` seam (Rust, no packaging yet)

### 2.1 Create `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/src/init_lua.rs`

A pure seam. GTK-free, environment-free, filesystem-free — everything injected, mirroring `config_seam.rs`'s discipline and `palette::open_recent_candidates`' precedent for taking a predicate as an argument.

Public items:

- `pub const INIT_LUA_ENV: &str = "WOLF_INIT_LUA";` — the development override's name, so `app.rs` and the docs read one spelling.
- `pub const INSTALLED_INIT_LUA_RELATIVE: &str = "../share/wolf/nvim/init.lua";` — the executable-relative install path. This constant is the **contract between the Rust resolver and the Nix install layout**, and `packaged_layout.rs` reads it rather than retyping the path, so the two cannot drift silently.
- `pub fn resolve_init_lua(env_override: Option<&str>, exe_dir: Option<&Path>, manifest_fallback: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf`

Behaviour, in order, total, never panicking:

1. `env_override` — if `Some(s)` and `s.trim()` is non-empty, return `PathBuf::from(s)` **without checking that it exists**. macOS's `Project.swift::initLuaPath()` does not check either, and the reason is not merely parity: an override the user typed is an instruction, not a candidate. Existence-checking it would make a typo fall silently through to the installed file, so the app would look fine while ignoring the override — and the failure that replaces it is loud (Neovim launched with a missing `-u` never attaches, core's start fails, and the app prints `failed to start core` and opens no window). Loud over silent is this repo's rule.
2. `exe_dir` — if `Some(dir)`, form `dir.join(INSTALLED_INIT_LUA_RELATIVE)` and return it **only if `exists(&candidate)`**. This branch *is* a guess the binary makes about its own surroundings, so it must fall through in a `cargo run` build where no such tree exists. That asymmetry with branch 1 is the point and belongs in the doc comment.
3. Otherwise return `manifest_fallback.to_path_buf()` unchanged.

Do **not** canonicalize or normalize the joined path. `<store>/bin/../share/wolf/nvim/init.lua` is handed to Neovim as `-u`; `debug.getinfo(1,"S").source` reports exactly that string and `fnamemodify(..., ":p:h")` yields `<store>/bin/../share/wolf/nvim`, whose `/orlando` and `/lighthouse` children resolve correctly through the kernel. Normalizing is harmless but unnecessary; state that it is not required so nobody adds a fallible `canonicalize` on a startup path.

Add a `#[cfg(test)] mod tests` covering the table in §6.

### 2.2 Modify `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/src/lib.rs`

- Add `pub mod init_lua;` in alphabetical position (between `input` and `layout`).
- Add a bullet to the `//!` seam inventory naming `mod@init_lua` — the captive `init.lua`'s three-branch resolution as a function of values, with the environment read and the `current_exe` read done by `app.rs`.

### 2.3 Modify `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/src/app.rs`

Rewrite `init_lua_path` (line 133-137) as glue only:

- Read `std::env::var(init_lua::INIT_LUA_ENV).ok()`.
- Read `std::env::current_exe().ok()` and take `.parent()` from it. Note in the doc comment, so nobody "fixes" it later: under `wrapGAppsHook4` the installed `$out/bin/wolf-gnome` is a wrapper that `exec`s `$out/bin/.wolf-gnome-wrapped`, and `current_exe()` reads `/proc/self/exe`, so the parent is still `$out/bin` and the `../share/...` join still resolves. A wrapper does not break this branch.
- Pass `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/nvim/init.lua")` as `manifest_fallback` — unchanged, and it is what keeps `cargo run` and every existing smoke working.
- Pass `|p: &Path| p.exists()` as the predicate.
- Keep the function `pub(crate)` and `#[must_use]`; `window.rs:419` and the coordinator call sites are untouched.

Keep `FONT_PT_ENV`'s neighbourhood convention: the environment read lives here, the rule lives in the seam.

---

## 3. Phase 2 — the flake

All edits are to `/home/jplein/dev/jplein/wolf/flake.nix`. The derivation lives **inline**, per your lean: it shares `wolfVendoredPins` and `pkgsRuntime` with `linuxShell`, matches `wolfPointerInject` / `iaWriterMono` precedent, and a separate directory would oblige a new `README.md` under CLAUDE.md's rule for a single derivation that has exactly one consumer.

### 3.1 The second input

```
inputs.nixpkgs-runtime.url = "github:NixOS/nixpkgs/9ae611a455b90cf061d8f332b977e387bda8e1ca";
```

Same rev the main `nixpkgs` input is locked at today, so nothing about the currently-tested Neovim changes and the closure is shared (zero extra download today). Add it to the `outputs` argument list. Bind `pkgsRuntime = import nixpkgs-runtime { inherit system; };` inside the existing `let`.

**Comment beside it must state**, in prose: what it pins and why (the runtime assets the shipped application carries — Neovim, its treesitter trees, ripgrep — held apart from the GTK/toolchain nixpkgs so a `nix flake update nixpkgs` cannot move what the Linux suite tested); the version it carries (Neovim 0.12.2); the Bazel pin it diverges from (0.12.3, `bazel/nvim/extensions.bzl`); that the divergence predates this unit; and the two-sentence version of the §C verdict — same minor series is what the shared `init.lua`'s per-version claims need, and closing the patch gap means either an off-cache source build or a full Linux re-gate.

**Recommended, scoped extension:** take `neovim`, `ripgrep` **and** `vimPlugins.nvim-treesitter` (both the plugin and its `.parsers.<lang>` attributes) from `pkgsRuntime`. Today they are identical store paths, so this changes nothing observable and carries no risk; what it buys is that the `wolfTreesitterQueries` comment's parity claim — "the same upstream revision `bazel/nvim-treesitter/extensions.bzl` pins for macOS (4916d659…), so the two platforms get byte-identical queries" — stops being falsifiable by an unrelated `nixpkgs` bump. If the orchestrator wants minimal scope, take Neovim alone and record the other two as an open drift in `macos-followups.md`; say which was chosen.

**Darwin laziness:** `pkgsRuntime` and everything derived from it must be referenced only from `linuxShell` and the package. Extend the existing `NOTE:` block (flake.nix lines 48-62) to name `pkgsRuntime`, `wolfOrlando`, `wolfLighthouse` and `wolfGnomePackage` alongside the others, and to state the *one* deliberate exception below.

### 3.2 `wolfVendoredPins` — one place for the three pins

Add, near the top of the `let`, ahead of `iaWriterMono`:

```nix
wolfVendoredPins = {
  ia-writer-mono = { commit = "f32c04c3058a75d7ce28919ce70fe8800817491b"; sha256 = "826767eaea4ce43ccb7ca71327a5c63ac31206b99df400699e6744df6104e1b5"; };
  orlando = { commit = "c960cf3e1c24fbdd2e6fb30ab9c8022cdc7b2bf9"; sha256 = "794911361108eb509ab00ec64ed03c229418be32f45ad7a385b6d2d6b057f3c5"; };
  lighthouse = { commit = "514dec2c79701b74082defc19090bd1dae57765b"; sha256 = "a452624fee9ce7b82e11877b013f578aa1c3befda6b6564536c3264a40b8e0d2"; };
};
```

Formatting is load-bearing and must be stated in the comment above it: **one attribute per line, `commit` then `sha256`, both double-quoted, closing `};` on the same line**, because `crates/wolf-gnome/tests/vendored_pins.rs` is a line scanner and not a Nix parser — the same contract the workspace `Cargo.toml` carries for `release_profile.rs`. Also carry the "BUMP BOTH TOGETHER" note that `iaWriterMono` has today, generalized to all three, plus a pointer to the drift check.

Refactor `iaWriterMono` to read `wolfVendoredPins.ia-writer-mono.commit` / `.sha256` instead of its local `commit` binding and inline hash. Its existing prose comment stays but loses the sentences the new block now owns.

### 3.3 `wolfOrlando` and `wolfLighthouse`

Two `pkgs.stdenv.mkDerivation`s shaped exactly like `iaWriterMono` — `fetchurl` from the same GitHub tarball URL with the same hex sha256, `sourceRoot` set to the archive root, `dontConfigure`/`dontBuild`, an explicit `installPhase`.

`wolfOrlando`:
- `pname = "wolf-orlando"`, `version = "0-unstable-2025"` (the pin is a commit, not a release — mirror `iaWriterMono`'s comment).
- `url = "https://github.com/jplein/orlando/archive/${commit}.tar.gz"`.
- `sourceRoot = "orlando-${commit}"`.
- Install exactly the subtrees `//bazel/orlando`'s `runtime_files` filegroup keeps, and nothing else: `plugin/`, `lua/`, `doc/`, `LICENSE*` → `$out/plugin`, `$out/lua`, `$out/doc`, `$out/LICENSE*`. `test/`, `flake.nix`, `flake.lock`, `.luarc.json` and `README.md` are deliberately excluded — a comment must say the exclusion list mirrors the Bazel filegroup so the two platforms ship the same files.
- The output root **is** the tree that becomes the `orlando/` sibling (i.e. `$out/plugin/orlando.lua`, not `$out/orlando/plugin/orlando.lua`). Bazel needs the extra inner directory because `rules_apple` strips a TreeArtifact's top-level name; Nix has no such rule and the `postInstall` below names the destination explicitly. State that difference in the comment, because a reader comparing the two will otherwise think one is wrong.

`wolfLighthouse`:
- `pname = "wolf-lighthouse"`.
- `url = "https://github.com/jplein/lighthouse.nvim/archive/${commit}.tar.gz"`.
- **`sourceRoot = "lighthouse.nvim-${commit}"`** — the upstream repo name includes `.nvim`; getting this wrong makes the install phase find nothing. Carry the Bazel file's own warning.
- Install `colors/` and `LICENSE*` only. No `plugin/`, no `lua/`, no `doc/` — a colorscheme has none.

Neither goes into `linuxShell.packages`, and that is not an oversight: **the devShell must keep running the smokes without orlando and lighthouse**, because the checkout binary with no sibling trees is the control leg of the packaged-artifact A/B in §6.4. Say so in the comment.

### 3.4 `wolfGnomePackage`

```nix
wolfGnomePackage = (pkgs.makeRustPlatform { cargo = rustToolchain; rustc = rustToolchain; }).buildRustPackage { … };
```

Using the flake's own pinned toolchain rather than `rustPlatform`'s nixpkgs default, so the artifact is built with the 1.96.0 the Linux evidence was taken with.

- `pname = "wolf-gnome"`, `version = "0.1.0"` (the workspace `[workspace.package] version`).
- `src` — a filtered source: keep `Cargo.toml`, `Cargo.lock` and `crates/`; drop `target/`, `.direnv/`, `result*`, `.git/`, `resources/`, `runtime/`, `tools/`, `bazel/`, `*.md`. Use `lib.cleanSourceWith` over `lib.cleanSource ./.` with an explicit `filter`. **`lib.cleanSource` alone does not exclude `target/`**, which on this host is over a gigabyte; a missing filter turns every `nix build` into a multi-GB copy into the store. Say so.
- `cargoLock.lockFile = ./Cargo.lock;` — no `outputHashes` needed; verified there are zero `source = "git+…"` entries in `Cargo.lock`.
- `cargoBuildFlags = [ "--package" "wolf-gnome" "--bin" "wolf-gnome" ];` so `wolf-ffi` (the swift-bridge crate, which the shipped Linux app does not use) is not built.
- `buildType` is left at its default `"release"`, which is what selects the workspace `[profile.release]` — `opt-level = 3`, `lto = "thin"`, `panic = "unwind"`, `debug = "line-tables-only"`, `strip = "none"`. State that no extra flag is needed and that `strip` must not be overridden by `dontStrip = false` sloppiness, since the release contract is that symbols survive.
- `nativeBuildInputs = [ pkgs.pkg-config pkgs.wrapGAppsHook4 pkgs.desktop-file-utils ];`
- `buildInputs = [ pkgs.gtk4 pkgs.libadwaita pkgs.webkitgtk_6_0 pkgs.libsoup_3 pkgs.cairo pkgs.pango pkgs.glib pkgs.gdk-pixbuf pkgs.graphene pkgs.harfbuzz pkgs.librsvg pkgs.gsettings-desktop-schemas pkgs.adwaita-icon-theme pkgs.hicolor-icon-theme pkgs.shared-mime-info ];`
  - The first eight are the `.pc` files the gtk-rs and `webkit6` `-sys` crates probe, taken straight from `linuxShell`'s list, plus `libsoup_3` for the reason that list already records (`webkitgtk-6.0.pc` declares `Requires: … libsoup-3.0`, so `soup3-sys` does not configure without it).
  - The last five are runtime, and they are in `buildInputs` rather than anywhere else because that is how `wrapGAppsHook4` and the `glib` / `hicolor-icon-theme` setup hooks discover them: they accumulate `GSETTINGS_SCHEMAS_PATH` and `XDG_ICON_DIRS` from `buildInputs` and write the corresponding `--prefix` flags into the wrapper. `librsvg` is what puts the SVG loader in the pixbuf module cache, without which the scalable icon and Adwaita's own SVGs do not decode.
  - `wolf-core`'s only C dependency is `rusqlite`'s bundled SQLite, compiled by `cc` from stdenv — no system `sqlite` is needed and none must be added.
- **`doCheck = false;`**, with the reason spelled out in the comment: the suite `cargo test -p wolf-gnome` runs is not a headless unit suite. Fifty-six of its tests launch `cage`, a wlroots compositor needing a writable `XDG_RUNTIME_DIR` at mode 0700, alongside `wtype`, `grim`, `tesseract` and this repo's own `wolf-pointer-inject`; every real-nvim test additionally needs `WOLF_NVIM_BIN`; and `wolf-core`'s swap-recovery test needs a writable `$HOME`. A Nix build sandbox provides no `/run/user/<uid>`, no `/dev/dri` (so GTK's GL renderer has no device and would have to fall back), and no session bus. Running a curated subset would execute only the pure seams the checkout `cargo test` already covers, adding nothing about the artifact. **The consequence is stated, not hidden: all evidence about the packaged artifact comes from outside the build sandbox** — §6.4 and §6.5.
- `postInstall` — see §3.5.
- `preFixup` — see §3.6.
- `installCheckPhase` (or a `postInstall` tail): run `desktop-file-validate "$out/share/applications/org.wolf.Wolf.desktop"`. Cheap, and it catches a malformed entry at build time rather than at a user's session.
- `meta` — `description`, `homepage`, `license = lib.licenses.mit` (the workspace is `MIT OR Apache-2.0`; use `[ mit asl20 ]`), `mainProgram = "wolf-gnome"`, `platforms = lib.platforms.linux`.

### 3.5 `postInstall` — the install layout

Everything below is written with `install -Dm644` / `install -Dm755` (never a bare `cp` from the store, whose read-only mode would carry over).

**The Neovim runtime tree.** Note the Nix path-literal gotcha: `./resources/icons/Wolf application icon GNOME light/...` is **not** a valid path literal, because a space terminates one. Use path-plus-string concatenation, e.g. `gnomeIconDir = ./resources/icons + "/Wolf application icon GNOME light";` and then `gnomeIconDir + "/icon-silhouette_48x48.png"`. Reference the four PNGs and the SVG individually rather than the directory, so the macOS masters in the sibling directories are not dragged into the store.

- `$out/share/wolf/nvim/init.lua` ← `${./runtime/nvim/init.lua}`, a real copy (mode 644).
- `$out/share/wolf/nvim/orlando/` ← recursive copy of `${wolfOrlando}` (`plugin/`, `lua/`, `doc/`, `LICENSE*`).
- `$out/share/wolf/nvim/lighthouse/` ← recursive copy of `${wolfLighthouse}` (`colors/`, `LICENSE*`).

**The desktop entry**, written with a heredoc so `$out` is available:

```
$out/share/applications/org.wolf.Wolf.desktop
```

Keys, exactly: `[Desktop Entry]`, `Type=Application`, `Version=1.0`, `Name=Wolf`, `GenericName=Markdown Editor`, `Comment=A Markdown editor that embeds Neovim as its editing engine`, `Exec=$out/bin/wolf-gnome`, `Icon=org.wolf.Wolf`, `Terminal=false`, `Categories=Utility;TextEditor;Development;`, `StartupNotify=true`, `StartupWMClass=org.wolf.Wolf`.

`Exec` is the **absolute store path**, not a bare `wolf-gnome`, and the comment must give the reason: the entry and the binary are outputs of the same derivation, so the path cannot dangle while the entry is visible, and an absolute path removes any dependence on `PATH` ordering. A bare name would be resolved by the session's `PATH`, where a developer's `target/release` directory ahead of the profile would launch an **unwrapped** binary carrying no `WOLF_NVIM_BIN` and no `init.lua` — a silent wrong-artifact launch from the application menu, which is exactly the failure class this repo spends its guards on. `StartupWMClass` matches `APP_ID` so the shell associates the window with the entry.

No `%f`, no `%U`, no `MimeType` — §F above.

**The icons.** From `Wolf application icon GNOME light/`:

| source | destination |
| --- | --- |
| `icon-silhouette_48x48.png` | `$out/share/icons/hicolor/48x48/apps/org.wolf.Wolf.png` |
| `icon-silhouette_64x64.png` | `$out/share/icons/hicolor/64x64/apps/org.wolf.Wolf.png` |
| `icon-silhouette_128x128.png` | `$out/share/icons/hicolor/128x128/apps/org.wolf.Wolf.png` |
| `icon-silhouette_256x256.png` | `$out/share/icons/hicolor/256x256/apps/org.wolf.Wolf.png` |
| `icon-silhouette_scalable.svg` | `$out/share/icons/hicolor/scalable/apps/org.wolf.Wolf.svg` |

**The fonts.** Copy the four iA Writer Mono S faces from `${iaWriterMono}/share/fonts/truetype/` and the JetBrains Mono faces from `${pkgs.jetbrains-mono}/share/fonts/truetype/` into `$out/share/fonts/truetype/`, plus `${iaWriterMono}/share/licenses/ia-writer-mono/LICENSE.md` into `$out/share/licenses/`. The two file sets are disjoint, which is the same argument `crates/wolf-macos/README.md` makes for its single bundled `fonts/` directory. This also means a NixOS user can put the package in `fonts.packages` and get the families system-wide.

**The fontconfig fragment**, written with a heredoc into `$out/share/wolf/fonts.conf`:

```xml
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
  <dir>OUT/share/fonts</dir>
</fontconfig>
```

The comment above it must state three things, because every one of them is a thing a later reader would undo. (1) It is **additive**, not the devShell's exclusionary pin: the include is what keeps the host's own fonts, aliases and rendering settings — the devShell's `testdata/fonts.conf` omits every host directory deliberately, for golden-render determinism, and shipping that to a user would leave the GTK chrome rendering in a monospace editor face. (2) `FONTCONFIG_FILE` replaces the *root* config rather than extending it, which is why the include is written out explicitly and why `--prefix XDG_DATA_DIRS` cannot substitute for any of this — fontconfig's `<dir prefix="xdg">fonts</dir>` covers `XDG_DATA_HOME` alone, verified against `/etc/fonts/fonts.conf` on this host. (3) The `<dir>` comes after the include so system fonts win ties.

### 3.6 `preFixup` — the wrapper environment

`wrapGAppsHook4` wraps during `fixupPhase` and consumes the `gappsWrapperArgs` array, so the flags are appended in `preFixup` rather than through a second `wrapProgram` call — a hand-rolled wrap here would double-wrap and shadow the hook's own `--prefix XDG_DATA_DIRS` / `GSETTINGS_SCHEMA_DIR` / `GIO_EXTRA_MODULES` / `GDK_PIXBUF_MODULE_FILE`.

```
gappsWrapperArgs+=(
  --set-default WOLF_NVIM_BIN "${lib.getExe wolfNeovim}"
  --set-default WOLF_RG_BIN "${lib.getExe wolfRipgrep}"
  --set-default WOLF_NVIM_TREESITTER_DIR "${wolfTreesitterQueries}"
  --set-default WOLF_PARSERS_DIR "${wolfTreesitterParsers}"
  --set-default FONTCONFIG_FILE "$out/share/wolf/fonts.conf"
)
```

`--set-default` for every one, and the reason must be written down per group:

- `WOLF_NVIM_BIN` / `WOLF_RG_BIN` — direct macOS parity. `Project.swift:83,88` use `setenv(name, value, 0)`, overwrite `0`, so a developer's export wins there; `--set-default` is the same rule. Reading `crates/wolf-macos/Sources/WolfMacOS/README.md` will confirm it in one line.
- `WOLF_NVIM_TREESITTER_DIR` / `WOLF_PARSERS_DIR` — same rule for consistency, and because a developer testing a work-in-progress query tree must be able to point the packaged binary at it. The values are the same store paths the devShell exports today, so inside `nix develop` the two agree.
- `FONTCONFIG_FILE` — `--set-default` is what keeps the devShell's exclusionary pin winning inside `nix develop`, which is what keeps `paint_golden.rs` and every OCR smoke reproducible; outside the shell the packaged additive config applies.
- `WOLF_INIT_LUA` is **never** set by the wrapper. Setting it would make the executable-relative branch dead code and would make the development override unusable on the packaged binary. Write that down beside the list.
- `WEBKIT_EXEC_PATH` is **not** set, pending §6.4's preview leg — see Open Questions.

### 3.7 The outputs attrset

```nix
{
  devShells.default = if isDarwin then darwinShell else linuxShell;
}
// pkgs.lib.optionalAttrs (!isDarwin) {
  packages.wolf-gnome = wolfGnomePackage;
  packages.default = wolfGnomePackage;
}
```

`optionalAttrs` forces only its condition, not the value, so the Darwin eval path never touches `wolfGnomePackage`. This guard **is** required, and it is a different thing from the `lib.optional` guard the existing `NOTE:` block forbids: that prohibition is about guarding a Linux derivation *inside* `linuxShell.packages`, where a guard would imply the laziness argument is unsound. Here the guard is on the *output attribute set* — without it `packages.aarch64-darwin.default` would be a Wayland GTK application that cannot build on Darwin at all and that `nix flake show` and `nix flake check` would try to evaluate. Add a comment making that distinction explicit, next to the existing `NOTE:`.

### 3.8 Also modify

- `linuxShell.packages`: `pkgs.neovim` → `wolfNeovim`, `pkgs.ripgrep` → `wolfRipgrep`.
- `linuxShell.shellHook`: the two `getExe` interpolations follow.
- `wolfTreesitterParsers` / `wolfTreesitterQueries`: source `nvim-treesitter` from `pkgsRuntime` if the recommended extension is adopted; update the two comment blocks to say the revision is now pinned by the runtime input rather than tracking `nixpkgs`.
- The file-header comment (lines 1-20) gains a sentence naming the package output.

---

## 4. Phase 3 — the drift check

### 4.1 Create `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/tests/vendored_pins.rs`

Not `#[ignore]`d. Pure: no environment, no subprocess, no compositor, microseconds. Runs on every `cargo test`.

**Paths.** All three rooted at `Path::new(env!("CARGO_MANIFEST_DIR")).join("../../…")`, exactly as `release_profile.rs::workspace_manifest_path` does: `flake.nix`, `bazel/orlando/extensions.bzl`, `bazel/lighthouse/extensions.bzl`, `bazel/ia-writer-mono/extensions.bzl`. A read failure panics naming the path and saying that an error here means this crate moved relative to the workspace root, not that a pin drifted.

**Bazel-side extraction rule, stated exactly so nobody invents a parser.** For a given `.bzl` text and a key `K` in `{"_COMMIT", "_SHA256"}`: consider every line whose `trim_start()` begins with `K`, with the remainder after `K` matching `\s*=\s*"` — collect the double-quoted contents. Assert **exactly one** match per key per file; zero means the file was reworded and the check has gone vacuous (fail with that word in the message, the `cage_is_only_spawned_through_the_isolating_helper` precedent), more than one means the file grew a second pin and the check no longer knows which is authoritative. Verified against all three files today: each has exactly one `_COMMIT = "` line and one `_SHA256 = "` line, and the prose mentions of `` `_COMMIT` `` in docstrings carry backticks and no `= "`, so they cannot match.

**Nix-side extraction rule.** Locate the single line whose trimmed form is `wolfVendoredPins = {`; take lines until the first subsequent line whose trimmed form is `};`; for each line between, match `^\s*(?P<name>[a-z-]+)\s*=\s*\{\s*commit\s*=\s*"(?P<commit>[0-9a-f]{40})"\s*;\s*sha256\s*=\s*"(?P<sha>[0-9a-f]{64})"\s*;\s*\};?\s*$` (do this with `str` splitting rather than a regex crate — this crate takes no new dependency for a test, the reason `release_profile.rs` records). Assert the block exists, that it holds **exactly three** entries, and that the three names are exactly `ia-writer-mono`, `orlando`, `lighthouse` — an extra unmatched entry means a fourth asset was pinned on the Nix side with no Bazel counterpart, which is itself the drift this check exists for.

**Assertions.** Three cases, one per asset, each asserting commit equality and sha256 equality. Assert in a way that a broken second row is still reported: collect mismatches into a `Vec` and assert the vec is empty at the end, rather than a `for` loop of `assert_eq!` — `crates/wolf-gnome/tests/README.md` records that a loop with an assertion inside produced one observed failure where a revert broke two rows.

**The failure message must name**, per mismatch: the asset; the Nix value and the Bazel value side by side, labelled with which file each came from; both absolute-relative paths; the fact that a one-sided bump is **silent**, because each side fetches successfully against its own sha256 and neither reads the other's — so Linux would ship one revision of a plugin while macOS bundled another with nothing failing anywhere; and the instruction to bump both together rather than "fixing" the test.

**Do not** edit `bazel/orlando/extensions.bzl` or `bazel/lighthouse/extensions.bzl` to add the reciprocal "BUMP BOTH TOGETHER" docstring that `ia-writer-mono` carries. That is a macOS-side edit and the constraint forbids it; log the obligation in `macos-followups.md` instead.

---

## 5. Phase 4 — the packaging evidence

This is the part that decides whether the unit is honest. A green `cargo test` on the checkout says nothing about `$out/bin/wolf-gnome`; they are different artifacts and different claims.

### 5.1 Shape chosen, and why the other two lose

I considered your three shapes and choose **(ii) plus (iii) together**: a committed script that always sets the precondition, driving `#[ignore]`d Cargo tests that carry the assertions.

- **(i), a `checks.<system>` derivation, is rejected** and I want to be plain that it is rejected on lack of confidence rather than on taste. The scenario needs `cage` on wlroots headless, which needs a writable `XDG_RUNTIME_DIR` at mode 0700 (arrangeable under `$TMPDIR`) but also a working GTK render path: the sandbox has no `/dev/dri`, so GSK's GL renderer has no device and the run depends on an untested cairo-renderer fallback. It has no `/dev/shm`, no session bus and no user-namespace guarantees for WebKit's mandatory bubblewrap sandbox. Any one of those turns the unit's evidence into a debugging project inside a build sandbox where the only instrument is a build log. I am not confident it works, so it is not where the evidence goes.
- **(iii) alone is rejected** for the reason you name: an env-gated test either skips quietly, which `crates/wolf-gnome/tests/README.md` and CLAUDE.md both forbid, or fails for everyone who has not run `nix build`. `#[ignore]` resolves that cleanly and has precedent in this crate (`paint_golden.rs`'s `regenerate_png_golden`): an ignored test is *not part of the default suite*, which is different from *silently skipped*, and the precondition inside it still fails loud. What makes it honest is that exactly one command runs it and that command always sets the variable.

### 5.2 Where the compositor smokes live — inside `smoke_headless.rs`, not a new file

This is not a stylistic call. A second test binary that spawns `cage` can run **concurrently** with `smoke_headless` under `cargo test -p wolf-gnome`, and `CAGE_LOCK` is a per-process mutex that cannot serialize across processes; the tests README already records the signature (`unable to lock lockfile … maybe another compositor is running`, plus a hang that reads as a mass regression). A second file would also need its own `Command::new("cage")` — a second spawn site outside `cage_command`, carrying none of its five isolations, which is precisely what `cage_is_only_spawned_through_the_isolating_helper` exists to prevent, and that guard reads only its own file's source so it would not catch it. Putting the packaged smokes in `smoke_headless.rs` reuses `CAGE_LOCK`, `cage_command`, `decode_capture`, `Capture`, `Rgb8`, `pixel_index`, `band_fraction`, the `ocr_*` helpers, `cell_metrics_for_smoke` and the settle constants, and keeps the single-spawn-site invariant intact.

The store-**layout** assertions need no compositor and go in a small new file of their own.

### 5.3 New file: `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/tests/packaged_layout.rs`

Every test `#[ignore]`d. One shared helper `packaged_out() -> PathBuf` reading **one** environment variable, `WOLF_PACKAGE_OUT` (the store path root; the binary is derived as `<out>/bin/wolf-gnome`). It panics — loud, never a skip — when unset, empty, not a directory, or missing `bin/wolf-gnome`, and the message names `./tools/wolf-package-smoke.sh` as the command that sets it and `nix build .#wolf-gnome` as what produces it.

Tests, each named for its claim:

1. `the_packaged_output_carries_the_captive_init_lua_at_the_resolvers_own_path` — join `<out>/bin` with `wolf_gnome::init_lua::INSTALLED_INIT_LUA_RELATIVE` (**read the constant, do not retype the path**) and assert the file exists and is **byte-identical** to `runtime/nvim/init.lua` in the checkout. This is the join that makes the Rust resolver and the Nix install layout one contract rather than two.
2. `the_packaged_output_carries_the_two_vendored_lua_trees_as_siblings` — `<nvim dir>/orlando/plugin/orlando.lua`, `<nvim dir>/orlando/lua/orlando/init.lua`, a `LICENSE*` under `orlando/`, `<nvim dir>/lighthouse/colors/lighthouse.lua`, a `LICENSE*` under `lighthouse/`. Also assert the exclusions actually took: no `orlando/test`, no `orlando/flake.nix`, no `orlando/README.md`, no `lighthouse/flake.nix`.
3. `the_packaged_desktop_entry_launches_the_wrapped_binary_and_claims_no_files` — parse `share/applications/org.wolf.Wolf.desktop` as `key=value` lines under `[Desktop Entry]`; assert `Type=Application`, `Icon=org.wolf.Wolf`, `Terminal=false`, `Exec` equal to `<out>/bin/wolf-gnome` exactly, `Exec` containing no `%`, and **no `MimeType` key at all**. The last two are the ones that would rot: a later "helpful" edit adding `%U` would make the launcher hand Wolf a path `run_with_args::<&str>(&[])` discards.
4. `the_packaged_icons_are_installed_at_the_sizes_their_directories_name` — for each of 48/64/128/256, the file exists and decodes with `cairo::ImageSurface::create_from_png` (already a dependency, for the `png` feature the appearance smoke needs) to exactly that width and height. That catches the real defect — the 48 px export installed into the 256 directory — which an existence check cannot see. Plus the scalable SVG exists, is non-empty, and its first bytes are `<?xml` or `<svg`.
5. `the_packaged_wrapper_carries_every_variable_the_running_app_needs` — read `<out>/bin/wolf-gnome` as **bytes** and assert each of `WOLF_NVIM_BIN`, `WOLF_RG_BIN`, `WOLF_NVIM_TREESITTER_DIR`, `WOLF_PARSERS_DIR`, `FONTCONFIG_FILE`, `XDG_DATA_DIRS`, `GSETTINGS_SCHEMA_DIR` appears as a substring. Bytes and not `read_to_string`, because `wrapGAppsHook4` may produce either a shell wrapper or a `makeBinaryWrapper` binary, and the names are present either way; the test must not encode which. Also assert `WOLF_INIT_LUA` does **not** appear — setting it would make the exe-relative branch dead code.
6. `the_packaged_output_carries_both_editor_font_families` — the four `iAWriterMonoS-*.ttf` and at least one `JetBrainsMono*` under `share/fonts/`, plus `share/wolf/fonts.conf` existing and containing both `<include` and the `<dir>` naming `<out>/share/fonts`.

### 5.4 New tests inside `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/tests/smoke_headless.rs`

**Shared helper** `packaged_cage_command(scenario, cwd) -> Command`: calls `cage_command(scenario, cwd)` — so the five isolations and the `DBUS_SESSION_BUS_ADDRESS` removal all still apply — then chains `.env_remove(...)` for every variable `flake.nix`'s `shellHook` exports that the packaged artifact must supply for itself: `WOLF_NVIM_BIN`, `WOLF_RG_BIN`, `WOLF_NVIM_TREESITTER_DIR`, `WOLF_PARSERS_DIR`, `WOLF_INIT_LUA`, `FONTCONFIG_FILE`, `RUST_SRC_PATH`, and — for the clean-launch smoke only — `XDG_DATA_DIRS` and `GSETTINGS_SCHEMA_DIR`. `XDG_RUNTIME_DIR`, `WLR_BACKENDS` and `WLR_LIBINPUT_NO_DEVICES` are **kept**: they belong to `cage`, not to the application under test.

**Smoke P1 — `packaged_binary_renders_typed_text_under_cage`** (`#[ignore]`). Reuse `SCENARIO` verbatim with `WOLF_BIN` = `<out>/bin/wolf-gnome` and the **full** scrub including the two GTK-runtime variables. Assert cage exit 0, the capture exists, and OCR contains `OCR_WORD`. What it proves: the store binary launches under a compositor carrying none of the devShell's exports; finds its Neovim through the wrapper's `WOLF_NVIM_BIN`; finds its `init.lua` through the executable-relative branch; gets its GSettings schemas, icon theme and mime data from `wrapGAppsHook4` rather than from `shellHook`; and draws glyphs a human could read with the packaged fontconfig rather than the devShell's pin.

**Smoke P2 — `the_packaged_binary_loads_the_vendored_lighthouse_and_orlando_under_cage`** (`#[ignore]`). Two launches in **one** `cage`, on the `APPEARANCE_SCENARIO` / `PREVIEW_THEME_SCENARIO` shape — a shell function invoked twice with different parameters, never two copies of the leg. Launch A is the **control**: `CARGO_BIN_EXE_wolf-gnome`, the checkout binary, which has no sibling `orlando/` or `lighthouse/` at all. Launch B is `<out>/bin/wolf-gnome`. This smoke keeps `XDG_DATA_DIRS` / `GSETTINGS_SCHEMA_DIR` in the environment, because launch A's unwrapped binary needs them to reach `activate` — say so in the doc comment, and say that P1 is where the wrapped-GTK-runtime claim is made. `WOLF_NVIM_BIN` is removed at the `Command` level and re-exported **inside the scenario for launch A only**, from a scenario variable passed under a different name (`CONTROL_NVIM_BIN`), so the packaged launch demonstrably never sees it.

Each launch types one `:lua vim.fn.writefile({…}, "<readback>")` — the same instrument `crates/wolf-ffi/tests/orlando_load.rs` and `lighthouse_load.rs` use — writing these lines:

1. `vim.g.colors_name == "lighthouse"` → `OK`/`NO`
2. `vim.g.loaded_orlando` truthy → `OK`/`NO`
3. `pcall(require, "orlando")` → `OK`/`NO`
4. `string.format("%06x", vim.api.nvim_get_hl(0, {name="Normal"}).bg or 0)`
5. `vim.v.progpath`
6. `vim.o.background`

Assertions:

- Lines 1-3 are `OK` for launch B and `NO` for launch A. The A side is what makes the B side mean something.
- Line 4 is `1b1b1b` for launch B — lighthouse's dark `Normal` background at the pinned commit — and something else for launch A.
- **Line 5 is equal between the two launches.** This is property (C) measured rather than argued: launch A's captive Neovim came from the devShell's `WOLF_NVIM_BIN`, launch B's came from the packaged wrapper, and `v:progpath` reports the resolved executable in both. Equality says the package ships exactly the Neovim the Linux suite tests against. Comparing against the test process's own `WOLF_NVIM_BIN` string would **not** work and must not be substituted: `pkgs.neovim` is a wrapper and `v:progpath` resolves through `/proc/self/exe` to the unwrapped store path, so the two strings legitimately differ. Two `v:progpath` readings are comparable; a `v:progpath` and a `getExe` are not.
- Line 6 is `dark` in both — the seeded `SMOKE_APPEARANCE`.

Plus a `grim` capture per launch and two pixel bands, decoded with the existing `decode_capture`:

- **The lighthouse band.** A rectangle inside the pane area, below the fixture's text and derived from `OVERLAY_ORIGIN_*_EXPECTED` / `CAGE_OUTPUT_*_EXPECTED` in the manner `preview_theme_pane_area()` already establishes — never hardcoded. Assert a high fraction of it is exactly `#1b1b1b` in launch B and essentially none of it is `#1b1b1b` in launch A. Both directions, because a coincidence between lighthouse's background and the default colorscheme's would otherwise satisfy the assertion; the tests README makes the control band mandatory for exactly this.
- **The orlando band.** The fixture is a Markdown file carrying a fenced code block whose lines are short. Orlando's `code_block` feature fills the block's rows to the grid edge — `crates/wolf-ffi/tests/orlando_bg_stress.rs`'s own doc comment states it as "(fences inclusive, `hl_eol` fill to the grid edge)", so this is established rather than hoped. Sample a rectangle in the **right margin** of a code-block row, past the longest code line and inside the grid, and assert: in launch B it is a single uniform colour **different** from the prose band's colour; in launch A it is the **same** as the prose band's. Deliberately relational rather than pinned to `#252525`, so the assertion does not encode lighthouse's `CursorLine` value or orlando's link target and survives a pin bump that changes either.

Fixture rules that apply and must not be forgotten: a fresh temp cwd; a fixture name lexically unlike every asserted term (and added to the three mechanical fixture guards' tables if it enters their scope); the cursor parked off the sampled rows with a normal-mode `o` then Escape; the sparse-text page segmentation the helpers pass.

**If the orlando pixel band proves not to discriminate** — measure it, do not assume — the readbacks at lines 2-3 remain the evidence and the row is ticked on those. Say so in `tests/README.md`'s "What is still unproven" in that case, in the form "the plugin is proved loaded inside the packaged app's captive Neovim; that its code-block fill reaches a pixel is not". Do not tick a row on what the mechanism implies.

**Smoke P3 — `the_packaged_binary_opens_a_preview_under_cage`** (`#[ignore]`, recommended). One launch of the store binary with the full scrub, driving the existing preview leg, asserting the document's background on a pixel band the way `preview_document_follows_the_configured_appearance_under_cage` already does. Its purpose is narrow and specific: it is the only thing that would catch a packaged app whose `WebKitWebProcess` / `WebKitNetworkProcess` helpers do not resolve, which `flake.nix`'s current comment claims depends on `PATH` and which the wrapper does not reproduce. **Decision rule for the coder:** run it with no `WEBKIT_EXEC_PATH`; if it passes, do not add the variable and correct the `flake.nix` comment that claims the helpers come from `PATH`; if it fails, add `--set-default WEBKIT_EXEC_PATH "${pkgs.webkitgtk_6_0}/libexec/webkitgtk-6.0"` to `gappsWrapperArgs` and re-run. Setting a variable nothing verified is the thing to avoid in either direction.

**Guard G1 — `every_shellhook_export_is_scrubbed_or_deliberately_kept`** (pure, **not** ignored, runs in the ordinary suite). Read `flake.nix` with a path rooted at `CARGO_MANIFEST_DIR`, collect every `export <NAME>=` inside the `shellHook`, and assert each name appears either in the packaged smokes' removal list or in a short, explicitly-named `KEPT_FOR_THE_COMPOSITOR` allowlist (`XDG_RUNTIME_DIR`, `WLR_BACKENDS`, `WLR_LIBINPUT_NO_DEVICES`). Without it, the next `flake.nix` export silently leaks into the packaged runs and every packaged claim quietly becomes an inherited-environment claim — the same distance-between-fixture-and-assertion problem the three fixture-name guards exist for. Assert from both sides: at least one export found (non-vacuous) and no unclassified name.

### 5.5 New file: `/home/jplein/dev/jplein/wolf/tools/wolf-package-smoke.sh`

Executable, `set -euo pipefail`, run from the repository root. Roughly: `nix build .#wolf-gnome --print-out-paths` (or `nix build` then `readlink -f result`) into `WOLF_PACKAGE_OUT`; then `nix develop --command cargo test -p wolf-gnome --test packaged_layout -- --ignored --nocapture` and `nix develop --command cargo test -p wolf-gnome --test smoke_headless -- --ignored --nocapture packaged_`, both with `WOLF_PACKAGE_OUT` exported. It prints the resolved store path before it starts, because that path is the evidence's subject and a recorded run must name it.

This is the "one command that always sets the variable" that makes the ignore honest. It is also the command `build.md` records as the packaged-artifact evidence.

---

## 6. Test plan

Each behavior names the smallest source edit that falsifies it.

### 6.1 `init_lua.rs` unit table (in-module `#[cfg(test)] mod tests`, no environment)

| Behavior | Falsifying revert |
| --- | --- |
| A non-empty `env_override` wins over an existing exe-relative candidate | delete the `env_override` branch |
| A non-empty `env_override` wins even when the path does **not** exist (the predicate answers `false` for everything) | add an `exists` check to the `env_override` branch — this is the case that pins the deliberate asymmetry |
| An all-whitespace or empty `env_override` is ignored and the next branch runs | drop the `.trim().is_empty()` test |
| `exe_dir` + a predicate answering `true` yields `<exe_dir>/../share/wolf/nvim/init.lua` | change `INSTALLED_INIT_LUA_RELATIVE` |
| `exe_dir` + a predicate answering `false` falls through to `manifest_fallback` | delete the `exists` guard on branch 2 — this is the case that keeps `cargo run` and all 63 existing smokes working |
| `exe_dir == None` falls through to `manifest_fallback` | make branch 2 unwrap |
| The joined candidate is passed to the predicate **verbatim** (record the exact path the predicate saw) | insert a `canonicalize` |
| Branch order: env beats exe-relative beats fallback, asserted with all three available at once | swap any two branches |

### 6.2 `vendored_pins.rs`

| Behavior | Falsifying revert |
| --- | --- |
| A Nix/Bazel commit mismatch fails, per asset | change one hex digit of `wolfVendoredPins.orlando.commit` in `flake.nix` |
| A Nix/Bazel sha256 mismatch fails, per asset | change one hex digit of `wolfVendoredPins.lighthouse.sha256` |
| All three assets are checked, not just the first | change `ia-writer-mono`'s commit and confirm the failure names *that* asset |
| Two broken rows produce two named mismatches, not one | change two assets at once (this is why mismatches are collected rather than asserted in a loop) |
| The check is non-vacuous on the Bazel side | reword `_COMMIT = "…"` in one `.bzl` to `_COMMIT= "…"` with no space and confirm the zero-match arm fires with its own message rather than the check passing |
| The Nix block's arity is pinned | add a fourth entry to `wolfVendoredPins` |

### 6.3 `packaged_layout.rs` (all `#[ignore]`d; falsifiers are `flake.nix` edits)

| Behavior | Falsifying revert |
| --- | --- |
| `init.lua` sits where the resolver looks and is byte-identical to the checkout's | change the `postInstall` destination to `share/wolf/init.lua` |
| Both vendored trees are siblings of it | delete the `wolfLighthouse` copy from `postInstall` |
| The Bazel exclusion list took | add `test/` to `wolfOrlando`'s `installPhase` and watch the "no `orlando/test`" assertion fire |
| `Exec` is the absolute wrapped path and carries no field code | change `Exec=` to `wolf-gnome`, and separately to `$out/bin/wolf-gnome %U` |
| No `MimeType` | add `MimeType=text/markdown;` |
| Each icon's pixel size matches its directory | install the 48 px export into the 256 directory |
| The wrapper carries every promised variable | delete one `--set-default` line from `gappsWrapperArgs` |
| `WOLF_INIT_LUA` is never wrapped in | add `--set-default WOLF_INIT_LUA …` |
| Both font families ship and the additive config names them | delete the JetBrains Mono copy; separately, delete the `<include>` line from the generated `fonts.conf` |

### 6.4 Packaged compositor smokes (all `#[ignore]`d)

| Behavior | Falsifying revert |
| --- | --- |
| P1: the store binary launches and renders with none of the devShell's exports | delete `--set-default WOLF_NVIM_BIN` from `gappsWrapperArgs` (the app prints `failed to start core: WOLF_NVIM_BIN is not set`, opens no window, and the scenario exits 11) |
| P1: it finds `init.lua` by its own path | delete branch 2 from `init_lua::resolve_init_lua` (Neovim gets a `-u` naming the build-tree path, which does not exist in the packaged run; core never attaches) |
| P1: the GTK runtime comes from the wrapper, not the shell | drop `pkgs.gsettings-desktop-schemas` from `buildInputs` |
| P2: lighthouse is active in the packaged app | delete the `wolfLighthouse` copy from `postInstall` — readback line 1 and the pixel band both fire |
| P2: the control leg is discriminating | add the lighthouse tree next to the checkout's `runtime/nvim/init.lua` and confirm launch A's assertions fire, then remove it. This is the check that the A/B is an A/B |
| P2: orlando is loaded in the packaged app | delete the `wolfOrlando` copy from `postInstall` — readback lines 2-3 fire |
| P2: the packaged Neovim is the tested Neovim | point `wolfNeovim` at `pkgs.neovim` from the main `nixpkgs` input while temporarily bumping that input, and confirm the `v:progpath` equality fires. If the two inputs cannot be made to differ cheaply, record that this assertion has no executed red run and say so in `tests/README.md`'s "What is still unproven" — do not claim a red run that was not taken |
| G1: the scrub list tracks the shellHook | add a new `export WOLF_ANYTHING=…` to `flake.nix`'s `shellHook` and confirm G1 fires |

### 6.5 Existing tests that must keep passing unchanged

`smoke_headless`'s 63, `release_profile.rs` (both halves — note its ELF comment already cites `app.rs::init_lua_path` baking `CARGO_MANIFEST_DIR`, which stays true, since branch 3 still holds the macro), `paint_golden.rs`, `roundtrip_nvim.rs`, `mouse_nvim.rs`, `frame_metrics_gate.rs`, every `#[cfg(test)]` seam table, and the whole `wolf-core` / `wolf-ffi` suite. Nothing in this unit may weaken or delete an assertion.

---

## 7. Implementation order

1. `crates/wolf-gnome/src/init_lua.rs` (new module + its unit tables).
2. `crates/wolf-gnome/src/lib.rs` — `pub mod init_lua;` and the `//!` bullet. Compiles.
3. `crates/wolf-gnome/src/app.rs` — rewrite `init_lua_path` as glue. `cargo build -p wolf-gnome` and `cargo test -p wolf-gnome --lib` green here; run the full `cargo test -p wolf-gnome` once to prove the fallback branch still carries all 63 smokes before any flake work.
4. `flake.nix` — `wolfVendoredPins` and the `iaWriterMono` refactor **only**. `nix develop --command true` must still enter the shell; nothing else changed.
5. `crates/wolf-gnome/tests/vendored_pins.rs`. Green against the refactor, and red under a one-digit edit.
6. `flake.nix` — the `nixpkgs-runtime` input, `pkgsRuntime`, `wolfNeovim` / `wolfRipgrep` / the treesitter re-sourcing, and the `linuxShell` swap. Re-enter the shell; confirm `WOLF_NVIM_BIN` is the same store path as before (it must be — same rev).
7. `flake.nix` — `wolfOrlando`, `wolfLighthouse`. `nix build` them individually first (`nix build .#packages.x86_64-linux.wolf-gnome` is not yet defined; build the derivations through a temporary `nix eval`/`nix build -f` or simply proceed to step 8 and inspect the output tree).
8. `flake.nix` — `wolfGnomePackage` and the `optionalAttrs` outputs block. `nix build .#wolf-gnome` must succeed, and `nix eval --raw .#devShells.aarch64-darwin.default.drvPath` must still succeed (the Darwin laziness re-check the ledger's existing rows perform).
9. `tools/wolf-package-smoke.sh`.
10. `crates/wolf-gnome/tests/packaged_layout.rs`. Run it through the script; iterate on `postInstall` until green.
11. `smoke_headless.rs` — guard G1 first (pure, fast, and it constrains the helper), then `packaged_cage_command`, then P1, then P2, then P3.
12. Documentation, §8.
13. The full Linux verification gate from `build.md`, then the packaged-artifact command.

---

## 8. Documentation, per file

- **`build.md`** — §"The optimized build" line 55: the parenthetical "because the binary resolves the captive `init.lua` through a compile-time path into its own build tree" is now the *third* branch, not the rule; rewrite it to say the checkout binary resolves the captive `init.lua` from the checkout, and link the packaged section. Add a new subsection under "Linux and NixOS (Cargo)" — the `nix build` command, what the output carries (`bin/`, `share/wolf/nvim/`, `share/applications/`, `share/icons/hicolor/`, `share/fonts/`), how a NixOS configuration consumes it as a flake input, the `fonts.packages` note, and the packaged-evidence command `./tools/wolf-package-smoke.sh`. The paragraph at lines 86-88 about running outside the devShell stays true and stays.
- **`feat-list.md`** — tick the four Linux boxes (Foundations "Neovim bundled into the shipped application"; Appearance "Application icon", "Vendored `orlando` rendering plugin", "Vendored `lighthouse` colorscheme") with `linux-feat/nixos-package` in the Branch column. Rewrite line 27 (the Foundations note claims "there is no packaged Linux application that carries its own Neovim the way `Wolf.app` does"). Rewrite line 206 (the Appearance note claims orlando and lighthouse are "bundled by Bazel into `Wolf.app` only … on Linux they are simply not there"), naming the packaged sibling layout and the evidence. Rewrite line 252 (Reliability note: "the Linux binary runs from the tree it was built in, because the captive `init.lua` is resolved through a compile-time absolute path"). Add a sentence to line 198 about the packaged app's font resolution if the mechanism lands. Delete "Suggested order of work" item 1 and **renumber 2-5 as 1-4**.
- **`crates/wolf-gnome/DECISIONS.md`** — §"Platform and build", the paragraph at line 21 is made false by this unit and must be *rewritten in place*, not appended to: state the three-branch resolution, that the executable-relative branch is what makes a packaged binary work, that the `CARGO_MANIFEST_DIR` fallback is what keeps `cargo run` and the smokes working, and that the `WOLF_INIT_LUA` override is deliberately not existence-checked. Same section: a sentence that the crate now has a package output in `flake.nix` and that its evidence comes from ignored smokes rather than the default suite. §"The headless smoke test": one sentence that a set of ignored smokes runs the store artifact under the same compositor and that `tests/README.md` owns them.
- **`crates/wolf-gnome/src/README.md`** — a new `#### init_lua.rs` entry in alphabetical position (between `input.rs` and `layout.rs`), Purpose / Public API / Decisions / Gotchas per the template, under the 200-word budget. The Decisions bullets are the two asymmetries: the override is not existence-checked while the exe-relative guess is, and `INSTALLED_INIT_LUA_RELATIVE` is the contract with the Nix install layout that `packaged_layout.rs` joins on. The Gotcha is the `wrapGAppsHook4` wrapper and `current_exe()`. Also edit the existing `#### app.rs` entry's Public API bullet, which names `init_lua_path` among the four `pub(crate)` helpers — it now reads the environment and `current_exe` and delegates the rule.
- **`crates/wolf-gnome/README.md`** — line 31 says "Three environment variables matter"; `WOLF_INIT_LUA` makes it four. Add a short "Packaging" line pointing at `build.md` for the Nix package (the outside view belongs here; the commands belong there).
- **`crates/wolf-gnome/tests/README.md`** — new `#### vendored_pins.rs` and `#### packaged_layout.rs` entries; update `#### smoke_headless.rs`'s counts (the pure guard makes it 64 passed, plus N ignored) and its Covers/Gotchas to name the packaged smokes and the one command that runs them; add whatever the P2 measurement leaves unproven to "What is still unproven", in the exact form the section uses.
- **`resources/icons/README.md`** — the GNOME bullet's "**reserved for a future GNOME pass and not consumed by any build today**" is falsified. Rewrite the bullet, and rewrite "## What the build consumes" to name both consumers: the two macOS 1024×1024 masters through `BUILD.bazel`, and the four GNOME **light** PNGs plus the scalable SVG through `flake.nix`'s package, installed as `org.wolf.Wolf.{png,svg}` under `share/icons/hicolor/`. State why the dark export is not installed (hicolor has no appearance axis; both exports are self-contained round icons with their own background plate). Add an "Updating the GNOME icon" step mirroring the macOS one.
- **`tools/README.md`** — one bullet for `wolf-package-smoke.sh`.
- **`macos-followups.md`** — one new row, dated, branch `linux-feat/nixos-package`, carrying: (a) the Neovim patch divergence (Nix 0.12.2 / Bazel 0.12.3 / unstable 0.12.4), that it predates this unit, that the Linux side is now rev-pinned so it cannot drift further, and that closing it means either an off-cache source build or a Linux re-gate; (b) the obligation to add the reciprocal "BUMP BOTH TOGETHER" docstring note to `bazel/orlando/extensions.bzl` and `bazel/lighthouse/extensions.bzl` (this unit deliberately did not touch them) and to point them at `crates/wolf-gnome/tests/vendored_pins.rs`; (c) the standing `flake.nix`-changed spot check the ledger's other rows carry, with the Darwin `drvPath` eval result from step 8; (d) that the `.desktop` entry declares no `MimeType` and no field code because `run_with_args::<&str>(&[])` discards argv, which is a shared front-end property worth reconciling if macOS ever gains a document-type association.
- Run `prettier --write` on every Markdown file touched, and `./tools/lint/markdown_check.sh` on them.

`architecture.md` needs no change — nothing here crosses a crate boundary or changes a contract between crates.

---

## 9. Dependencies

**No new Rust crates.** `cairo-rs`'s `png` feature (already enabled, and already gating a test that runs on every `cargo test`) is what decodes the icons in `packaged_layout.rs`; `vendored_pins.rs` uses `std` string scanning rather than a regex crate, for the reason `release_profile.rs` records — this crate declines a dev-dependency for a test, because a lock-file change carries a `CARGO_BAZEL_REPIN` question nobody can answer from Linux.

**New Nix inputs:** one, `nixpkgs-runtime`, rev-locked to `9ae611a455b90cf061d8f332b977e387bda8e1ca`. `flake.lock` will gain an entry; today it resolves to a rev the lock already carries, so no new download.

**New nixpkgs packages referenced:** `wrapGAppsHook4`, `desktop-file-utils`, `librsvg`, `makeRustPlatform`, plus the existing GTK/WebKit set — all in nixpkgs, none new to the closure beyond the hooks.

---

## 10. Lint considerations

- `cargo clippy --workspace --all-targets -- -D warnings` covers `tests/*.rs`. Expect `clippy::too_many_lines` and `clippy::missing_panics_doc` pressure in the new test files; prefer splitting helpers over an `#[allow]`. Where an allow is unavoidable, it carries a one-line reason, matching the crate's habit.
- `resolve_init_lua` takes four arguments including an `impl Fn`; that is under clippy's `too_many_arguments` threshold and matches `palette::open_recent_candidates`. No allow expected.
- `#[must_use]` on `resolve_init_lua`, per the crate's convention for every seam.
- The new constants are `&str` and need no allow.
- `packaged_layout.rs` and the packaged smokes will trip `clippy::unwrap_used` habits — this crate's tests use `unwrap_or_else(|e| panic!(…))` with an explanatory message everywhere, not bare `unwrap`; follow it, because these panics *are* the failure messages an operator reads.
- Markdown: `./tools/lint/markdown_check.sh` on every edited doc — watch the per-entry word budget on the new `src/README.md` entry and the banned changelog words ("now", "no longer", "previously") in every rewritten paragraph, especially the `DECISIONS.md` and `feat-list.md` rewrites, where the temptation to write "there is now a packaged application" is high.
- `prettier --check "**/*.md"` gates; `proseWrap: never` means no hard wraps inside a paragraph.

---

## 11. Open questions / risks

1. **The fontconfig mechanism is the least certain thing here.** I have verified what does *not* work (`XDG_DATA_DIRS` is never scanned; fontconfig's whole environment surface is `FONTCONFIG_FILE` / `FONTCONFIG_PATH` / `FONTCONFIG_SYSROOT`, none additive) and specified the additive `FONTCONFIG_FILE` that includes `/etc/fonts/fonts.conf`. Two residual risks: on a host with `fonts.fontconfig.enable = false` the include is skipped and the app sees **only** its own two families, which would render the GTK chrome in an editor face; and I have not executed the mechanism. Mitigation: P1's OCR runs with `FONTCONFIG_FILE` scrubbed, so a total failure to resolve any font is visible, and `packaged_layout.rs` pins the file's content. If a stronger font discriminator is wanted, the cheapest one is a `winwidth(0)` readback compared against a column count derived in-process from `cell_metrics_for_smoke()` — the packaged app's advance width would differ under a substituted family. Considered and rejected as the primary mechanism: `FcConfigAppFontAddDir` through a new `fontconfig-sys` dependency, which is the true macOS analogue (`FontRegistration.swift` registers `Contents/Resources/fonts/` with CoreText) but adds a C-FFI crate, a `Cargo.lock` change with an unanswerable-from-Linux repin question, and a Pango-font-map-cache ordering hazard on the startup path.
2. **WebKit's helper processes.** `flake.nix`'s `linuxShell` comment asserts the out-of-process helpers are found through `PATH`; if that is literally true the wrapper must carry `WEBKIT_EXEC_PATH` or a `PATH` prefix. P3 is the cheap experiment that settles it, and §5.4 gives the decision rule for each outcome. If P3 is dropped for scope, the packaged preview pane is unverified and `tests/README.md` must say so.
3. **The `v:progpath` equality assertion may have no executed red run** on this host, because making the two Neovims differ means temporarily bumping the main `nixpkgs` input. If the coder cannot take that measurement cheaply, the assertion is recorded as unfalsified rather than claimed.
4. **The orlando pixel band is the one assertion whose discriminating power is inferred** from `orlando_bg_stress.rs`'s `hl_eol` statement rather than measured through `grim`. Measure it; if it does not discriminate, fall back to the readback and record the gap.
5. **`nix build` is a cold Rust build** — `build.md` measures a cold `cargo build --release` at ~53 s wall / ~6 m 20 s user on this host for 167 crates, and the derivation cannot reuse `target/`. Every `./tools/wolf-package-smoke.sh` after a source change pays that. Not a blocker, but the orchestrator should not expect a fast loop.
6. **Source filtering is mandatory, not tidiness.** A `src = ./.` without a filter copies `target/` — over a gigabyte here — into the store on every evaluation. If `nix build` appears to hang before it compiles anything, this is why.
7. **The Nix path literal with spaces** (`resources/icons/Wolf application icon GNOME light`) is not a valid path literal and must be built by path-plus-string concatenation. This is an easy silent-looking eval error.
8. **`optionalAttrs` on the outputs is required and looks like the guard the `NOTE:` block forbids.** If a later reader removes it "for consistency", `nix flake check` starts trying to evaluate a Wayland GTK application for Darwin. The comment distinguishing the two cases is load-bearing.
9. **Adding orlando or lighthouse to `linuxShell` would silently destroy P2's control leg.** It looks like a convenience; it is not. Worth a comment beside both derivations.

---

## 12. Orchestrator rulings on the test-writer's report

1. **G1 vs. ruling 3's two new exports.** Adopted as the test-writer recommends: `WOLF_TEST_ORLANDO_DIR` and `WOLF_TEST_LIGHTHOUSE_DIR` go in `PACKAGED_SCRUB_ALWAYS`, not in `KEPT_FOR_THE_COMPOSITOR`. No `wolf-gnome` code reads either, so scrubbing them costs nothing and the keep-list stays exactly the three variables `cage` itself needs. The coder must NOT make this edit; it reports G1's failure and the test-writer makes it.
2. **`PACKAGED_LIGHTHOUSE_NORMAL_BG` is a measurement, not a guess.** The coder must read `colors/lighthouse.lua` out of the fetched store tree at the pinned commit `514dec2c…` and read the P2 sentinel back, and report the actual `Normal` background. If it is not `#1b1b1b`, report the measured value — do not edit the constant.
3. **If P2's band-uniformity precondition fires**, the answer is that P2 keeps `FONTCONFIG_FILE` — the same exemption shape as its `GtkRuntimeEnv::Inherited` carve-out, because P1 is where the packaged-fontconfig claim is made and P2's subject is the two vendored Lua trees. Report it; the test-writer makes the edit.
4. **The orlando band's discriminating power must be measured, never inferred.** If it does not discriminate, the row is ticked on the readbacks alone and `tests/README.md` says what stays unproven.
5. The coder runs `cargo fmt` over every file it or the test-writer touched.

---

## 13. Orchestrator rulings on the coder's report

The implementation is accepted. Six test-side items, ruled here on the coder's measurements.

1. **`cage_is_only_spawned_through_the_isolating_helper`** — the test-writer's own section comment carries a literal `Command::new("cage")`, which the guard counts. Break the literal in the comment the way `CAGE_SPAWN_NEEDLE` is broken. Test-writer edit.

2. **Three `doc_lazy_continuation` clippy errors** in `PACKAGED_PLUGINS_SCENARIO`'s doc comment. Test-writer edit.

3. **`REQUIRED_WRAPPER_VARS` drops `GSETTINGS_SCHEMA_DIR`.** Measured: `wrapGAppsHook4` emits no such flag for any `buildInputs` configuration — it supplies schemas through `XDG_DATA_DIRS`, the standard GSettings lookup — and P1 launches, reaches `activate` and draws readable glyphs under the full scrub. The premise in the test's own comment is false and the comment is corrected with it. **No speculative sixth `--set-default` is added**: the rule is not to set a variable nothing verified, and that rule cuts both ways.

4. **P2's orlando band changes shape, not strength.** Two edits, both required, and neither is a weakened assertion:
   - P2 keeps `FONTCONFIG_FILE` (ruling §12.3, pre-committed), because P2's subject is whether the two vendored Lua trees load and the font stack is a confound P1 already owns.
   - The band asserts **presence against absence of orlando's fill colour**, not uniformity. Measured, the fill cannot be uniform: a 1-px antialiased seam every ~22.4 px — the cell pitch — caps any per-cell-filled row at about 0.961, which is a property of Cairo drawing one background rectangle per cell and not of the assertion. So the floor must be a fraction of the sampled region carrying `#252525` in launch B, with a hard ceiling in launch A, and the constant's comment must carry the measured seam spacing and the 0.961 ceiling as its justification.
5. **P3 keeps a narrower claim than the sibling smoke it borrowed from.** Measured: lighthouse's `Normal` background and `PREVIEW_DARK_BG` are both `#1b1b1b`, so the editor pane and the preview document are the same colour by construction and `assert_preview_theme_launch`'s control band cannot discriminate for this binary. P3 drops that reuse and asserts what it can: cage exit 0, the window-list readback carrying both the source buffer and the preview scratch, and the right pane rendering the document. Its doc comment states why the appearance discriminator is unavailable here and that the light/dark claim belongs to `preview_document_follows_the_configured_appearance_under_cage` on the checkout binary. P3's purpose was to settle the WebKit-helper question by measurement, and it did: `WEBKIT_EXEC_PATH` is not needed, the helper path is baked into `libwebkitgtk-6.0.so.4` as an absolute store path, and the token does not appear in the library at all.

6. **New assertion, adopted: the two sibling trees must be non-empty, not merely present.** The coder measured that an empty `lighthouse/` satisfies `init.lua`'s `fs_stat` gate, makes `:colorscheme lighthouse` error, aborts `init.lua` mid-file and kills the app with `attempt to call global 'wolf_setup_current_file_tracking' (a nil value)`. An empty sibling is strictly worse than an absent one and nothing guards it.

### Measurements accepted as recorded

- `PACKAGED_LIGHTHOUSE_NORMAL_BG` is `#1b1b1b`, from two independent readings (the pinned `colors/lighthouse.lua`, and P2's sentinel), against a control reading `14161b`. The constant stands.
- The `pkgsRuntime` swap is store-path-identical for all four variables.
- `WEBKIT_EXEC_PATH` is not needed; the `flake.nix` comment claiming `PATH` is corrected.
- The orlando band discriminates: `#252525` at 0.944 in the packaged launch against `#14161b` at 1.000 in the control.
- The `v:progpath` assertion is live and green on real data (both launches read the same unwrapped store path) rather than vacuous; no red run was taken, and none is claimed.
