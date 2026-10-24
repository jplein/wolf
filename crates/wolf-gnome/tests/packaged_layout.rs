//! Layout assertions about the INSTALLED Nix output — `nix build .#wolf-gnome`'s
//! `$out` — rather than about the checkout build.
//!
//! A green `cargo test` says nothing about `$out/bin/wolf-gnome`: they are two
//! different artifacts making two different claims. Everything here reads the store
//! tree the packaging derivation produced, and every one of its falsifiers is a
//! `flake.nix` edit rather than a `src/` one.
//!
//! ## Why every test is `#[ignore]`d, and what makes that honest
//!
//! These tests have a precondition no ordinary `cargo test` can satisfy: a built
//! store path, handed over in `WOLF_PACKAGE_OUT`. The two alternatives are both
//! worse. A test that SKIPS when the variable is unset is a silently-skipped test,
//! which `CLAUDE.md` and `crates/wolf-gnome/tests/README.md` both forbid — the whole
//! file would go green on a machine that had never run `nix build`. A test that
//! FAILS when it is unset breaks the default suite for every developer.
//!
//! `#[ignore]` resolves that cleanly, and it has precedent in this crate
//! (`paint_golden.rs::regenerate_png_golden`): an ignored test is *not part of the
//! default suite*, which is a different thing from being silently skipped. What
//! makes it honest is that exactly ONE command runs these, and that command always
//! sets the variable:
//!
//! ```sh
//! ./tools/wolf-package-smoke.sh
//! ```
//!
//! So the precondition inside [`packaged_out`] still fails LOUD — it panics on an
//! unset, empty, non-directory or binary-less value — because by the time it runs,
//! the only caller has already promised to satisfy it.
//!
//! ## The one contract this file exists to keep
//!
//! Test 1 joins `<out>/bin` with [`wolf_gnome::init_lua::INSTALLED_INIT_LUA_RELATIVE`]
//! rather than retyping the path. That join is the whole point: the Rust resolver's
//! executable-relative branch and the Nix `postInstall`'s destination are ONE
//! contract, and a test that retyped the path would let them drift while staying
//! green. Every other path below hangs off the directory that join lands in.
//!
//! NO BAZEL WIRING, and none is missing. `crates/wolf-gnome` has no `BUILD.bazel` at
//! all — that absence is deliberate and load-bearing, see
//! [`notes/wolf-gnome-decisions.md`](../../../notes/wolf-gnome-decisions.md) — so there is no `rust_test`
//! target for this file to be added to. Do not "fix" the missing target.

use std::path::{Path, PathBuf};

use cairo::ImageSurface;
use wolf_gnome::init_lua::{INIT_LUA_ENV, INSTALLED_INIT_LUA_RELATIVE};

// ---------------------------------------------------------------------------
// The precondition, and the one command that satisfies it.

/// The environment variable carrying the store path under test. ONE variable, and
/// the binary is derived from it rather than passed separately, so the two cannot
/// name different outputs.
const OUT_ENV: &str = "WOLF_PACKAGE_OUT";

/// The command that builds the artifact.
const BUILD_COMMAND: &str = "nix build .#wolf-gnome";

/// The command that sets [`OUT_ENV`] and runs every test in this file.
const SMOKE_COMMAND: &str = "./tools/wolf-package-smoke.sh";

/// The installed executable, relative to the store root.
const BIN_RELATIVE: &str = "bin/wolf-gnome";

/// The store path under test, checked to be a real package before anything reads it.
///
/// Panics — never skips — on each of the four ways the precondition can be unmet,
/// naming which one and naming the command that fixes it. A skip here would be the
/// silently-skipped test this file's header rejects.
fn packaged_out(test: &str) -> PathBuf {
    let raw = match std::env::var(OUT_ENV) {
        Ok(v) if !v.trim().is_empty() => v,
        Ok(_) => panic!(
            "{test}: {OUT_ENV} is set but EMPTY. It must be the store path `{BUILD_COMMAND}` \
             produced, and `{SMOKE_COMMAND}` is what resolves it and exports it. An empty value \
             usually means the build step's `--print-out-paths` produced nothing, which is a \
             failed build reported as an empty string."
        ),
        Err(e) => panic!(
            "{test}: {OUT_ENV} is not set ({e}). Every test in this file is `#[ignore]`d and reads \
             the INSTALLED artifact rather than the checkout build, so it is run by exactly one \
             command: `{SMOKE_COMMAND}`, which runs `{BUILD_COMMAND}` and exports the resulting \
             store path. Running these with `--ignored` by hand needs that variable exported \
             first. This is a PANIC and not a skip on purpose: a skip would let the whole file go \
             green on a machine that never built the package."
        ),
    };

    let out = PathBuf::from(raw);
    let meta = std::fs::metadata(&out).unwrap_or_else(|e| {
        panic!(
            "{test}: {OUT_ENV} names {}, which cannot be read: {e}. A dangling `result` symlink \
             after a garbage collection looks exactly like this; re-run `{BUILD_COMMAND}` (or \
             `{SMOKE_COMMAND}`, which does).",
            out.display()
        )
    });
    assert!(
        meta.is_dir(),
        "{test}: {OUT_ENV} names {}, which is not a directory. It must be the derivation's `$out` \
         ROOT, not the binary inside it: every path in this file is derived from that root, and \
         `{BIN_RELATIVE}` in particular is appended to it.",
        out.display()
    );

    let bin = out.join(BIN_RELATIVE);
    assert!(
        bin.is_file(),
        "{test}: {} has no `{BIN_RELATIVE}`, so it is not a wolf-gnome package. Either \
         {OUT_ENV} names some other store path, or the derivation's `cargoBuildFlags` stopped \
         installing the binary. Re-run `{SMOKE_COMMAND}`.",
        out.display()
    );

    out
}

