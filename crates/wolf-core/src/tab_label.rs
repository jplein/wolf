//! Pure per-pane TAB-LABEL display-string math (plan §2).
//!
//! =============================================================================
//! CODER: THIS FILE IS A TEST-WRITER STUB. It currently holds ONLY the
//! `#[cfg(test)] mod tests` contract block. You must ADD, ABOVE this test module,
//! the production surface the plan (§2) describes, and wire the module up:
//!
//!   * `pub struct TabLabel { pub bufnr: i64, pub display: String,
//!      pub active: bool, pub modified: bool }`
//!   * `pub fn tab_labels(paths: &[String], cwd: Option<&Path>,
//!      home: Option<&Path>) -> Vec<String>`
//!   * a `use std::path::Path;` (and `use crate::pane_status::NO_NAME;` if you
//!     render the empty-name case through it)
//!   * add `pub mod tab_label;` + the `TabLabel` / `tab_labels` re-export to
//!     `crates/wolf-core/src/lib.rs`.
//!
//! Until then this file will NOT compile (the tests reference undefined items) —
//! that is EXPECTED, exactly like the not-yet-implemented wolf-ffi bridge tests.
//! =============================================================================
//!
//! `tab_labels` mirrors `pane_status::pane_status`'s path precedence: it draws no
//! pixels and touches no filesystem, operating over ALREADY-CANONICAL inputs. The
//! base case is each path's basename; a same-basename collision is disambiguated by
//! prepending parent directories until unique, ultimately falling back to the FULL
//! `pane_status`-style display (cwd-relative, else `~`-prefixed home, else the raw
//! absolute path). An empty path renders as `pane_status::NO_NAME` (`[No Name]`).

use std::path::Path;

use crate::pane_status::{self, NO_NAME};

/// One pane tab's display data: the buffer handle, the disambiguated `display`
/// string, and the active/modified flags (plan §2).
///
/// Assembled by [`crate::api::WolfCore::pane_tabs`] from a pane's
/// [`PaneTabs`](crate::render::PaneTabs) using [`tab_labels`] for the display
/// strings. The front-end reads these fields to paint each tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabLabel {
    /// The Neovim buffer handle this tab shows.
    pub bufnr: i64,
    /// The disambiguated display string (see [`tab_labels`]).
    pub display: String,
    /// Whether this tab is the pane's active tab.
    pub active: bool,
    /// Whether the buffer has unsaved modifications.
    pub modified: bool,
}

/// Computes the disambiguated per-tab display strings for a pane's buffer
/// `paths`, given the (already-canonical) `cwd` and `home` directories (plan §2).
///
/// This is IO-free string/path arithmetic over ALREADY-CANONICAL inputs (like
/// [`pane_status`](crate::pane_status)); it draws no pixels and touches no
/// filesystem. The returned vector is parallel to `paths`.
///
/// Algorithm:
/// - An empty path (unnamed buffer) renders as [`NO_NAME`].
/// - The base case for a named path is its `file_name()` (basename).
/// - When two-or-more named paths share a basename, each colliding path is
///   disambiguated by the MINIMAL parent-suffix of that path's `pane_status`-style
///   display (cwd-relative, else `~`-home, else absolute), expanded ONE parent
///   component at a time until its label is unique among all labels. If expansion
///   reaches the full display, that full `pane_status` display is used.
#[must_use]
pub fn tab_labels(paths: &[String], cwd: Option<&Path>, home: Option<&Path>) -> Vec<String> {
    // Per-path expansion ladders, smallest (basename) rung first. An empty path
    // (unnamed buffer) has a single NO_NAME rung and never expands.
    let ladders: Vec<Vec<String>> = paths
        .iter()
        .map(|p| {
            if p.is_empty() {
                vec![NO_NAME.to_string()]
            } else {
                let display = pane_status::pane_status(p, cwd, home).display_path;
                build_ladder(&display)
            }
        })
        .collect();

    // The current rung index for each path (0 = basename). Bumped one at a time
    // for colliding labels until every label is unique or fully expanded.
    let mut level: Vec<usize> = vec![0; paths.len()];
    let label_at = |ladders: &[Vec<String>], level: &[usize], i: usize| -> String {
        ladders[i][level[i]].clone()
    };

    loop {
        // Find the label strings that appear more than once.
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for i in 0..paths.len() {
            *counts.entry(label_at(&ladders, &level, i)).or_insert(0) += 1;
        }
        let colliding: std::collections::HashSet<String> = counts
            .into_iter()
            .filter(|&(_, n)| n > 1)
            .map(|(l, _)| l)
            .collect();
        if colliding.is_empty() {
            break;
        }

        let mut changed = false;
        for i in 0..paths.len() {
            if level[i] + 1 >= ladders[i].len() {
                continue; // Already fully expanded.
            }
            if colliding.contains(&label_at(&ladders, &level, i)) {
                level[i] += 1;
                changed = true;
            }
        }
        if !changed {
            break; // No colliding label can grow further.
        }
    }

    (0..paths.len())
        .map(|i| label_at(&ladders, &level, i))
        .collect()
}

