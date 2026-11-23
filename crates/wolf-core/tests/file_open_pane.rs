//! Headless table tests of `wolf_core::choose_pane`, the pane rule of
//! `specs/cli-and-file-open.md` §5.2, over hand-built `app_state::TabContent`. Every expected
//! `PaneChoice` is written out by hand.
//!
//! Layouts are written in the rows' comments as `[a | b]`, a `Vertical` split (side by side)
//! with equal weights unless given, and `H[a .3, b .7]`, a `Horizontal` one (stacked). `E(p)` is
//! an edit pane on `p`, `V(p)` a preview and `T` a terminal; `f` is the focused pane. Indices
//! count panes depth-first, as `PaneRef` does.
//!
//! The file asked about is [`F`]. No other pane is named after it, so a pane counts as showing it
//! only by an exact path match.

use std::path::Path;

use wolf_core::app_state::{Layout, Node, Pane, PaneOrientation, PaneRef, TabContent};
use wolf_core::{choose_pane, PaneChoice, PanePlacement};

/// The file every case opens.
const F: &str = "/work/proj/opened.md";
/// Other files, which a pane may show instead.
const A: &str = "/work/proj/alpha.md";
const B: &str = "/work/proj/bravo.md";
const X: &str = "/work/proj/xray.md";
/// A Markdown source a preview pane shows.
const P: &str = "/work/proj/preview-source.md";
/// A terminal pane's directory.
const TERM_DIR: &str = "/work/proj";

fn e(path: &str) -> Node {
    Node::Pane(Pane::Edit {
        path: path.to_owned(),
        cursor: None,
        top_line: None,
    })
}

fn v(path: &str) -> Node {
    Node::Pane(Pane::Preview {
        path: path.to_owned(),
        scroll: None,
    })
}

fn t() -> Node {
    Node::Pane(Pane::Terminal {
        path: TERM_DIR.to_owned(),
    })
}

fn split(orientation: PaneOrientation, weights: Vec<f64>, children: Vec<Node>) -> Node {
    Node::Layout(Layout {
        orientation,
        weights,
        children,
    })
}

/// `[a | b | …]` with equal weights.
fn vs(children: Vec<Node>) -> Node {
    let n = children.len();
    split(PaneOrientation::Vertical, vec![1.0 / n as f64; n], children)
}

/// `H[…]` with the given weights.
fn hs(weights: Vec<f64>, children: Vec<Node>) -> Node {
    split(PaneOrientation::Horizontal, weights, children)
}

fn tab(root: Option<Node>, focused: Option<PaneRef>, zoomed: Option<PaneRef>) -> TabContent {
    TabContent {
        root,
        focused,
        zoomed,
        ..TabContent::default()
    }
}

fn pane(i: usize) -> Option<PaneRef> {
    Some(PaneRef::Pane(i))
}

fn keep(placement: PanePlacement) -> PaneChoice {
    PaneChoice {
        unzoom: false,
        placement,
    }
}

/// Runs each `(row, tab, expected)` and fails on the first mismatch, naming the row and what the
/// rule it exercises means.
fn check(rows: Vec<(&str, TabContent, PaneChoice)>, meaning: &str) {
    for (row, content, expected) in rows {
        let got = choose_pane(&content, Path::new(F));
        assert_eq!(
            got, expected,
            "row {row}: choose_pane gave {got:?} where {expected:?} is right. {meaning}\nThe tab \
             was {content:?}"
        );
    }
}