/// The captive `init.lua` inside the package, found the way the running application
/// finds it.
///
/// `<out>/bin` joined with [`INSTALLED_INIT_LUA_RELATIVE`], NOT normalized — the same
/// `bin/../share/...` string `init_lua::resolve_init_lua` builds and hands Neovim as
/// `-u`, so this test walks exactly the path the app walks.
fn packaged_init_lua(out: &Path) -> PathBuf {
    out.join("bin").join(INSTALLED_INIT_LUA_RELATIVE)
}

/// The directory that `init.lua` sits in, which is where the captive runtime's
/// `orlando/` and `lighthouse/` siblings have to be.
///
/// Derived from [`packaged_init_lua`] rather than spelled out, so a change to
/// [`INSTALLED_INIT_LUA_RELATIVE`] moves both together.
fn packaged_nvim_dir(test: &str, out: &Path) -> PathBuf {
    let init = packaged_init_lua(out);
    init.parent()
        .unwrap_or_else(|| {
            panic!(
                "{test}: the joined init.lua path {} has no parent directory, which cannot happen \
                 for a path built by joining a relative path onto an absolute one. \
                 INSTALLED_INIT_LUA_RELATIVE = {INSTALLED_INIT_LUA_RELATIVE:?}.",
                init.display()
            )
        })
        .to_path_buf()
}

/// The checkout's own copy of the captive `init.lua`, resolved from this crate's
/// directory exactly as `release_profile.rs` resolves the workspace manifest.
fn checkout_init_lua() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/nvim/init.lua")
}

/// Reads a file, or panics naming it and what its absence means.
fn read_bytes(test: &str, what: &str, path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| {
        panic!(
            "{test}: could not read {what} at {}: {e}. Every path here is derived from {OUT_ENV}, \
             so a missing file means the derivation's `postInstall` did not write it — not that \
             the variable is wrong, which `packaged_out` would already have caught.",
            path.display()
        )
    })
}

/// Every file name (not directory) beneath `dir`, recursively.
///
/// Used for the two "a `LICENSE*` is somewhere under this tree" assertions and for
/// the font families, because neither claim should pin the subdirectory the installer
/// chose — `LICENSE.md` against `LICENSE`, `share/fonts/truetype/` against
/// `share/fonts/`. What each assertion pins is the FILE, and a walk is what lets it
/// do that without also pinning a layout it has no opinion about.
fn file_names_under(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return names;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            names.extend(file_names_under(&path));
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            names.push(name.to_string());
        }
    }
    names
}

// ---------------------------------------------------------------------------
// Test 1: the captive init.lua, at the path the resolver itself computes.

/// The packaged `init.lua` sits where [`INSTALLED_INIT_LUA_RELATIVE`] says it does,
/// and is byte-identical to the checkout's.
///
/// ## What it proves that nothing else can
///
/// This is the ONE join that makes the Rust resolver and the Nix install layout a
/// single contract. `init_lua::resolve_init_lua`'s unit table proves the resolver
/// forms `<exe dir>/../share/wolf/nvim/init.lua`; nothing in that table can say a
/// file is there. This test appends the same constant to the real store path and
/// looks.
///
/// The byte comparison is the second half and is not decoration: a `postInstall` that
/// generated an `init.lua` — or copied a stale one from somewhere else — would
/// satisfy an existence check while shipping a runtime the whole `wolf-core` suite
/// was never run against.
///
/// ## Falsifiers
///
/// Change the `postInstall` destination to `share/wolf/init.lua`; assertion 1 fires.
/// Copy any other file into place; assertion 2 fires and reports both lengths.
#[test]
#[ignore = "reads the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_output_carries_the_captive_init_lua_at_the_resolvers_own_path() {
    const TEST: &str = "the_packaged_output_carries_the_captive_init_lua_at_the_resolvers_own_path";

    let out = packaged_out(TEST);
    let installed = packaged_init_lua(&out);

    // --- Assertion 1: it is there, at the resolver's own join -----------------
    assert!(
        installed.is_file(),
        "{TEST}: no file at {}. That path is `<out>/bin` joined with \
         INSTALLED_INIT_LUA_RELATIVE = {INSTALLED_INIT_LUA_RELATIVE:?} — the exact join \
         `init_lua::resolve_init_lua`'s executable-relative branch builds and hands Neovim as \
         `-u`, and the reason this test reads the constant instead of retyping the path. Its \
         absence means the derivation's `postInstall` writes the captive runtime somewhere else, \
         in which case the packaged app falls through to the CARGO_MANIFEST_DIR branch, is \
         launched against a build tree that does not exist in the store, and opens no window at \
         all. Fix `flake.nix`'s destination or the constant — not this test.",
        installed.display()
    );

    // --- Assertion 2: it is the checkout's file, byte for byte ----------------
    let checkout = checkout_init_lua();
    let installed_bytes = read_bytes(TEST, "the packaged init.lua", &installed);
    let checkout_bytes = read_bytes(TEST, "the checkout's init.lua", &checkout);
    assert!(
        installed_bytes == checkout_bytes,
        "{TEST}: the packaged init.lua at {} is {} bytes and the checkout's at {} is {} bytes, and \
         they differ. The package must ship the SAME captive runtime the Linux suite ran against, \
         because every claim `runtime/nvim/init.lua` carries — the treesitter gate, the preview \
         filter, the two sibling-tree branches — was measured against that file. A generated or \
         patched copy would make the whole Linux gate evidence about a file the user never runs.",
        installed.display(),
        installed_bytes.len(),
        checkout.display(),
        checkout_bytes.len()
    );
}

