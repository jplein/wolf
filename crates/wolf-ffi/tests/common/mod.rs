//! Shared helpers for `wolf-ffi` bridge integration tests.
//!
//! These tests drive a real, hermetic Neovim process (no mocks). The helpers the wolf-core and
//! wolf-gnome tests share live in the `wolf_test_support` crate and are re-exported here. Where
//! the wolf-core suite panics when it finds no nvim, the wolf-ffi suite loudly skips:
//! [`nvim_bin_or_skip`] returns `None` only on a host with neither the Bazel-staged
//! `//bazel/nvim:nvim_tree` nor a `WOLF_NVIM_BIN`, and every test in the suite then returns as
//! a no-op pass. It is also the state-dir funnel, so every test calls it before
//! `wolf_ffi_start`.
//!
//! The helpers defined below are wolf-ffi's own: the vendored-tree resolvers, which read their
//! variables through `wolf_test_support::resolve_test_runfile`, [`copy_dir_recursive`], and
//! [`Lighthouse`], the oracle that reads expected colours from the pinned `lighthouse.lua`.

#![allow(dead_code, unused_imports)] // Not every helper is used by every test binary.

use std::path::{Path, PathBuf};

use wolf_test_support::resolve_test_runfile;

pub use wolf_test_support::{
    fresh_cwd, init_lua_path, nvim_bin_or_skip, pin_default_shell, rg_bin_or_skip, DEFAULT_COLS,
    DEFAULT_ROWS,
};

/// Resolves the vendored orlando plugin tree from `WOLF_TEST_ORLANDO_DIR`, whose
/// inner content is `orlando/` (i.e. `<dir>/orlando/plugin/orlando.lua`). BOTH
/// supported environments stage that tree and set the variable: Bazel's runfiles on
/// macOS (`$(rootpath //bazel/orlando:orlando_tree)`, wired in the `rust_test`'s
/// `env`) and `flake.nix`'s `linuxShell.shellHook` on Linux (from the `wolfOrlando`
/// derivation, which builds the commit `bazel/orlando/extensions.bzl` pins).
///
/// `None` is what a run OUTSIDE both gets — a bare `cargo test` on a host with no
/// devShell — where `orlando_load` / `orlando_bg_stress` SKIP correctly; inside
/// either environment a `None` is a LOST EXPORT, silently turning those tests into
/// no-op passes.
/// `the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests` in
/// `crates/wolf-gnome/tests/flake_exports.rs` is the guard against that deletion.
/// Mirrors the `*_or_none` / `*_or_skip` convention of the other runfile helpers and
/// reuses the same [`resolve_test_runfile`] mechanism as [`init_lua_path`] /
/// [`nvim_bin_or_skip`].
#[must_use]
pub fn orlando_dir_or_none() -> Option<PathBuf> {
    resolve_test_runfile("WOLF_TEST_ORLANDO_DIR")
}

/// Resolves the vendored lighthouse colorscheme tree from `WOLF_TEST_LIGHTHOUSE_DIR`,
/// whose inner content is `lighthouse/` (i.e. `<dir>/lighthouse/colors/lighthouse.lua`).
/// BOTH supported environments stage that tree and set the variable: Bazel's runfiles
/// on macOS (`$(rootpath //bazel/lighthouse:lighthouse_tree)`, wired in the
/// `rust_test`'s `env`) and `flake.nix`'s `linuxShell.shellHook` on Linux (from the
/// `wolfLighthouse` derivation, which builds the commit
/// `bazel/lighthouse/extensions.bzl` pins).
///
/// `None` is what a run OUTSIDE both gets — a bare `cargo test` on a host with no
/// devShell — where `lighthouse_load` SKIPS correctly and `orlando_bg_stress` drops
/// its colorscheme overlay; inside either environment a `None` is a LOST EXPORT,
/// silently turning `lighthouse_load` into a no-op pass.
/// `the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests` in
/// `crates/wolf-gnome/tests/flake_exports.rs` is the guard against that deletion.
/// Mirrors the `*_or_none` / `*_or_skip` convention of the other runfile helpers and
/// reuses the same [`resolve_test_runfile`] mechanism as [`init_lua_path`] /
/// [`nvim_bin_or_skip`].
#[must_use]
pub fn lighthouse_dir_or_none() -> Option<PathBuf> {
    resolve_test_runfile("WOLF_TEST_LIGHTHOUSE_DIR")
}

