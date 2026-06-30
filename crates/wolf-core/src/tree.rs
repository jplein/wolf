//! Pure, toolkit-free hierarchical projection of the picker's flat file list.
//!
//! The file picker (`wolf-core`'s `FileSearch`) produces a *flat* list of
//! [`crate::FileEntry`] values, each carrying a relative, `/`-separated
//! `display` path and an absolute `path`. The sidebar tree, however, is a
//! *hierarchy*: directories that contain (transitively) listed files, with files
//! as leaves. This module holds the pure projection from the flat list into the
//! per-directory children that a lazy outline/tree view needs, plus the "which
//! ancestor dirs must be expanded to reveal this file" helper used when the
//! current buffer changes.
//!
//! Keeping this toolkit-free is what lets it be unit-tested headlessly, exactly
//! mirroring how `rank.rs` and `layout.rs` keep their decisions pure and
//! testable with no display server, no widget, and no live Neovim. The platform
//! tree views (`wolf-gnome`'s `tree_view.rs`, `wolf-macos`'s `FileTreeView`) are
//! the only places that wrap these results in widgets.

/// One immediate child of a directory in the projected tree: either a
/// sub-directory (`Dir`) or a leaf file (`File`).
///
/// `rel_path` is the full relative, `/`-separated path from the cwd root to this
/// node (so a directory `b` under `a` has `name = "b"`, `rel_path = "a/b"`). A
/// `File`'s `rel_path` equals the originating [`crate::FileEntry::display`],
/// and its `abs_path` is that entry's absolute `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeNode {
    /// A directory that (transitively) contains at least one listed file.
    Dir {
        /// The immediate component name (last segment of `rel_path`).
        name: String,
        /// The full relative path from the cwd root to this directory.
        rel_path: String,
    },
    /// A leaf file.
    File {
        /// The immediate component name (file name).
        name: String,
        /// The full relative path from the cwd root (== the entry's `display`).
        rel_path: String,
        /// The absolute filesystem path (== the entry's `path`).
        abs_path: std::path::PathBuf,
    },
}

/// Projects the flat file list into the IMMEDIATE children of `dir`.
///
/// `dir == ""` means the cwd root. Each [`crate::FileEntry::display`] is a
/// relative, `/`-separated path; an entry contributes a child of `dir` iff it
/// lives under `dir`. The component immediately after `dir` becomes either a
/// `File` (when it is the entry's final component) or a `Dir` (when more
/// components follow). Directory nodes are deduped by `rel_path` (many files may
/// imply one dir); files are unique but deduped defensively. The result is
/// sorted directories-first, then files, each group case-insensitively by `name`
/// with `rel_path` as a deterministic tie-break.
#[must_use]
pub fn children_of(entries: &[crate::FileEntry], dir: &str) -> Vec<TreeNode> {
    let mut nodes: Vec<TreeNode> = Vec::new();

    for entry in entries {
        let display = entry.display.as_str();

        // Determine `rest`: the portion of `display` below `dir`, and `comp0`,
        // the first component of `rest`.
        let rest = if dir.is_empty() {
            display
        } else {
            let prefix = format!("{dir}/");
            match display.strip_prefix(&prefix) {
                Some(r) => r,
                None => continue,
            }
        };
        if rest.is_empty() {
            continue;
        }

        let (comp0, tail) = match rest.split_once('/') {
            Some((head, tail)) => (head, Some(tail)),
            None => (rest, None),
        };

        let child_rel = if dir.is_empty() {
            comp0.to_string()
        } else {
            format!("{dir}/{comp0}")
        };

        let node = if tail.is_none() {
            // `comp0` is the entry's final component => a file.
            TreeNode::File {
                name: comp0.to_string(),
                rel_path: child_rel,
                abs_path: entry.path.clone(),
            }
        } else {
            // More components follow => an intermediate directory.
            TreeNode::Dir {
                name: comp0.to_string(),
                rel_path: child_rel,
            }
        };

        if !nodes.iter().any(|n| node_rel_path(n) == node_rel_path(&node)) {
            nodes.push(node);
        }
    }

    nodes.sort_by(|a, b| {
        let a_is_dir = matches!(a, TreeNode::Dir { .. });
        let b_is_dir = matches!(b, TreeNode::Dir { .. });
        // Directories first.
        b_is_dir.cmp(&a_is_dir).then_with(|| {
            let (a_name, a_rel) = node_name_rel(a);
            let (b_name, b_rel) = node_name_rel(b);
            a_name
                .to_ascii_lowercase()
                .cmp(&b_name.to_ascii_lowercase())
                .then_with(|| a_rel.cmp(b_rel))
        })
    });

    nodes
}