// ---------------------------------------------------------------------------
// Test 2: the two vendored Lua trees, as siblings of it.

/// The `orlando/` and `lighthouse/` trees sit beside the packaged `init.lua`, and
/// carry exactly the subtrees the Bazel filegroups keep.
///
/// ## Why the siblings and not a runtimepath entry
///
/// `runtime/nvim/init.lua` self-locates both trees from its OWN directory
/// (`debug.getinfo(1, "S").source`) and gates each on an `fs_stat`, so the tree being
/// present as a sibling is the entire activation mechanism. Nothing else turns
/// either on, and a tree installed one directory away is a tree the app silently
/// ignores.
///
/// ## Why "not empty" is asserted before any named file
///
/// An EMPTY sibling directory is strictly worse than an absent one, and nothing else
/// in the repository guards against it. MEASURED with an empty `lighthouse/`: the
/// directory satisfies `init.lua`'s `fs_stat` gate, so the tree is prepended to
/// `runtimepath` and `:colorscheme lighthouse` is called; the colorscheme is not there,
/// the command ERRORS, `init.lua` aborts MID-FILE, and the application dies with
/// `attempt to call global 'wolf_setup_current_file_tracking' (a nil value)` — a
/// message that names neither the tree nor the package. An ABSENT directory fails the
/// gate and the app runs correctly without the feature.
///
/// So the two states have opposite consequences and must be reported apart. This
/// assertion runs first and names the emptiness; the per-file assertion below then
/// names which file a populated-but-wrong tree is missing.
///
/// The two TREES also fail differently, which is why this assertion covers both while
/// only one of them can kill the application. MEASURED with an empty `orlando/`:
/// `vim.cmd.runtime("plugin/orlando.lua")` finds nothing to source and returns
/// silently, `init.lua` runs to its end, and the app behaves normally — nothing but the
/// vendored-tree readbacks in `smoke_headless.rs` observes that the plugin is missing.
/// The empty-lighthouse case is the fatal one and the empty-orlando case the silent
/// one; this assertion is what reports either.
///
/// ## Why the exclusions are asserted too
///
/// The Bazel filegroups keep `plugin/`, `lua/`, `doc/` and `LICENSE*` for orlando and
/// `colors/` plus `LICENSE*` for lighthouse, and drop the rest. The two platforms are
/// supposed to ship the same files; an inclusive `cp -r` on the Nix side would ship a
/// `test/` directory, a `flake.nix` and a `README.md` that macOS does not, and
/// nothing else would report the divergence.
///
/// ## Falsifiers
///
/// Delete the `wolfLighthouse` copy from `postInstall`; the `colors/lighthouse.lua`
/// assertion fires. Replace that copy with a bare `mkdir -p`; assertion 1 fires
/// instead, which is the point of having it. Add `test/` to `wolfOrlando`'s
/// `installPhase`; the "no `orlando/test`" assertion fires.
#[test]
#[ignore = "reads the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_output_carries_the_two_vendored_lua_trees_as_siblings() {
    const TEST: &str = "the_packaged_output_carries_the_two_vendored_lua_trees_as_siblings";

    let out = packaged_out(TEST);
    let nvim = packaged_nvim_dir(TEST, &out);
    let orlando = nvim.join("orlando");
    let lighthouse = nvim.join("lighthouse");

    // --- Assertion 1: neither tree is empty -----------------------------------
    // First, because an empty tree and a mis-installed one fail the running app in opposite ways.
    for (label, tree) in [("orlando", &orlando), ("lighthouse", &lighthouse)] {
        let names = file_names_under(tree);
        let directory = if tree.is_dir() { "THERE" } else { "absent" };
        assert!(
            !names.is_empty(),
            "{TEST}: the packaged {label} tree at {} holds NO files at all, and the directory \
             itself is {directory}. An EMPTY sibling directory is strictly worse than a missing \
             one, which is why this is asserted rather than left to the per-file checks below. The \
             captive `init.lua` gates each tree on an `fs_stat` of the DIRECTORY, which an empty \
             directory SATISFIES, so the gated branch runs against nothing. MEASURED for \
             lighthouse: `:colorscheme lighthouse` errors, `init.lua` aborts MID-FILE, and the \
             application dies with `attempt to call global 'wolf_setup_current_file_tracking' (a \
             nil value)` — a message naming neither the tree nor the package. An ABSENT directory \
             fails the gate and the app runs correctly without the feature. If the directory is \
             THERE, the `postInstall` copy ran against an empty or wrongly-rooted source (check \
             the derivation's `sourceRoot`, which for lighthouse carries the upstream \
             repository's `.nvim` suffix); if it is absent, the copy did not run at all.",
            tree.display()
        );
    }

    // --- Assertion 2: the files each tree must carry --------------------------
    let required: [(&Path, &str, &str); 3] = [
        (
            orlando.as_path(),
            "plugin/orlando.lua",
            "the file `init.lua` sources with `vim.cmd.runtime`, which is what sets \
             `vim.g.loaded_orlando`",
        ),
        (
            orlando.as_path(),
            "lua/orlando/init.lua",
            "the module `require(\"orlando\")` resolves once the tree is on runtimepath",
        ),
        (
            lighthouse.as_path(),
            "colors/lighthouse.lua",
            "the colorscheme `init.lua` activates; a colorscheme merely on runtimepath does \
             nothing at all",
        ),
    ];
    for (tree, relative, why) in required {
        let path = tree.join(relative);
        assert!(
            path.is_file(),
            "{TEST}: no file at {} — {why}. The captive init.lua self-locates both trees from its \
             own directory ({}) and gates each on an `fs_stat`, so a missing file here is a \
             feature that silently does not exist in the packaged app rather than an error \
             anywhere. Check the corresponding `postInstall` copy and the derivation's own \
             `installPhase`.",
            path.display(),
            nvim.display()
        );
    }

    // --- Assertion 3: each tree carries its licence ---------------------------
    // A walk, not a fixed name: one upstream ships `LICENSE` and the other `LICENSE.md`.
    for (label, tree) in [("orlando", &orlando), ("lighthouse", &lighthouse)] {
        let names = file_names_under(tree);
        assert!(
            names.iter().any(|n| n.starts_with("LICENSE")),
            "{TEST}: the packaged {label} tree at {} carries no `LICENSE*` file; it holds \
             {names:?}. Both Bazel filegroups keep `LICENSE*` deliberately, for compliance — Wolf \
             redistributes both trees inside its artifact — so dropping it on the Nix side ships a \
             vendored plugin with no licence beside it.",
            tree.display()
        );
    }

    // --- Assertion 4: the Bazel exclusion list really took --------------------
    let excluded: [(&Path, &str, &str); 4] = [
        (orlando.as_path(), "test", "orlando"),
        (orlando.as_path(), "flake.nix", "orlando"),
        (orlando.as_path(), "README.md", "orlando"),
        (lighthouse.as_path(), "flake.nix", "lighthouse"),
    ];
    for (tree, relative, label) in excluded {
        let path = tree.join(relative);
        assert!(
            !path.exists(),
            "{TEST}: the packaged {label} tree carries {}, which `bazel/{label}/extensions.bzl`'s \
             `runtime_files` filegroup deliberately excludes. The point of vendoring these on both \
             platforms is that both ship the SAME files, so an inclusive copy on the Nix side is a \
             divergence nothing else would report — the macOS bundle would carry one file set and \
             the Linux package another, both green. Restrict the derivation's `installPhase` to \
             the subtrees the filegroup lists.",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: the desktop entry.

/// The section header the desktop-entry scanner keys on.
const DESKTOP_SECTION: &str = "[Desktop Entry]";

/// The desktop entry, relative to the store root.
const DESKTOP_RELATIVE: &str = "share/applications/org.wolf.Wolf.desktop";

/// The `Icon=` value, which is the hicolor basename and NOT a path.
const EXPECT_ICON: &str = "org.wolf.Wolf";

/// A deliberately small line scanner over one `.desktop` group. NOT a full
/// desktop-entry parser, for the reason `release_profile.rs`'s manifest scanner
/// records: parsing properly would mean a dependency, and a dependency here would
/// move `Cargo.lock`.
///
/// STATED COUPLING with the generated file: one `Key=Value` per line inside
/// `[Desktop Entry]`, with no trailing comment on a value line. The file is written
/// by a heredoc in `postInstall`, so that shape is under this repository's control
/// rather than an upstream's.
fn desktop_entry_keys(text: &str) -> Vec<(String, String)> {
    let mut keys = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_section = line == DESKTOP_SECTION;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            keys.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    keys
}

/// The value of one key in a parsed `[Desktop Entry]` group, or `None`.
///
/// A free function rather than a closure so the returned borrow is tied to `keys`
/// alone; a closure taking `wanted: &str` and returning `Option<&str>` ties the two
/// lifetimes together and does not compile.
fn desktop_value<'a>(keys: &'a [(String, String)], wanted: &str) -> Option<&'a str> {
    keys.iter()
        .find(|(key, _)| key == wanted)
        .map(|(_, value)| value.as_str())
}