/// Resolves a compiled treesitter-parsers tree from either platform's provider,
/// Bazel first:
///
/// 1. `WOLF_TEST_PARSERS_DIR` → `$(rootpath //bazel/treesitter-parsers:parsers_tree)`,
///    the `parsers_tree` TreeArtifact whose inner content is `parsers/` (i.e.
///    `<dir>/parsers/parser/go.so`).
/// 2. `WOLF_PARSERS_DIR`, the Linux devShell's export (`flake.nix`), a store path
///    whose content is flat (`<dir>/parser/go.so`). It is the same variable
///    `runtime/nvim/init.lua`'s gate falls back to.
///
/// **The Bazel variable wins, and the order is load-bearing.** Under Bazel the
/// staged runfiles are what the target's `data` guarantees and what its assertions
/// were written against, so a stray devShell variable inherited into a Bazel
/// invocation must never shadow them. On the cargo path `WOLF_TEST_PARSERS_DIR` is
/// never set, so the second arm is reached unconditionally.
///
/// Returns `None` only when NEITHER resolves — a host with neither the Bazel
/// staging nor the devShell exports — so `treesitter_parser_abi` SKIPS gracefully
/// there rather than failing. Mirrors the `*_or_none` / `*_or_skip` convention of
/// the other runfile helpers and reuses the same [`resolve_test_runfile`] mechanism as
/// [`init_lua_path`] / [`nvim_bin_or_skip`]; that helper returns an absolute
/// existing path unchanged, which is what lets a `/nix/store/…` value work with no
/// change to it.
#[must_use]
pub fn parsers_dir_or_none() -> Option<PathBuf> {
    resolve_test_runfile("WOLF_TEST_PARSERS_DIR")
        .or_else(|| resolve_test_runfile("WOLF_PARSERS_DIR"))
}

/// Resolves a vendored nvim-treesitter (main queries) tree from either platform's
/// provider, Bazel first:
///
/// 1. `WOLF_TEST_NVIM_TREESITTER_DIR` →
///    `$(rootpath //bazel/nvim-treesitter:nvim_treesitter_tree)`, the
///    `nvim_treesitter_tree` TreeArtifact whose inner content is `nvim-treesitter/`
///    (i.e. `<dir>/nvim-treesitter/queries/markdown/injections.scm`).
/// 2. `WOLF_NVIM_TREESITTER_DIR`, the Linux devShell's export (`flake.nix`),
///    pointing at the flake's assembled runtimepath-shaped nvim-treesitter tree,
///    with `queries/`, `lua/` and `plugin/` as siblings (i.e.
///    `<dir>/queries/markdown/injections.scm`). It is the same variable
///    `runtime/nvim/init.lua`'s gate falls back to. Its revision comes from the
///    flake's nixpkgs, a pin separate from Bazel's that is matched to it by hand,
///    so the two queries trees diverge only when one pin is bumped without the
///    other — see `bazel/nvim-treesitter/README.md#the-pin`.
///
/// **The Bazel variable wins, and the order is load-bearing** — see
/// [`parsers_dir_or_none`] for why. Callers must accommodate both layouts: probe
/// for an inner `nvim-treesitter/` and fall back to the directory itself.
///
/// Returns `None` only when NEITHER resolves — a host with neither the Bazel
/// staging nor the devShell exports — so `treesitter_highlight` SKIPS gracefully
/// there rather than failing. Mirrors the `*_or_none` / `*_or_skip` convention of
/// the other runfile helpers and reuses the same [`resolve_test_runfile`] mechanism as
/// [`init_lua_path`] / [`nvim_bin_or_skip`].
#[must_use]
pub fn nvim_treesitter_dir_or_none() -> Option<PathBuf> {
    resolve_test_runfile("WOLF_TEST_NVIM_TREESITTER_DIR")
        .or_else(|| resolve_test_runfile("WOLF_NVIM_TREESITTER_DIR"))
}

/// Recursively copies the directory tree at `src` into `dst` (creating `dst` and
/// any parent dirs), preserving the relative layout.
///
/// Used by the `orlando_load` test to stage a sibling `orlando/` next to a temp
/// `init.lua`, because the captive `init.lua` self-locates the plugin relative to
/// ITS OWN directory. Entries are classified with [`std::fs::metadata`] (which
/// FOLLOWS symlinks) rather than [`std::fs::DirEntry::file_type`], because under
/// Bazel the runfiles entries are symlinks into the output tree and would
/// otherwise be skipped; [`std::fs::copy`] likewise follows the source symlink to
/// copy the real file content.
pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        // metadata() follows symlinks so a symlinked dir is recursed into and a
        // symlinked file is copied by content (correct for Bazel runfiles).
        let meta = std::fs::metadata(&from)?;
        if meta.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// One of Lighthouse's two variants, as `vim.o.background` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Dark,
    Light,
}

