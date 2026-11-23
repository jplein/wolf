//! The rules that place a path opened from outside Wolf: directory versus file
//! (`specs/cli-and-file-open.md#1-the-command`), the project directory a file belongs to
//! (`specs/cli-and-file-open.md#51-choosing-the-tab`, step 2), and the pane of the chosen tab
//! it opens in (`specs/cli-and-file-open.md#52-choosing-the-pane`).
//!
//! Step 3 of choosing the tab is [`crate::app_state::project_tab`]'s rule, which a front-end
//! applies to the directory [`project_directory`] returns.

use std::fs;
use std::path::{Path, PathBuf};

use crate::app_state::{Node, Pane, PaneOrientation, PaneRef, TabContent};

/// What an open request asks for, decided from its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    /// No path: launch or activate Wolf.
    Activate,
    /// An existing directory, canonicalized, for Open Folder.
    Directory(PathBuf),
    /// Anything else, existing or not: the nearest existing ancestor of its directory
    /// canonicalized, the rest of the path kept as given, the last component never resolved.
    File(PathBuf),
}

impl OpenTarget {
    /// The target `path` names. `path` is expected to be absolute; a part that cannot be
    /// canonicalized is kept as written.
    #[must_use]
    pub fn of(path: Option<&Path>) -> OpenTarget {
        let Some(path) = path else {
            return OpenTarget::Activate;
        };
        if path.is_dir() {
            return OpenTarget::Directory(
                fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()),
            );
        }
        OpenTarget::File(resolve_file(path).1)
    }
}

/// `(D, F')` for a file path: `D` the canonical nearest existing directory at or above its
/// parent, `F'` that directory joined with the rest of the path.
fn resolve_file(file: &Path) -> (PathBuf, PathBuf) {
    let parent = file.parent().unwrap_or(file);
    let Some(existing) = parent.ancestors().find(|dir| dir.is_dir()) else {
        return (parent.to_owned(), file.to_owned());
    };
    let rest = file.strip_prefix(existing).unwrap_or(file);
    let dir = fs::canonicalize(existing).unwrap_or_else(|_| existing.to_owned());
    let resolved = dir.join(rest);
    (dir, resolved)
}

/// The first ancestor-or-self of `dir` holding a `.git`, a directory or a file.
fn repository_root(dir: &Path) -> Option<&Path> {
    dir.ancestors()
        .find(|dir| dir.join(".git").symlink_metadata().is_ok())
}

/// The project directory `P` of §5.1 step 2 for `file`, given the directories open in tabs,
/// which are canonical; an empty entry is ignored. Total, and runs no process: a linked
/// worktree is recognised by reading `.git` and `commondir`, as `git rev-parse
/// --git-common-dir` would resolve them.
#[must_use]
pub fn project_directory(file: &Path, open_dirs: &[PathBuf]) -> PathBuf {
    let is_open = |dir: &Path| {
        open_dirs
            .iter()
            .any(|open| !open.as_os_str().is_empty() && open == dir)
    };
    let (dir, file) = resolve_file(file);
    let mut above_file = file.ancestors().skip(1);
    let Some(root) = repository_root(&dir) else {
        return above_file
            .find(|a| is_open(a))
            .unwrap_or(dir.as_path())
            .to_owned();
    };
    if let Some(inside) = above_file
        .take_while(|a| a.starts_with(root))
        .find(|a| is_open(a))
    {
        return inside.to_owned();
    }
    match root.ancestors().skip(1).find(|a| is_open(a)) {
        Some(above) if !is_linked_worktree_of(root, above) => above.to_owned(),
        _ => root.to_owned(),
    }
}

/// Whether `root` is a linked worktree whose git common dir is that of `other`'s repository.
fn is_linked_worktree_of(root: &Path, other: &Path) -> bool {
    let Some(gitdir) = gitdir_of(root) else {
        return false;
    };
    let Some(common) = read_commondir(&gitdir) else {
        return false;
    };
    let Some(other_root) = repository_root(other) else {
        return false;
    };
    common_dir_of(other_root).is_some_and(|other_common| other_common == common)
}

/// The canonical git common dir of the repository rooted at `root`.
fn common_dir_of(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return fs::canonicalize(dot_git).ok();
    }
    let gitdir = gitdir_of(root)?;
    read_commondir(&gitdir).or_else(|| fs::canonicalize(gitdir).ok())
}

/// The directory a `.git` file's `gitdir:` line names, relative to `root` when relative.
fn gitdir_of(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if !dot_git.is_file() {
        return None;
    }
    let text = fs::read_to_string(dot_git).ok()?;
    let named = text
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:"))?
        .trim();
    (!named.is_empty()).then(|| root.join(named))
}