/// The desktop entry launches the wrapped store binary by absolute path, carries no
/// field code and claims no MIME type.
///
/// ## Why `Exec` must be the absolute store path
///
/// The entry and the binary are outputs of the same derivation, so the path cannot
/// dangle while the entry is visible, and an absolute path removes any dependence on
/// `PATH` ordering. A bare `wolf-gnome` would be resolved by the session's `PATH`,
/// where a developer's `target/release` directory ahead of their profile would launch
/// an UNWRAPPED binary carrying no `WOLF_NVIM_BIN` and no installed `init.lua` — a
/// silent wrong-artifact launch from the application menu.
///
/// ## Why no field code and no `MimeType`
///
/// `app::run` calls `app.run_with_args::<&str>(&[])`, so the process ignores its real
/// argv entirely. A `%f` or `%U` would make the desktop hand Wolf a path it silently
/// drops, and a `MimeType=text/markdown` would put Wolf in the "Open With" list for
/// every `.md` file and then open an empty window. Both are the shape `notes/wolf-gnome-decisions.md`
/// rejects: a control that visibly refuses input.
///
/// ## Falsifiers
///
/// Change `Exec=` to a bare `wolf-gnome`, and separately to `$out/bin/wolf-gnome %U`.
/// Add `MimeType=text/markdown;`.
#[test]
#[ignore = "reads the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_desktop_entry_launches_the_wrapped_binary_and_claims_no_files() {
    const TEST: &str = "the_packaged_desktop_entry_launches_the_wrapped_binary_and_claims_no_files";

    let out = packaged_out(TEST);
    let path = out.join(DESKTOP_RELATIVE);
    let bytes = read_bytes(TEST, "the desktop entry", &path);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let keys = desktop_entry_keys(&text);

    // --- Assertion 1 (non-vacuity): the group parsed at all -------------------
    assert!(
        !keys.is_empty(),
        "{TEST}: {} declares no `{DESKTOP_SECTION}` group this scanner can read, so every key \
         lookup below would answer None and the must-NOT-contain assertions would pass \
         vacuously. Its contents were:\n{text}",
        path.display()
    );

    // --- Assertion 2: the three fixed keys ------------------------------------
    for (key, expected) in [
        ("Type", "Application"),
        ("Icon", EXPECT_ICON),
        ("Terminal", "false"),
    ] {
        let got = desktop_value(&keys, key);
        assert!(
            got == Some(expected),
            "{TEST}: {} states {key} = {got:?}, expected Some({expected:?}). `Type` is what makes \
             the entry launchable at all; `Icon` is a hicolor BASENAME and not a path, so it must \
             match the `org.wolf.Wolf.{{png,svg}}` files installed under \
             `share/icons/hicolor/`; `Terminal=false` is what stops the session opening a \
             terminal window around a GTK application.",
            path.display()
        );
    }

    // --- Assertion 3: Exec is the absolute wrapped path, with no field code ---
    let want_exec = out.join(BIN_RELATIVE);
    let want_exec = want_exec.to_string_lossy().into_owned();
    let exec = desktop_value(&keys, "Exec");
    assert!(
        exec == Some(want_exec.as_str()),
        "{TEST}: {} states Exec = {exec:?}, expected Some({want_exec:?}) EXACTLY. Three outcomes \
         are distinguishable and all are defects. A bare `wolf-gnome` is resolved through the \
         session's PATH, where a developer's `target/release` ahead of their profile launches an \
         UNWRAPPED binary with no WOLF_NVIM_BIN and no installed init.lua — a silent \
         wrong-artifact launch from the application menu. A DIFFERENT store path means the entry \
         and the binary came from different builds. A trailing field code is caught by assertion \
         4 below, which says why.",
        path.display()
    );
    assert!(
        !exec.unwrap_or_default().contains('%'),
        "{TEST}: {} states Exec = {exec:?}, which carries a `%` field code. `app::run` calls \
         `run_with_args::<&str>(&[])`, so the process ignores its real argv entirely: a `%f` or \
         `%U` makes the desktop hand Wolf a path it silently DROPS, and the user sees an empty \
         window instead of their file. Opening a file from the desktop needs `HANDLES_OPEN`, a \
         real argv parse and an `open` handler — it is a unit of its own, not a key.",
        path.display()
    );

    // --- Assertion 4: no MimeType key at all ----------------------------------
    let mime = desktop_value(&keys, "MimeType");
    assert!(
        mime.is_none(),
        "{TEST}: {} declares MimeType = {mime:?}. That puts Wolf in the desktop's \"Open With\" \
         list for the named types and then opens an EMPTY window, because the process discards \
         argv (see assertion 3). Asserted as an absent KEY rather than an empty value, since an \
         empty `MimeType=` is the same promise made in a way a value comparison would miss.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Test 4: the icons, at the sizes their directories name.

/// The hicolor pixel sizes the package installs a PNG for.
const ICON_SIZES: [i32; 4] = [48, 64, 128, 256];

/// The two byte prefixes an SVG may begin with, once leading whitespace is trimmed.
const SVG_PREFIXES: [&str; 2] = ["<?xml", "<svg"];

/// Each installed icon decodes to exactly the pixel size its hicolor directory
/// claims, and the scalable SVG is really an SVG.
///
/// ## Why decoding, and not an existence check
///
/// The real defect this catches is an export installed into the wrong directory —
/// the 48 px PNG copied to `256x256/apps/` — which every existence check in the world
/// passes. The icon theme spec indexes by directory, so the desktop would scale a
/// 48 px image up to 256 and the launcher entry would look soft with nothing failing
/// anywhere.
///
/// The SVG is checked by its first bytes rather than decoded: `cairo` has no SVG
/// loader, and adding `librsvg` bindings to read one file would be a dependency this
/// crate declines. What the prefix check discriminates is a PNG or an empty file
/// copied to a `.svg` name, which is the plausible mistake.
///
/// ## Falsifier
///
/// Install the 48 px export into the 256 directory; the size assertion fires and
/// names both numbers.
#[test]
#[ignore = "reads the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_icons_are_installed_at_the_sizes_their_directories_name() {
    const TEST: &str = "the_packaged_icons_are_installed_at_the_sizes_their_directories_name";

    let out = packaged_out(TEST);
    let hicolor = out.join("share/icons/hicolor");

    for size in ICON_SIZES {
        let path = hicolor.join(format!("{size}x{size}/apps/{EXPECT_ICON}.png"));
        let mut file = std::fs::File::open(&path).unwrap_or_else(|e| {
            panic!(
                "{TEST}: could not open the {size}x{size} icon at {}: {e}. The hicolor theme is \
                 looked up by DIRECTORY, so a missing size is a size the desktop silently \
                 substitutes another for.",
                path.display()
            )
        });
        let surface = ImageSurface::create_from_png(&mut file).unwrap_or_else(|e| {
            panic!(
                "{TEST}: cairo could not decode {} as a PNG: {e}. A zero-length file means the \
                 `install` step copied nothing; a decode error on a non-empty file means the \
                 source was not a PNG at all.",
                path.display()
            )
        });
        let (w, h) = (surface.width(), surface.height());
        assert!(
            w == size && h == size,
            "{TEST}: {} decodes to {w}x{h}, but its directory claims {size}x{size}. The icon-theme \
             spec indexes by directory and never re-measures, so the desktop would SCALE this \
             image to {size} px and the launcher would render a soft or cropped icon with nothing \
             failing anywhere. The named cause is the wrong export copied into this directory in \
             `postInstall` — the sources are `icon-silhouette_<n>x<n>.png` under \
             `resources/icons/Wolf application icon GNOME light/`, and the mapping is one export \
             per directory.",
            path.display()
        );
    }

    // --- The scalable SVG -----------------------------------------------------
    let svg = hicolor.join(format!("scalable/apps/{EXPECT_ICON}.svg"));
    let bytes = read_bytes(TEST, "the scalable icon", &svg);
    assert!(
        !bytes.is_empty(),
        "{TEST}: {} is zero bytes. An empty file at the right path satisfies every existence \
         check and renders nothing.",
        svg.display()
    );
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)])
        .trim_start()
        .to_string();
    assert!(
        SVG_PREFIXES.into_iter().any(|p| head.starts_with(p)),
        "{TEST}: {} does not begin with any of {SVG_PREFIXES:?}; its first bytes are {head:?}. \
         This is a byte-prefix check rather than a decode because `cairo` has no SVG loader and \
         binding `librsvg` to read one file would be a dependency this crate declines — what it \
         discriminates is a PNG, or an empty placeholder, copied to a `.svg` name, which is the \
         plausible mistake.",
        svg.display()
    );
}