impl Background {
    /// The variant's name, which is also the name of its palette table in `lighthouse.lua`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// The expected colours of the pinned Lighthouse, read from its `colors/lighthouse.lua` so a
/// test needs no constants a pin bump would leave stale.
///
/// It reads three shapes of the file, and panics naming the file and the missing piece when
/// one is absent:
///
/// - each palette is the `local dark = {` or `local light = {` table, one `key = "#rrggbb",` per
///   line, closed by a line that is exactly `}`; other lines inside, such as comments, are
///   skipped;
/// - the ANSI order is the `local ansi = {` list, `[0] = p.<key>,` then fifteen `p.<key>,`
///   lines, closed by `}`, which the file assigns to `g:terminal_color_0` to `_15`;
/// - colours are packed `0x00RRGGBB`, as `FfiColor.rgb` holds them.
pub struct Lighthouse {
    path: PathBuf,
    text: String,
}

impl Lighthouse {
    /// Reads the colorscheme file of a staged tree: `<tree>/lighthouse/colors/lighthouse.lua`
    /// when the inner `lighthouse/` of Bazel's TreeArtifact exists, else the flat
    /// `<tree>/colors/lighthouse.lua` of the devShell's `wolfLighthouse`.
    #[must_use]
    pub fn load(tree: &Path) -> Self {
        let inner = tree.join("lighthouse");
        let root = if inner.is_dir() {
            inner
        } else {
            tree.to_path_buf()
        };
        let path = root.join("colors").join("lighthouse.lua");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "failed to read the pinned Lighthouse colorscheme at {}: {e}",
                path.display()
            )
        });
        Self { path, text }
    }

    /// The lines strictly between the line `opener` and the next line that is exactly `}`.
    fn table(&self, opener: &str, what: &str) -> Vec<&str> {
        let mut lines = self.text.lines();
        if !lines.any(|line| line.trim_end() == opener) {
            panic!(
                "{} has no `{opener}` line, so it has no {what}",
                self.path.display()
            );
        }
        lines.take_while(|line| line.trim_end() != "}").collect()
    }

    /// The `background` palette's colour for `key`, such as `cursorBG` or `ansiRed`.
    #[must_use]
    pub fn colour(&self, background: Background, key: &str) -> u32 {
        let opener = format!("local {} = {{", background.name());
        let what = format!("{} palette", background.name());
        let prefix = format!("{key} = \"#");
        let hex = self
            .table(&opener, &what)
            .into_iter()
            .find_map(|line| line.trim().strip_prefix(prefix.as_str()))
            .and_then(|rest| rest.strip_suffix("\","))
            .unwrap_or_else(|| {
                panic!(
                    "{}'s {what} has no `{key} = \"#rrggbb\",` line",
                    self.path.display()
                )
            });
        u32::from_str_radix(hex, 16).unwrap_or_else(|e| {
            panic!(
                "{}'s {what} gives {key} the colour #{hex}, which is not hex: {e}",
                self.path.display()
            )
        })
    }

    /// The palette keys of `g:terminal_color_0` to `_15`, in order.
    #[must_use]
    pub fn ansi_keys(&self) -> [String; 16] {
        let mut keys = Vec::new();
        let opener = "local ansi = {";
        if !self.text.lines().any(|line| line.trim_end() == opener) {
            panic!(
                "{} has no `{opener}` list, so this Lighthouse sets no g:terminal_color_N and a \
                 terminal pane keeps libghostty-vt's built-in palette",
                self.path.display()
            );
        }
        for line in self.table(opener, "ANSI list") {
            let item = line.trim();
            if item.is_empty() || item.starts_with("--") {
                continue;
            }
            let item = item.strip_prefix("[0] = ").unwrap_or(item);
            let key = item
                .strip_prefix("p.")
                .and_then(|rest| rest.strip_suffix(','))
                .unwrap_or_else(|| {
                    panic!(
                        "{}'s ANSI list has the line {line:?}, not `p.<key>,`",
                        self.path.display()
                    )
                });
            keys.push(key.to_owned());
        }
        keys.try_into().unwrap_or_else(|keys: Vec<String>| {
            panic!(
                "{}'s ANSI list names {} colours, not 16: {keys:?}",
                self.path.display(),
                keys.len()
            )
        })
    }

    /// The colours of `g:terminal_color_0` to `_15` for `background`.
    #[must_use]
    pub fn ansi(&self, background: Background) -> [u32; 16] {
        self.ansi_keys().map(|key| self.colour(background, &key))
    }
}