#[test]
fn a_pane_already_showing_the_file_is_focused() {
    check(
        vec![
            (
                "lone E(F)",
                tab(Some(e(F)), None, None),
                keep(PanePlacement::Focus(0)),
            ),
            (
                "[E(A) | E(F)] f0",
                tab(Some(vs(vec![e(A), e(F)])), pane(0), None),
                keep(PanePlacement::Focus(1)),
            ),
            (
                "[E(F) | E(F)] f1",
                tab(Some(vs(vec![e(F), e(F)])), pane(1), None),
                keep(PanePlacement::Focus(1)),
            ),
            (
                "[E(F) | E(X) | E(F)] f1",
                tab(Some(vs(vec![e(F), e(X), e(F)])), pane(1), None),
                keep(PanePlacement::Focus(0)),
            ),
            (
                "[V(F) | E(F)] f0",
                tab(Some(vs(vec![v(F), e(F)])), pane(0), None),
                keep(PanePlacement::Focus(1)),
            ),
            (
                "[V(F) | E(A)] f1",
                tab(Some(vs(vec![v(F), e(A)])), pane(1), None),
                keep(PanePlacement::Open(1)),
            ),
        ],
        "An editor pane already on F is focused: the active one when it is among them, else the \
         first depth-first. Focus(0) where Focus(1) is right in [E(F) | E(F)] f1 means the \
         active pane is ignored; Focus(0) on a V(F) row means a preview counts as showing F.",
    );
}

#[test]
fn the_active_editor_pane_takes_the_file() {
    check(
        vec![
            (
                "[E(A) | E(B)] f1",
                tab(Some(vs(vec![e(A), e(B)])), pane(1), None),
                keep(PanePlacement::Open(1)),
            ),
            (
                "[E(A) | E(B)] f out of range",
                tab(Some(vs(vec![e(A), e(B)])), pane(5), None),
                keep(PanePlacement::Open(0)),
            ),
            (
                "[E(A) | E(B)] f drawer",
                tab(Some(vs(vec![e(A), e(B)])), Some(PaneRef::Drawer), None),
                keep(PanePlacement::Open(0)),
            ),
        ],
        "Rule 1 opens F in the active pane when it is an editor; Open(0) for f1 means rule 1 is \
         skipped. An out-of-range or drawer focus skips rule 1, so the leftmost editor, pane 0, \
         takes F.",
    );
}

#[test]
fn the_leftmost_editor_pane_takes_the_file() {
    check(
        vec![
            (
                "[E(A) | T] f1",
                tab(Some(vs(vec![e(A), t()])), pane(1), None),
                keep(PanePlacement::Open(0)),
            ),
            (
                "[H[E(A) .3, E(B) .7] | T] f2",
                tab(
                    Some(vs(vec![hs(vec![0.3, 0.7], vec![e(A), e(B)]), t()])),
                    pane(2),
                    None,
                ),
                keep(PanePlacement::Open(1)),
            ),
            (
                "[H[E(A) .5, E(B) .5] | T] f2",
                tab(
                    Some(vs(vec![hs(vec![0.5, 0.5], vec![e(A), e(B)]), t()])),
                    pane(2),
                    None,
                ),
                keep(PanePlacement::Open(0)),
            ),
            (
                "H[[T | E(A)] .5, E(B) .5] f0",
                tab(
                    Some(hs(vec![0.5, 0.5], vec![vs(vec![t(), e(A)]), e(B)])),
                    pane(0),
                    None,
                ),
                keep(PanePlacement::Open(2)),
            ),
            (
                "[T | E(A)] f0",
                tab(Some(vs(vec![t(), e(A)])), pane(0), None),
                keep(PanePlacement::SplitLeft),
            ),
        ],
        "Rule 2 takes the editor touching the area's left edge, the largest, then the topmost. \
         Open(0) where Open(1) is right in the .3/.7 row means the smallest wins; Open(1) in the \
         .5/.5 row means a tie goes to the lower pane; Open(1) in [T | E(A)] means leftmost is \
         read as the smallest x rather than the left edge; a wrong pane in the H[[T | E(A)], \
         E(B)] row means a Horizontal split is read as dividing width.",
    );
}