// ---------------------------------------------------------------------------
// Test 5: the wrapper's environment.

/// Every variable the wrapper must name for the running application.
///
/// The first four are Wolf's own and are set by `gappsWrapperArgs`; `FONTCONFIG_FILE`
/// is the additive font config the package generates; `XDG_DATA_DIRS` is
/// `wrapGAppsHook4`'s own contribution from `buildInputs`, and it is listed because
/// dropping a runtime dependency from that list changes it silently.
///
/// `GSETTINGS_SCHEMA_DIR` is deliberately NOT in this list, and the reason is a
/// measurement rather than an opinion. The hook emits no such flag for ANY
/// `buildInputs` configuration: the wrapper it writes is
/// `makeCWrapper '…/bin/.wolf-gnome-wrapped' --prefix 'GIO_EXTRA_MODULES' … --set
/// 'GDK_PIXBUF_MODULE_FILE' … XDG_DATA_DIRS …`, and schemas are supplied THROUGH
/// `XDG_DATA_DIRS`, which is the standard GSettings lookup. The devShell exports the
/// variable explicitly, which is what made it look like part of the contract; the
/// package does not, and does not need to. `packaged_binary_renders_typed_text_under_cage`
/// launches the store binary with the whole devShell environment scrubbed, reaches
/// `activate` and draws readable glyphs without it. No sixth `--set-default` is added
/// to make this list come true: setting a variable nothing verified is the thing to
/// avoid, and that rule cuts both ways.
const REQUIRED_WRAPPER_VARS: [&str; 6] = [
    "WOLF_NVIM_BIN",
    "WOLF_RG_BIN",
    "WOLF_NVIM_TREESITTER_DIR",
    "WOLF_PARSERS_DIR",
    "FONTCONFIG_FILE",
    "XDG_DATA_DIRS",
];