/// Builds the expansion ladder for a `pane_status` display string: the smallest
/// rung (the basename) first, then one added parent directory per rung, and
/// finally the WHOLE `display` (which restores any `/` or `~` root prefix).
///
/// The intermediate rungs omit the root prefix (`src/mod.rs`), so a colliding
/// label grows a parent at a time; only the LAST rung is the full `pane_status`
/// display (`/opt/a/b/mod.rs`, `~/a/b/mod.rs`, or the already-prefix-free
/// cwd-relative form).
fn build_ladder(display: &str) -> Vec<String> {
    let segs: Vec<&str> = display.split('/').collect();
    // The directory segments live between the (optional) root prefix and the
    // basename. Absolute displays start with an empty segment (leading `/`);
    // home displays start with `~`; cwd-relative displays have no root prefix.
    let dir_start = if display.starts_with('/') || segs.first() == Some(&"~") {
        1
    } else {
        0
    };
    let basename_idx = segs.len() - 1;
    // A single-segment display — a bare `~` (a buffer path equal to home), a bare
    // basename, root `/`, or a bare relative name — has no parent interior to
    // expand: `segs[dir_start..basename_idx]` would be an empty-or-inverted range
    // (e.g. `segs[1..0]` for `"~"`, which PANICS across the synchronous Swift paint
    // FFI boundary — BLOCKER #1). There is nothing to disambiguate, so return the
    // display verbatim as the sole rung.
    if basename_idx <= dir_start {
        return vec![display.to_string()];
    }
    let dirs = &segs[dir_start..basename_idx];
    let basename = segs[basename_idx];

    let mut ladder = vec![basename.to_string()];
    // Intermediate rungs: last `i` dirs + basename, WITHOUT the root prefix.
    for i in 1..dirs.len() {
        let start = dirs.len() - i;
        ladder.push(format!("{}/{}", dirs[start..].join("/"), basename));
    }
    // The final rung is the whole display (adds back the root prefix, if any).
    if ladder.last().map(String::as_str) != Some(display) {
        ladder.push(display.to_string());
    }
    ladder
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    // ------------------------------------------------------------------------
    // CONTRACT / ASSUMPTIONS (test-writer — coder must reconcile)
    //
    // The two clearly-specified cases below (unique -> basename; a single
    // basename collision -> ONE parent prefix) are pinned to exact values, as is
    // the empty-path `[No Name]` case.
    //
    // The DEEPER-collision fallbacks (`deep_*`) encode the most sensible reading
    // of plan §2 that is ALSO consistent with the `collision_prepends_one_parent`
    // example: a colliding label is the MINIMAL parent-suffix OF THAT PATH'S
    // `pane_status` display (cwd-relative, else `~`-home, else absolute), expanded
    // one parent at a time until unique — so when expansion reaches the display's
    // root you get the whole `pane_status` string. If the coder's disambiguation
    // differs, reconcile the impl or these `deep_*` expectations (see report).
    // ------------------------------------------------------------------------

    /// Builds the label vector from string-slice paths and optional cwd/home,
    /// adapting to the `tab_labels(&[String], Option<&Path>, Option<&Path>)`
    /// signature (plan §2).
    fn labels(paths: &[&str], cwd: Option<&str>, home: Option<&str>) -> Vec<String> {
        let owned: Vec<String> = paths.iter().map(|s| (*s).to_string()).collect();
        tab_labels(&owned, cwd.map(Path::new), home.map(Path::new))
    }

    /// Distinct basenames need no disambiguation — each label is just the
    /// basename, regardless of the differing parent directories.
    #[test]
    fn unique_basenames_are_bare() {
        let got = labels(
            &["/proj/src/main.rs", "/proj/docs/readme.md"],
            Some("/proj"),
            None,
        );
        assert_eq!(
            got,
            vec!["main.rs".to_string(), "readme.md".to_string()],
            "distinct basenames render as bare basenames"
        );
    }

    /// The plan's explicit collision example: two `mod.rs` under DIFFERENT
    /// immediate parents disambiguate by prepending exactly one parent segment
    /// (`src/mod.rs` vs `lib/mod.rs`) — with no cwd/home, this proves the prefix
    /// is a minimal parent trim, NOT the full absolute path.
    #[test]
    fn collision_prepends_one_parent() {
        let got = labels(&["/a/src/mod.rs", "/b/lib/mod.rs"], None, None);
        assert_eq!(
            got,
            vec!["src/mod.rs".to_string(), "lib/mod.rs".to_string()],
            "a same-basename collision prepends one disambiguating parent"
        );
    }

    /// A deeper collision (same basename AND same immediate parent) under cwd
    /// keeps prepending parents; the result coincides with the cwd-relative
    /// remainder from `pane_status`'s first precedence branch.
    #[test]
    fn deep_collision_falls_back_to_cwd_relative() {
        let got = labels(
            &["/proj/a/util/mod.rs", "/proj/b/util/mod.rs"],
            Some("/proj"),
            None,
        );
        assert_eq!(
            got,
            vec!["a/util/mod.rs".to_string(), "b/util/mod.rs".to_string()],
            "a deep collision under cwd resolves to the cwd-relative remainders"
        );
    }

    /// A collision that can only be resolved by expanding PAST the cwd boundary
    /// falls back to the `~`-home form for the home-rooted path and the
    /// cwd-relative form for the cwd-rooted one — the second precedence branch of
    /// `pane_status`. Both share the entire `a/b/mod.rs` suffix, so shallow trims
    /// keep colliding until each expands to its full `pane_status` display.
    #[test]
    fn deep_collision_falls_back_to_home_tilde() {
        let got = labels(
            &["/home/user/a/b/mod.rs", "/proj/a/b/mod.rs"],
            Some("/proj"),
            Some("/home/user"),
        );
        assert_eq!(
            got,
            vec!["~/a/b/mod.rs".to_string(), "a/b/mod.rs".to_string()],
            "a home-rooted path that collides past cwd renders with the ~ prefix"
        );
    }

    /// REGRESSION (BLOCKER #1): a single buffer whose path IS the home directory
    /// renders as the bare `~` — matching `pane_status`'s empty-home-remainder
    /// branch (`home_relative` returns `"~"` when `path == home`). That display has
    /// ONE segment (`["~"]`) and nothing to disambiguate, so the ladder must NOT try
    /// to slice a parent-directory range: the unguarded `build_ladder` computed
    /// `segs[dir_start..basename_idx]` == `segs[1..0]` and PANICKED (`:e ~`, reachable
    /// on the synchronous Swift paint path). This pins the single-segment guard.
    #[test]
    fn home_exact_path_is_tilde() {
        let got = labels(&["/home/user"], Some("/home/user"), Some("/home/user"));
        assert_eq!(
            got,
            vec!["~".to_string()],
            "a buffer path equal to home renders as a bare ~ (single segment, no expansion)"
        );
    }

    /// COMPANION to `home_exact_path_is_tilde`: the bare-`~` home-exact entry
    /// coexists with a normal cwd-relative child under home without disturbing the
    /// disambiguation loop — the home dir stays `~` while the file under home keeps
    /// its basename. Confirms the single-segment guard does not break the ordinary
    /// (multi-segment) path.
    #[test]
    fn home_exact_path_coexists_with_home_child() {
        let got = labels(
            &["/home/user", "/home/user/notes.md"],
            Some("/home/user"),
            Some("/home/user"),
        );
        assert_eq!(
            got,
            vec!["~".to_string(), "notes.md".to_string()],
            "a home-exact ~ tab coexists with a home child's basename (guard is inert here)"
        );
    }

    /// A collision resolvable only by expanding past cwd, where one path lives
    /// under NEITHER cwd nor home, falls back to the raw absolute path — the
    /// third/last precedence branch of `pane_status`.
    #[test]
    fn deep_collision_falls_back_to_absolute() {
        let got = labels(
            &["/proj/a/b/mod.rs", "/opt/a/b/mod.rs"],
            Some("/proj"),
            None,
        );
        assert_eq!(
            got,
            vec!["a/b/mod.rs".to_string(), "/opt/a/b/mod.rs".to_string()],
            "a path under neither cwd nor home falls back to its absolute display"
        );
    }

    /// An empty buffer path (unnamed buffer) renders as the shared
    /// `pane_status::NO_NAME` placeholder, exactly like the statusbar.
    #[test]
    fn empty_path_is_no_name() {
        let got = labels(&[""], None, None);
        assert_eq!(
            got,
            vec![crate::pane_status::NO_NAME.to_string()],
            "an empty buffer path renders as the NO_NAME placeholder"
        );
    }

    /// A mix of a named and an unnamed buffer keeps the placeholder for the empty
    /// entry while the named one keeps its basename (no spurious collision).
    #[test]
    fn empty_path_mixes_with_named() {
        let got = labels(&["/proj/src/main.rs", ""], Some("/proj"), None);
        assert_eq!(
            got,
            vec!["main.rs".to_string(), crate::pane_status::NO_NAME.to_string()],
            "an unnamed buffer keeps NO_NAME alongside a named buffer's basename"
        );
    }

    /// Pins the `TabLabel` field contract from plan §2 (`bufnr: i64`,
    /// `display: String`, `active: bool`, `modified: bool`). If the coder chooses
    /// different field names/types, this and the FFI `pane_tabs` mapping must move
    /// together.
    #[test]
    fn tab_label_struct_has_expected_fields() {
        let t = TabLabel {
            bufnr: 7,
            display: "mod.rs".to_string(),
            active: true,
            modified: false,
        };
        assert_eq!(t.bufnr, 7, "bufnr");
        assert_eq!(t.display, "mod.rs", "display");
        assert!(t.active, "active");
        assert!(!t.modified, "modified");
    }
}