/// The canonical directory `gitdir/commondir` names, which only a linked worktree's git dir
/// holds.
fn read_commondir(gitdir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(gitdir.join("commondir")).ok()?;
    fs::canonicalize(gitdir.join(text.trim())).ok()
}

/// Where §5.2 puts a file in its chosen tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneChoice {
    /// Whether the tab's zoom, a pane's or the drawer's, is undone first.
    pub unzoom: bool,
    /// The pane the file goes to.
    pub placement: PanePlacement,
}

/// The pane of a [`PaneChoice`]. Indices count the root's panes depth-first, as
/// [`PaneRef::Pane`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanePlacement {
    /// An editor pane already shows the file: focus it.
    Focus(usize),
    /// Open the file in this editor pane.
    Open(usize),
    /// Make this pane, a preview, an editor pane showing the file.
    Convert(usize),
    /// Split the whole tab vertically, the file in a new editor pane on the left and
    /// everything the tab held on the right.
    SplitLeft,
}

/// Below this, a coordinate or an area difference counts as zero.
const EPSILON: f64 = 1e-9;

/// A pane's place in the unit square of the pane area.
#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// The pane `file` opens in, by §5.2, in `tab`. Paths compare lexically, so `file` is given
/// in the form tab panes hold, [`OpenTarget::File`]'s. Terminal panes and the drawer are never
/// candidates.
#[must_use]
pub fn choose_pane(tab: &TabContent, file: &Path) -> PaneChoice {
    let mut panes = Vec::new();
    if let Some(root) = &tab.root {
        place(
            root,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            &mut panes,
        );
    }
    PaneChoice {
        unzoom: tab.zoomed.is_some(),
        placement: placement(&panes, tab.focused, file),
    }
}

fn placement(panes: &[(&Pane, Rect)], focused: Option<PaneRef>, file: &Path) -> PanePlacement {
    let focused = match focused {
        Some(PaneRef::Pane(i)) if i < panes.len() => Some(i),
        _ => None,
    };
    let is_edit = |pane: &Pane| matches!(pane, Pane::Edit { .. });
    let showing: Vec<usize> = panes
        .iter()
        .enumerate()
        .filter(|(_, (pane, _))| is_edit(pane) && Path::new(pane.path()) == file)
        .map(|(i, _)| i)
        .collect();
    if let Some(&first) = showing.first() {
        return PanePlacement::Focus(focused.filter(|i| showing.contains(i)).unwrap_or(first));
    }
    if let Some(i) = focused.filter(|&i| is_edit(panes[i].0)) {
        return PanePlacement::Open(i);
    }
    if let Some(i) = leftmost(panes, is_edit) {
        return PanePlacement::Open(i);
    }
    if let Some(i) = leftmost(panes, |pane| {
        matches!(pane, Pane::Edit { .. } | Pane::Preview { .. })
    }) {
        return PanePlacement::Convert(i);
    }
    PanePlacement::SplitLeft
}

/// The largest pane `accept` takes whose left edge is the area's, the topmost among equals,
/// then the first depth-first.
fn leftmost(panes: &[(&Pane, Rect)], accept: impl Fn(&Pane) -> bool) -> Option<usize> {
    let mut best: Option<(usize, Rect)> = None;
    for (i, (pane, rect)) in panes.iter().enumerate() {
        if rect.x > EPSILON || !accept(pane) {
            continue;
        }
        let better = best.is_none_or(|(_, b)| {
            let (area, best_area) = (rect.w * rect.h, b.w * b.h);
            area > best_area + EPSILON || (area >= best_area - EPSILON && rect.y < b.y - EPSILON)
        });
        if better {
            best = Some((i, *rect));
        }
    }
    best.map(|(i, _)| i)
}

/// Appends `node`'s panes depth-first with their rects inside `rect`.
fn place<'a>(node: &'a Node, rect: Rect, out: &mut Vec<(&'a Pane, Rect)>) {
    let layout = match node {
        Node::Pane(pane) => {
            out.push((pane, rect));
            return;
        }
        Node::Layout(layout) => layout,
    };
    let n = layout.children.len();
    let sum: f64 = layout.weights.iter().sum();
    let usable = layout.weights.len() == n
        && layout.weights.iter().all(|w| w.is_finite() && *w >= 0.0)
        && sum > 0.0;
    let mut offset = 0.0;
    for (i, child) in layout.children.iter().enumerate() {
        let share = if usable {
            layout.weights[i] / sum
        } else {
            1.0 / n as f64
        };
        let child_rect = match layout.orientation {
            PaneOrientation::Vertical => Rect {
                x: rect.x + rect.w * offset,
                w: rect.w * share,
                ..rect
            },
            PaneOrientation::Horizontal => Rect {
                y: rect.y + rect.h * offset,
                h: rect.h * share,
                ..rect
            },
        };
        place(child, child_rect, out);
        offset += share;
    }
}
