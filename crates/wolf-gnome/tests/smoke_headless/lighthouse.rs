//! The expected colours of the pinned Lighthouse, read from its `colors/lighthouse.lua` in the
//! tree `WOLF_TEST_LIGHTHOUSE_DIR` names, so a smoke needs no colour constant a pin bump would
//! leave stale. The Rust copy in `crates/wolf-ffi/tests/common/mod.rs` reads the same shapes.
//!
//! It reads three shapes of the file, and panics naming the file and the missing piece when
//! one is absent:
//!
//! - each palette is the `local dark = {` or `local light = {` table, one `key = "#rrggbb",` per
//!   line, closed by a line that is exactly `}`; other lines inside, such as comments, are
//!   skipped;
//! - the ANSI order is the `local ansi = {` list, `[0] = p.<key>,` then fifteen `p.<key>,`
//!   lines, closed by `}`, which the file assigns to `g:terminal_color_0` to `_15`;
//! - colours are packed `0xRRGGBB`.
//!
//! The variable is read in the test process only: the packaged smokes scrub it from the cage,
//! and nothing is read out of a capture, which would compare the app under test with itself.

use std::path::{Path, PathBuf};

use super::Rgb8;

/// The devShell's export of the vendored Lighthouse tree, from `wolfLighthouse`.
pub(crate) const LIGHTHOUSE_DIR_ENV: &str = "WOLF_TEST_LIGHTHOUSE_DIR";

/// One of Lighthouse's two variants, as `vim.o.background` and the config's `appearance` name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Background {
    Dark,
    Light,
}

impl Background {
    /// The variant's name, which is also the name of its palette table in `lighthouse.lua`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// The pinned `lighthouse.lua`, read once.
pub(crate) struct Lighthouse {
    path: PathBuf,
    text: String,
}

impl Lighthouse {
    /// Reads the colorscheme of the tree [`LIGHTHOUSE_DIR_ENV`] names. Panics when the variable
    /// is unset or names no tree: the devShell always exports it, so either is a lost export.
    pub(crate) fn from_env(smoke: &str) -> Self {
        let tree = std::env::var_os(LIGHTHOUSE_DIR_ENV)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                panic!(
                    "{smoke}: {LIGHTHOUSE_DIR_ENV} is unset or empty. The Linux devShell exports \
                     it from `wolfLighthouse`, the commit `flake.nix`'s `wolfVendoredPins` pins, \
                     and this smoke reads its expected colours from that tree; run it inside the \
                     devShell."
                )
            });
        Self::load(&tree)
    }

    /// Reads `<tree>/lighthouse/colors/lighthouse.lua` when the inner `lighthouse/` exists,
    /// else the flat `<tree>/colors/lighthouse.lua` of the devShell's tree.
    pub(crate) fn load(tree: &Path) -> Self {
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

    /// The colorscheme file read.
    pub(crate) fn path(&self) -> &Path {
        &self.path
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

    /// The `background` palette's colour for `key`, such as `background` or `ansiRed`.
    pub(crate) fn colour(&self, background: Background, key: &str) -> u32 {
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

    /// [`Self::colour`] as the `(r, g, b)` a capture's pixel is compared with.
    pub(crate) fn rgb8(&self, background: Background, key: &str) -> Rgb8 {
        rgb8(self.colour(background, key))
    }

    /// The palette keys of `g:terminal_color_0` to `_15`, in order.
    pub(crate) fn ansi_keys(&self) -> [String; 16] {
        let opener = "local ansi = {";
        if !self.text.lines().any(|line| line.trim_end() == opener) {
            panic!(
                "{} has no `{opener}` list, so this Lighthouse sets no g:terminal_color_N and a \
                 terminal pane keeps libghostty-vt's built-in palette",
                self.path.display()
            );
        }
        let mut keys = Vec::new();
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
    pub(crate) fn ansi(&self, background: Background) -> [u32; 16] {
        self.ansi_keys().map(|key| self.colour(background, &key))
    }
}

/// A packed `0xRRGGBB` colour as `(r, g, b)`.
pub(crate) fn rgb8(packed: u32) -> Rgb8 {
    let [_, r, g, b] = packed.to_be_bytes();
    (r, g, b)
}