/// Returns the ancestor directory rel-paths of `rel_path`, from the cwd root
/// down to the node's parent.
///
/// `"a/b/c.md"` => `["a", "a/b"]`; `"c.md"` (no `/`) => `[]`; `""` => `[]`.
#[must_use]
pub fn ancestors_to_reveal(rel_path: &str) -> Vec<String> {
    if rel_path.is_empty() {
        return Vec::new();
    }

    let components: Vec<&str> = rel_path.split('/').collect();
    // Drop the final component (the node itself); accumulate prefixes of the
    // rest.
    let mut result = Vec::new();
    let mut prefix = String::new();
    for component in &components[..components.len().saturating_sub(1)] {
        if prefix.is_empty() {
            prefix.push_str(component);
        } else {
            prefix.push('/');
            prefix.push_str(component);
        }
        result.push(prefix.clone());
    }
    result
}

/// Returns the `rel_path` of either [`TreeNode`] variant.
#[must_use]
pub fn node_rel_path(node: &TreeNode) -> &str {
    match node {
        TreeNode::Dir { rel_path, .. } | TreeNode::File { rel_path, .. } => rel_path,
    }
}

/// Helper: the `(name, rel_path)` pair of either variant, for sorting.
fn node_name_rel(node: &TreeNode) -> (&str, &str) {
    match node {
        TreeNode::Dir { name, rel_path } | TreeNode::File { name, rel_path, .. } => {
            (name, rel_path)
        }
    }
}