#[test]
fn a_leftmost_preview_becomes_the_editor() {
    check(
        vec![
            (
                "[V(P) | E(A)] f0",
                tab(Some(vs(vec![v(P), e(A)])), pane(0), None),
                keep(PanePlacement::Convert(0)),
            ),
            (
                "[V(P) | T] f1",
                tab(Some(vs(vec![v(P), t()])), pane(1), None),
                keep(PanePlacement::Convert(0)),
            ),
        ],
        "Rule 3 converts the leftmost editor or preview pane when no editor touches the left \
         edge; SplitLeft means rule 3 is skipped, Open(1) in [V(P) | E(A)] means an editor off \
         the left edge counts as leftmost.",
    );
}

#[test]
fn with_no_candidate_the_tab_splits_left() {
    check(
        vec![
            (
                "[T | T] f0",
                tab(Some(vs(vec![t(), t()])), pane(0), None),
                keep(PanePlacement::SplitLeft),
            ),
            (
                "lone T f0",
                tab(Some(t()), pane(0), None),
                keep(PanePlacement::SplitLeft),
            ),
            (
                "root None",
                tab(None, None, None),
                keep(PanePlacement::SplitLeft),
            ),
        ],
        "With no editor or preview pane, the whole tab splits with F on the left. Open or Convert \
         of a terminal pane means terminals count as candidates, which would replace the \
         terminal that ran `wolf`.",
    );
}

#[test]
fn a_zoomed_tab_is_unzoomed_first() {
    let unzoom = |placement| PaneChoice {
        unzoom: true,
        placement,
    };
    check(
        vec![
            (
                "[E(A) | E(B)] zoomed 1 f1",
                tab(Some(vs(vec![e(A), e(B)])), pane(1), pane(1)),
                unzoom(PanePlacement::Open(1)),
            ),
            (
                "[E(A) | E(B)] zoomed drawer f1",
                tab(Some(vs(vec![e(A), e(B)])), pane(1), Some(PaneRef::Drawer)),
                unzoom(PanePlacement::Open(1)),
            ),
            (
                "[E(A) | E(B)] zoomed None f1",
                tab(Some(vs(vec![e(A), e(B)])), pane(1), None),
                keep(PanePlacement::Open(1)),
            ),
            (
                "[E(A) | E(F)] zoomed 0 f0",
                tab(Some(vs(vec![e(A), e(F)])), pane(0), pane(0)),
                unzoom(PanePlacement::Focus(1)),
            ),
        ],
        "A zoomed tab, a pane or the drawer, is unzoomed whatever the placement; unzoom false on \
         a zoomed row means the zoom is left on, true on the unzoomed row means it is reported \
         when there is none.",
    );
}

#[test]
fn bad_weights_count_as_equal_shares() {
    let row = |weights: Vec<f64>| {
        tab(
            Some(vs(vec![hs(weights, vec![e(A), e(B)]), t()])),
            pane(2),
            None,
        )
    };
    check(
        vec![
            ("H weights []", row(vec![]), keep(PanePlacement::Open(0))),
            (
                "H weights [NaN, 1]",
                row(vec![f64::NAN, 1.0]),
                keep(PanePlacement::Open(0)),
            ),
            (
                "H weights [inf, 1]",
                row(vec![f64::INFINITY, 1.0]),
                keep(PanePlacement::Open(0)),
            ),
            (
                "H weights [-1, 2]",
                row(vec![-1.0, 2.0]),
                keep(PanePlacement::Open(0)),
            ),
            (
                "H weights [.2, .8, 5]",
                row(vec![0.2, 0.8, 5.0]),
                keep(PanePlacement::Open(0)),
            ),
            (
                "H weights [0, 0]",
                row(vec![0.0, 0.0]),
                keep(PanePlacement::Open(0)),
            ),
        ],
        "Weights of the wrong length, with a non-finite or negative value, or summing to 0 give \
         equal shares, so E(A) and E(B) tie and the topmost, pane 0, wins. Open(1) on the \
         [-1, 2] or [.2, .8, 5] row means the weights were used as given, which makes E(B) the \
         larger; a panic means an index past the weights.",
    );
}