/// Whether `haystack` contains `needle` as a byte substring.
fn contains_bytes(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The installed wrapper names every variable the running application needs, and
/// never names [`INIT_LUA_ENV`].
///
/// ## Why bytes, and not `read_to_string`
///
/// `wrapGAppsHook4` may produce either a `#!/bin/sh` wrapper script or a
/// `makeBinaryWrapper` ELF. The variable NAMES appear in either, and this test must
/// not encode which one the hook chose — a `read_to_string` would fail outright on
/// the binary form, reporting a UTF-8 error where the finding is "the package is
/// fine".
///
/// ## The premise, stated rather than assumed
///
/// The file read is `<out>/bin/wolf-gnome`, the WRAPPER. The wrapped application
/// binary — `<out>/bin/.wolf-gnome-wrapped` — contains the literal `WOLF_INIT_LUA` in
/// its read-only data whatever the wrapper does, because `app.rs` reads that variable
/// by name. So a test pointed at the wrapped binary would fire the last assertion on
/// a perfectly good package. If a future hook stops wrapping at all, the positive
/// assertions fire first, which is the right order: an unwrapped binary carries none
/// of the six.
///
/// ## Falsifiers
///
/// Delete one `--set-default` line from `gappsWrapperArgs`; assertion 1 names it. Add
/// `--set-default WOLF_INIT_LUA …`; assertion 2 fires.
///
/// Dropping `pkgs.gsettings-desktop-schemas` from `buildInputs` is NOT a falsifier for
/// this test and must not be recorded as one: `XDG_DATA_DIRS` would still be written
/// by the hook, one store path shorter, so this scan would stay green. That defect is
/// caught by `packaged_binary_renders_typed_text_under_cage`, where `adw::Application`
/// aborts on `activate` and the scenario exits 11 — a byte scan cannot tell a
/// `--prefix` with the right names from one with the right paths.
#[test]
#[ignore = "reads the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_wrapper_carries_every_variable_the_running_app_needs() {
    const TEST: &str = "the_packaged_wrapper_carries_every_variable_the_running_app_needs";

    let out = packaged_out(TEST);
    let wrapper = out.join(BIN_RELATIVE);
    let bytes = read_bytes(TEST, "the installed wrapper", &wrapper);

    // --- Assertion 1: every promised variable is named ------------------------
    let missing: Vec<&str> = REQUIRED_WRAPPER_VARS
        .iter()
        .copied()
        .filter(|name| !contains_bytes(&bytes, name))
        .collect();
    assert!(
        missing.is_empty(),
        "{TEST}: {} ({} bytes) names none of {missing:?}. Each absence has its own consequence \
         and none of them is an error message the user could act on. Without WOLF_NVIM_BIN the \
         app prints `failed to start core` and opens no window. Without WOLF_RG_BIN the project \
         search returns an already-complete session with zero matches — silently. Without \
         WOLF_NVIM_TREESITTER_DIR or WOLF_PARSERS_DIR the captive init.lua's treesitter gate is a \
         strict no-op, so injected fenced-language highlighting disappears from the editor and the \
         preview. Without FONTCONFIG_FILE the bundled editor faces are unresolvable. \
         XDG_DATA_DIRS is `wrapGAppsHook4`'s own contribution from `buildInputs` — and the way it \
         supplies the GSettings schemas, which is why no GSETTINGS_SCHEMA_DIR appears in this \
         list — so its absence means the hook did not run at all rather than that a \
         `--set-default` line was deleted; `adw::Application` then aborts on `activate`. Read as \
         BYTES on purpose: the hook may emit a shell script or a `makeBinaryWrapper` ELF, and the \
         names are present either way.",
        wrapper.display(),
        bytes.len()
    );

    // --- Assertion 2: the override is never wrapped in ------------------------
    assert!(
        !contains_bytes(&bytes, INIT_LUA_ENV),
        "{TEST}: {} names {INIT_LUA_ENV}. The wrapper must NEVER set it. Setting it makes \
         `init_lua::resolve_init_lua`'s executable-relative branch dead code — branch 1 always \
         wins — so the packaged app would stop resolving its own installed runtime, and it would \
         make the development override unusable on the packaged binary, since a `--set-default` \
         only yields to a variable the user exported and a plain `--set` would not yield at all. \
         Note which file this reads: `<out>/bin/wolf-gnome` is the WRAPPER. The wrapped binary \
         beside it carries this string in `.rodata` whatever the wrapper does, because `app.rs` \
         reads the variable by name, so a test aimed at `.wolf-gnome-wrapped` would fire here on a \
         perfectly good package.",
        wrapper.display()
    );
}