/// Maps an absolute path to its tree-display string by finding the FIRST
/// [`crate::FileEntry`] whose `path` equals `abs` exactly, returning a clone of
/// that entry's `display`. Returns `None` if no entry matches.
///
/// The match is EXACT: a prefix of an entry's path (e.g. a parent directory) does
/// not match. This is the current-file -> tree-display projection used by the
/// sidebar's reveal logic.
#[must_use]
pub fn rel_path_for(entries: &[crate::FileEntry], abs: &std::path::Path) -> Option<String> {
    entries
        .iter()
        .find(|entry| entry.path == abs)
        .map(|entry| entry.display.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::{Path, PathBuf};

    use crate::FileEntry;

    // -- test helper -------------------------------------------------------
    //
    // Builds a `FileEntry` from a single `display` string. CONVENTION: the
    // absolute `path` is synthesized as `/root/{display}`, so a File node's
    // `abs_path` is exactly `PathBuf::from(format!("/root/{display}"))`. The MRU
    // rank is irrelevant to the projection, so it is always `None`.
    fn fe(display: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(format!("/root/{display}")),
            display: display.into(),
            mru_rank: None,
        }
    }

    /// Convenience: the synthesized absolute path for a `display`, matching the
    /// `fe()` convention, so File-node `abs_path` assertions read clearly.
    fn abs(display: &str) -> PathBuf {
        PathBuf::from(format!("/root/{display}"))
    }

    /// Builds the expected Dir node for `rel_path` (its `name` is the final
    /// component of `rel_path`).
    fn dir(rel_path: &str) -> TreeNode {
        let name = rel_path.rsplit('/').next().unwrap_or(rel_path).to_string();
        TreeNode::Dir {
            name,
            rel_path: rel_path.to_string(),
        }
    }

    /// Builds the expected File node for `rel_path` (== display), with `name` the
    /// final component and `abs_path` per the `fe()` convention.
    fn file(rel_path: &str) -> TreeNode {
        let name = rel_path.rsplit('/').next().unwrap_or(rel_path).to_string();
        TreeNode::File {
            name,
            rel_path: rel_path.to_string(),
            abs_path: abs(rel_path),
        }
    }

    // =====================================================================
    // children_of
    // =====================================================================

    #[test]
    fn children_of_root_lists_top_level_dirs_first_then_files() {
        let entries = [
            fe("readme.md"),
            fe("src/main.rs"),
            fe("src/lib.rs"),
            fe("Cargo.toml"),
            fe("docs/intro.md"),
        ];

        let got = children_of(&entries, "");

        // Dirs FIRST (case-insensitive name: "docs" < "src"), then files
        // (case-insensitive: "Cargo.toml" -> 'c' < 'r' <- "readme.md").
        let want = vec![dir("docs"), dir("src"), file("Cargo.toml"), file("readme.md")];
        assert_eq!(got, want);
    }

    #[test]
    fn children_of_nested_dir() {
        let entries = [
            fe("readme.md"),
            fe("src/main.rs"),
            fe("src/lib.rs"),
            fe("Cargo.toml"),
            fe("docs/intro.md"),
        ];

        let got = children_of(&entries, "src");

        // Both are leaves under "src", ordered case-insensitively by name:
        // "lib.rs" < "main.rs".
        let want = vec![file("src/lib.rs"), file("src/main.rs")];
        assert_eq!(got, want);

        // Pin the abs_path convention explicitly on a File node.
        match &got[0] {
            TreeNode::File { abs_path, .. } => {
                assert_eq!(*abs_path, abs("src/lib.rs"), "File abs_path == /root/{{display}}");
            }
            other => panic!("expected a File node, got {other:?}"),
        }
    }

    #[test]
    fn children_of_deep_surfaces_intermediate_dir() {
        let entries = [fe("a/b/c.md")];

        // Root surfaces only the top intermediate dir "a".
        assert_eq!(children_of(&entries, ""), vec![dir("a")]);
        // Under "a", the next intermediate dir "b" (rel_path "a/b").
        assert_eq!(children_of(&entries, "a"), vec![dir("a/b")]);
        // Under "a/b", the leaf file "c.md" (rel_path "a/b/c.md").
        assert_eq!(children_of(&entries, "a/b"), vec![file("a/b/c.md")]);
    }

    #[test]
    fn children_of_dedupes_dirs() {
        let entries = [fe("a/x.md"), fe("a/y.md"), fe("a/z.md")];

        // The dir "a" appears exactly once at root despite three files under it.
        assert_eq!(children_of(&entries, ""), vec![dir("a")]);

        // Under "a", all three files, sorted (already case-insensitive ascending).
        let want = vec![file("a/x.md"), file("a/y.md"), file("a/z.md")];
        assert_eq!(children_of(&entries, "a"), want);
    }

    #[test]
    fn only_dirs_with_files_appear() {
        // "a/f.md" yields dir "a"; "b.md" is a top-level FILE, not a dir. There
        // must be no dir named "b" and no phantom/empty dirs.
        let entries = [fe("a/f.md"), fe("b.md")];

        let got = children_of(&entries, "");
        // Dir "a" first, then file "b.md".
        assert_eq!(got, vec![dir("a"), file("b.md")]);

        // Defensive: there is no Dir node whose rel_path is "b".
        assert!(
            !got.iter().any(|n| matches!(n, TreeNode::Dir { rel_path, .. } if rel_path == "b")),
            "a top-level file must not surface a same-named dir"
        );
    }

    #[test]
    fn dirs_sort_before_files_case_insensitive() {
        let entries = [
            fe("Zeta.md"),
            fe("alpha/x.md"),
            fe("Beta.md"),
            fe("gamma/y.md"),
        ];

        let got = children_of(&entries, "");

        // Dirs FIRST, case-insensitive ("alpha" < "gamma"), then files
        // case-insensitive ("Beta" < "Zeta") — case must NOT reorder these.
        let want = vec![
            dir("alpha"),
            dir("gamma"),
            file("Beta.md"),
            file("Zeta.md"),
        ];
        assert_eq!(got, want);
    }

    // =====================================================================
    // ancestors_to_reveal
    // =====================================================================

    #[test]
    fn ancestors_to_reveal_cases() {
        struct Case {
            rel_path: &'static str,
            expected: Vec<String>,
        }

        let cases = [
            Case {
                rel_path: "a/b/c.md",
                expected: vec!["a".to_string(), "a/b".to_string()],
            },
            Case {
                rel_path: "c.md",
                expected: Vec::new(),
            },
            Case {
                rel_path: "a/c.md",
                expected: vec!["a".to_string()],
            },
            Case {
                rel_path: "x/y/z/f.md",
                expected: vec![
                    "x".to_string(),
                    "x/y".to_string(),
                    "x/y/z".to_string(),
                ],
            },
        ];

        for case in &cases {
            assert_eq!(
                ancestors_to_reveal(case.rel_path),
                case.expected,
                "rel_path: {}",
                case.rel_path
            );
        }
    }

    // =====================================================================
    // node_rel_path
    // =====================================================================

    #[test]
    fn node_rel_path_both_variants() {
        let d = dir("a/b");
        assert_eq!(node_rel_path(&d), "a/b", "Dir rel_path");

        let f = file("a/b/c.md");
        assert_eq!(node_rel_path(&f), "a/b/c.md", "File rel_path");
    }

    // =====================================================================
    // rel_path_for
    // =====================================================================

    #[test]
    fn rel_path_for_returns_display_of_matching_entry() {
        let entries = [fe("a/b.md"), fe("c.md")];

        // Exact abs-path match returns that entry's `display`.
        assert_eq!(
            rel_path_for(&entries, Path::new("/root/a/b.md")),
            Some("a/b.md".to_string())
        );
        assert_eq!(
            rel_path_for(&entries, Path::new("/root/c.md")),
            Some("c.md".to_string())
        );
    }

    #[test]
    fn rel_path_for_absent_path_is_none() {
        let entries = [fe("a/b.md"), fe("c.md")];
        assert_eq!(rel_path_for(&entries, Path::new("/root/nope.md")), None);
    }

    #[test]
    fn rel_path_for_empty_entries_is_none() {
        let entries: [FileEntry; 0] = [];
        assert_eq!(rel_path_for(&entries, Path::new("/root/a/b.md")), None);
    }

    #[test]
    fn rel_path_for_requires_exact_path_not_prefix() {
        // Only "/root/a/b.md" exists; the parent dir path "/root/a" is a prefix
        // but NOT an entry path, so it must not match.
        let entries = [fe("a/b.md")];
        assert_eq!(rel_path_for(&entries, Path::new("/root/a")), None);
    }

    // =====================================================================
    // edge cases
    // =====================================================================

    #[test]
    fn empty_entries_yield_empty() {
        let entries: [FileEntry; 0] = [];
        assert_eq!(children_of(&entries, ""), Vec::new());
        assert_eq!(children_of(&entries, "a"), Vec::new());
        assert_eq!(ancestors_to_reveal(""), Vec::<String>::new());
    }
}