// ---------------------------------------------------------------------------
// Test 6: the two editor font families, and the additive fontconfig.

/// The four iA Writer Mono S faces the package ships — the family
/// `wolf_core::default_config()` names, so a package without them renders the default
/// configuration in a substituted face.
const IA_WRITER_FACES: [&str; 4] = [
    "iAWriterMonoS-Regular.ttf",
    "iAWriterMonoS-Italic.ttf",
    "iAWriterMonoS-Bold.ttf",
    "iAWriterMonoS-BoldItalic.ttf",
];

/// The filename prefix of the other family the shared `wolf-config.yaml` may name.
const JETBRAINS_PREFIX: &str = "JetBrainsMono";

/// The generated fontconfig fragment, relative to the store root.
const FONTS_CONF_RELATIVE: &str = "share/wolf/fonts.conf";

/// Both editor families ship, and the generated fontconfig is ADDITIVE rather than
/// exclusionary.
///
/// ## Why the `<include>` is asserted and not just the `<dir>`
///
/// `FONTCONFIG_FILE` replaces the ROOT configuration rather than extending it, so a
/// fragment naming only the app's own font directory would leave the running
/// application seeing two monospace families and nothing else — the GTK chrome would
/// render in an editor face on every host. That is exactly what the devShell's
/// `testdata/fonts.conf` does on purpose, for golden-render determinism, and shipping
/// that shape to a user is the mistake this assertion exists to catch. The
/// `<include>` of the system configuration is what keeps the host's own fonts,
/// aliases and rendering settings.
///
/// ## Falsifiers
///
/// Delete the JetBrains Mono copy from `postInstall`; assertion 2 fires. Delete the
/// `<include>` line from the generated `fonts.conf`; assertion 4 fires.
#[test]
#[ignore = "reads the installed Nix output; run through ./tools/wolf-package-smoke.sh"]
fn the_packaged_output_carries_both_editor_font_families() {
    const TEST: &str = "the_packaged_output_carries_both_editor_font_families";

    let out = packaged_out(TEST);
    let fonts_dir = out.join("share/fonts");
    let names = file_names_under(&fonts_dir);

    // --- Assertion 1: the four iA Writer Mono S faces -------------------------
    let missing: Vec<&str> = IA_WRITER_FACES
        .iter()
        .copied()
        .filter(|face| !names.iter().any(|n| n.as_str() == *face))
        .collect();
    assert!(
        missing.is_empty(),
        "{TEST}: {} carries none of {missing:?}; it holds {names:?}. iA Writer Mono S is the \
         family `wolf_core::default_config()` names, so a package missing these renders the \
         DEFAULT configuration — the one a first-run user gets — in whatever face fontconfig \
         substitutes. All four faces are required: dropping the italic or bold ones makes \
         emphasised text synthesise a slant or a weight instead of using the designed face.",
        fonts_dir.display()
    );

    // --- Assertion 2: at least one JetBrains Mono face ------------------------
    assert!(
        names.iter().any(|n| n.starts_with(JETBRAINS_PREFIX)),
        "{TEST}: {} carries no `{JETBRAINS_PREFIX}*` face; it holds {names:?}. The shared \
         `wolf-config.yaml` may name either editor family, so a user who configures JetBrains \
         Mono on a host that does not have it installed gets a substituted face from a package \
         that was supposed to carry it. The two file sets are disjoint, which is why both can \
         live in one directory.",
        fonts_dir.display()
    );

    // --- Assertion 3: the fragment exists -------------------------------------
    let conf = out.join(FONTS_CONF_RELATIVE);
    let bytes = read_bytes(TEST, "the generated fontconfig fragment", &conf);
    let text = String::from_utf8_lossy(&bytes).into_owned();

    // --- Assertion 4: it is ADDITIVE, and it names the app's own directory ----
    assert!(
        text.contains("<include"),
        "{TEST}: {} carries no `<include` element. FONTCONFIG_FILE replaces the ROOT \
         configuration rather than extending it, so without the include of the system config the \
         packaged application sees ONLY the two bundled editor families — the GTK chrome then \
         renders in a monospace editor face on every host, and no host font, alias or rendering \
         setting applies. That exclusionary shape is what the devShell's \
         `crates/wolf-gnome/testdata/fonts.conf` does deliberately, for golden-render \
         determinism; shipping it to a user is a different thing entirely. Its contents \
         were:\n{text}",
        conf.display()
    );
    let want_dir = fonts_dir.to_string_lossy().into_owned();
    assert!(
        text.contains(&want_dir),
        "{TEST}: {} does not name {want_dir:?} in any `<dir>` element. That directory is where \
         assertions 1 and 2 just found the bundled faces, so without it fontconfig never scans \
         them and both families are unresolvable in the packaged app however many files are \
         installed. Its contents were:\n{text}",
        conf.display()
    );
}
