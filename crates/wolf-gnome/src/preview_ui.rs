//! The pure, GTK-free rulebook for the MARKDOWN PREVIEW pane: where a preview
//! opens, where its inverse lands the source editor, whether either press may be
//! issued at all, whether a frame may be rendered, when a bound web view must be
//! reloaded, and what a preview pane's statusbar reads.
//!
//! A port of `WolfKeyMap`'s `PreviewSeam` and `EditSeam` (see
//! `crates/wolf-macos/Sources/WolfKeyMap/README.md`), joined into one module
//! because the two placement
//! rules are inverses of each other and a reader who has only one of them in front
//! of them is exactly the reader who inverts the polarity. Data in, data out — no
//! widget type, no `webkit6::WebView`, no `WolfCore` — which is what lets the whole
//! rulebook be unit-tested headlessly, the way [`crate::nav_ui`] and
//! [`crate::clipboard_ui`] are. The widget work of carrying the answers out lives
//! in [`crate::pane_view`].
//!
//! ## The mode strings are a cross-language contract
//!
//! [`MODE_SPLIT_RIGHT`] and its four siblings are the exact `mode` argument
//! `WolfCore::open_preview` / `WolfCore::edit_source` thread into
//! `runtime/nvim/init.lua`'s `wolf_open_preview(mode)` and `wolf_edit_source(mode)`,
//! which dispatch on the literal string. A rename here is silently a no-op there:
//! the Lua falls through to its own `else` branch and binds into the current
//! window, so the preview replaces the pane the user was reading.
//!
//! ## The two placements are inverses, and the polarity is the whole difference
//!
//! [`preview_placement`] reuses a counterpart pane when the active pane is **not**
//! a preview; [`edit_placement`] reuses one when the active pane **is**. Both
//! conditions are defensive: a preview opened from inside a preview pane, or a
//! source edited from inside an editor pane, would clobber the one pane holding
//! what the user asked to keep. Copying either condition to the other side is the
//! single most likely edit here, which is why `the_two_placements_are_inverses`
//! states the separation over the whole input domain rather than row by row.
//!
//! ## The blocking calls
//!
//! `open_preview`, `edit_source`, `preview_file`, `open_in_browser` and the
//! coalesced `render_preview` are all synchronous RPCs on the GTK main thread, so
//! [`preview_press`], [`edit_press`], [`preview_file_press`], [`browser_press`] and
//! [`may_render`] are liveness-gated exactly the way
//! [`crate::nav_ui::nav_press`] and [`crate::clipboard_ui::copy_decision`] are. The
//! inventory of this crate's blocking calls and the reasoning behind the gate live
//! in `crates/wolf-gnome/DECISIONS.md`, "Crash and stall recovery", and are
//! deliberately not restated here.
//!
//! Beyond liveness, three of the five carry a second gate and two deliberately do
//! not. [`preview_file_press`] has NONE, and that is a decision rather than an
//! omission: whether the file a clicked link names is Markdown is core's own
//! `preview_file` gate, and this front end cannot answer it without touching the
//! filesystem — which is the one thing a seam whose whole value is purity may not
//! do. A `0` from core IS the refusal, and [`crate::pane_view`] prints it.
//!
//! **Liveness is read BEFORE the buffer-kind gate**, and the order is the
//! [`crate::search_overlay::open_decision`] precedence rather than an accident: a
//! user whose Neovim has wedged is told their Neovim has wedged, not that the
//! buffer they are looking at is not Markdown — a claim that is unverifiable
//! anyway, since the snapshot behind it stopped advancing when the editor did.
//!
//! ## Totality is the contract
//!
//! [`preview_press`], [`edit_press`], [`preview_file_press`], [`browser_press`] and
//! [`may_render`] each match `Option<Liveness>` with **no `_` arm**, so appending a
//! variant to [`wolf_core::Liveness`] is an `E0004` compile error here until it is
//! classified. `Option<Liveness>` (not `Liveness`) is the input type precisely so
//! the ABSENT core, which the window's close handler leaves behind, is a classified
//! arm rather than an early return no headless test could see.
//!
//! ## The reload key is a PAIR, and a tick alone is not enough
//!
//! [`PreviewLoad`] carries the source buffer's number as well as its `changedtick`
//! because **two freshly opened files share a `changedtick`**. A gate keyed on the
//! tick alone therefore suppresses the reload exactly when a reused preview pane
//! switches which source it shows, leaving the previous document on screen under
//! the new file's statusbar. That is the defect `PreviewSeam.PreviewLoad` records
//! on macOS, and `needs_reload_table`'s fourth row is the only place in
//! this crate that can see it.
//!
//! Nothing on this path may be gated on `Snapshot::seq`: the `wolf_*` notifications
//! that carry a preview's source and tick fire the wake without bumping it.
//!
//! ## `document_update` is coupled to two things its signature cannot show
//!
//! [`document_update`] decides whether a freshly rendered document may be swapped
//! into the live view IN PLACE — which is what preserves the reader's scroll offset
//! across an edit — or has to go through a full `load_html`. Its first rule (nothing
//! has ever been loaded) is obvious from the code; the other two are not.
//!
//! A PENDING FRAGMENT forces a full load because the cross-file scroll is applied on
//! a `LoadEvent::Finished` edge, and only a real load produces one. An in-place body
//! swap fires no load event at all, so the scroll would simply never run and the
//! reader would land at the top of the linked file instead of at the anchor.
//!
//! A CHANGED SOURCE BUFFER forces a full load because the view's base URI belongs to
//! the PREVIOUS file ([`crate::preview_link::file_uri_for`]) and a body swap does not
//! replace it. Every relative link in the new document would resolve against the old
//! document's directory — and, because the rendered `<head>` is byte-identical across
//! files by design, [`crate::pane_view`]'s head-offset guard cannot see the switch on
//! its own. The key is the source BUFNR and never the tick, for the reason stated
//! just above.
//!
//! ## A preview pane's statusbar does not come from `WolfCore::pane_status`
//!
//! Preview windows are omitted from `win_buffers` entirely, so core answers
//! `[No Name]` for one. [`preview_status`] builds the two lines from the preview's
//! own source path instead, and its `abs_path` is that path VERBATIM — never the
//! display path — because [`crate::pane_view`] persists exactly that field as a
//! pane-layout leaf, and a cwd-relative string there is a path relative to a
//! working directory `state.json` has no record of.

use std::path::Path;

use wolf_core::{Liveness, PaneStatus};

/// The prefix a preview pane's statusbar filename carries, matching the name
/// `runtime/nvim/init.lua` gives the preview scratch buffer so the buffer picker
/// and the statusbar read the same way.
pub const PREVIEW_STATUS_PREFIX: &str = "Preview: ";

/// Open the preview in a NEW pane to the right of the only pane there is.
pub const MODE_SPLIT_RIGHT: &str = "split_right";

/// Bind the preview (or the source) into the ACTIVE pane, replacing its buffer.
///
/// Shared by both placements: it is the same instruction to Neovim either way.
pub const MODE_ACTIVE: &str = "active";

/// Bind the preview into the OTHER pane, which is already a preview.
pub const MODE_REUSE_PREVIEW_PANE: &str = "reuse_preview_pane";

/// Open the source editor in a NEW pane to the left of the lone preview, so the
/// editor lands left and the preview stays right.
pub const MODE_SPLIT_LEFT: &str = "split_left";

/// Bind the source into the OTHER pane, which is already an editor.
pub const MODE_REUSE_EDITOR_PANE: &str = "reuse_editor_pane";

/// What a preview pane's web view was last loaded from: the source buffer's
/// number AND its `changedtick`.
///
/// Both halves are load-bearing; see the module docs for why the tick alone is not
/// a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewLoad {
    /// The bound source buffer's number.
    pub source_bufnr: i64,
    /// That buffer's `changedtick` at the moment the view was loaded.
    pub source_tick: u64,
}

/// The window shape a Preview press is decided against.
#[derive(Clone, Copy, Debug)]
pub struct PreviewPlacement {
    /// The window's live pane count.
    pub pane_count: usize,
    /// Whether the ACTIVE pane is itself a preview.
    pub active_is_preview: bool,
    /// How many panes OTHER than the active one are previews.
    pub other_preview_panes: usize,
}

/// The window shape an Edit press is decided against.
#[derive(Clone, Copy, Debug)]
pub struct EditPlacement {
    /// The window's live pane count.
    pub pane_count: usize,
    /// Whether the ACTIVE pane is itself a preview.
    pub active_is_preview: bool,
    /// How many panes OTHER than the active one are editors (non-preview).
    pub other_editor_panes: usize,
}

/// What pressing Preview does.
#[derive(Debug, PartialEq, Eq)]
pub enum PreviewPress {
    /// Issue the BLOCKING `open_preview` at this placement mode.
    Open(&'static str),
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause
    /// (the [`crate::command_action`] / [`crate::nav_ui`] convention).
    Refuse(&'static str),
}

/// What pressing Edit Source does.
#[derive(Debug, PartialEq, Eq)]
pub enum EditPress {
    /// Issue the BLOCKING `edit_source` at this placement mode.
    Edit(&'static str),
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause.
    Refuse(&'static str),
}

/// What following a link that names a FILE does.
#[derive(Debug, PartialEq, Eq)]
pub enum PreviewFilePress {
    /// Issue the BLOCKING `preview_file`, rebinding the pane's preview window at the
    /// linked path.
    Follow,
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause.
    Refuse(&'static str),
}

/// What choosing Preview in Browser does.
#[derive(Debug, PartialEq, Eq)]
pub enum BrowserPress {
    /// Issue the BLOCKING `open_in_browser` and hand its URL to the platform's URI
    /// launcher.
    Open,
    /// Do nothing and issue no RPC; the string names why, as a mid-sentence clause.
    Refuse(&'static str),
}

/// Where a Preview press opens its pane.
///
/// `pane_count <= 1` splits right unconditionally — a degenerate count of `0` falls
/// here too. Exactly two panes with an EDITOR active and exactly one other preview
/// reuses that preview. Everything else replaces the active pane's buffer.
#[must_use]
pub fn preview_placement(p: PreviewPlacement) -> &'static str {
    // The pane count is read FIRST and dominates both preview inputs: they describe
    // a two-pane window, so consulting them for a lone pane would be reading a shape
    // that does not exist and would replace the only pane the user has.
    if p.pane_count <= 1 {
        return MODE_SPLIT_RIGHT;
    }
    if p.pane_count == 2 && !p.active_is_preview && p.other_preview_panes == 1 {
        return MODE_REUSE_PREVIEW_PANE;
    }
    MODE_ACTIVE
}

/// Where an Edit press lands the source editor — [`preview_placement`]'s inverse.
///
/// `pane_count <= 1` splits left unconditionally. Exactly two panes with a PREVIEW
/// active and exactly one other editor reuses that editor. Everything else replaces
/// the active pane's buffer.
#[must_use]
pub fn edit_placement(p: EditPlacement) -> &'static str {
    // The pane count dominates here too, for the same reason it does on the preview
    // side.
    if p.pane_count <= 1 {
        return MODE_SPLIT_LEFT;
    }
    // The polarity is INVERTED relative to `preview_placement`: reuse needs the
    // ACTIVE pane to BE a preview. Copying that seam's `!p.active_is_preview` across
    // is the single most likely edit in this file, and it silently swaps rows 3 and 4
    // of `edit_placement_table` — an Edit fired from the preview pane would clobber
    // the preview it was fired from.
    if p.pane_count == 2 && p.active_is_preview && p.other_editor_panes == 1 {
        return MODE_REUSE_EDITOR_PANE;
    }
    MODE_ACTIVE
}

/// Whether a Preview press may issue its BLOCKING `open_preview` at all.
///
/// TOTAL by construction: a `match` over `Option<Liveness>` with **no `_` arm**,
/// read BEFORE the `markdown_active` gate (see the module docs). Only
/// [`Liveness::Alive`] with a Markdown buffer active opens, at the mode
/// [`preview_placement`] gives for `p`.
#[must_use]
pub fn preview_press(
    liveness: Option<Liveness>,
    markdown_active: bool,
    p: PreviewPlacement,
) -> PreviewPress {
    match liveness {
        Some(Liveness::Alive) => {
            if markdown_active {
                PreviewPress::Open(preview_placement(p))
            } else {
                PreviewPress::Refuse(
                    "the buffer being edited is not Markdown, and wolf_open_preview no-ops on \
                     anything else, so the blocking RPC would be spent to be told nothing happened",
                )
            }
        }
        Some(Liveness::NotResponding) => PreviewPress::Refuse(
            "the captive Neovim is not responding, and opening a preview is a blocking RPC that \
             would freeze the window for another whole timeout budget",
        ),
        Some(Liveness::Dead) => PreviewPress::Refuse(
            "the captive Neovim has crashed, so there is no buffer left to render a preview of",
        ),
        None => PreviewPress::Refuse(
            "the window is shutting down and the core has already been taken out, so there is \
             nothing left to preview",
        ),
    }
}

/// Whether an Edit press may issue its BLOCKING `edit_source` at all.
///
/// TOTAL by construction, the same shape as [`preview_press`]: liveness first, then
/// the `preview_active` gate. Only [`Liveness::Alive`] with a preview pane active
/// edits, at the mode [`edit_placement`] gives for `p`.
#[must_use]
pub fn edit_press(liveness: Option<Liveness>, preview_active: bool, p: EditPlacement) -> EditPress {
    match liveness {
        Some(Liveness::Alive) => {
            if preview_active {
                EditPress::Edit(edit_placement(p))
            } else {
                EditPress::Refuse(
                    "the active pane is not a preview, so there is no b:wolf_preview_source to \
                     return to and wolf_edit_source would find nothing to do",
                )
            }
        }
        Some(Liveness::NotResponding) => EditPress::Refuse(
            "the captive Neovim is not responding, and leaving a preview is a blocking RPC that \
             would freeze the window for another whole timeout budget",
        ),
        Some(Liveness::Dead) => EditPress::Refuse(
            "the captive Neovim has crashed, so there is no preview pane left to leave",
        ),
        None => EditPress::Refuse(
            "the window is shutting down and the core has already been taken out, so there is \
             nowhere to open the source",
        ),
    }
}

/// Whether a followed preview link may issue its BLOCKING `preview_file` at all.
///
/// TOTAL by construction, [`crate::nav_ui::nav_press`]'s shape: a `match` over
/// `Option<Liveness>` with **no `_` arm**. Only [`Liveness::Alive`] follows.
///
/// There is deliberately NO second gate, and the module docs say why: the linked
/// file's Markdown-ness belongs to core's `preview_file`, and answering it here
/// would mean a filesystem touch in a pure seam. What this function decides is only
/// whether the RPC may be spent at all.
#[must_use]
pub fn preview_file_press(liveness: Option<Liveness>) -> PreviewFilePress {
    match liveness {
        Some(Liveness::Alive) => PreviewFilePress::Follow,
        Some(Liveness::NotResponding) => PreviewFilePress::Refuse(
            "the captive Neovim is not responding, and repointing the preview is a blocking RPC \
             that would freeze the window for another whole timeout budget",
        ),
        Some(Liveness::Dead) => PreviewFilePress::Refuse(
            "the captive Neovim has crashed, so there is no preview window left to repoint at the \
             linked file",
        ),
        None => PreviewFilePress::Refuse(
            "the window is shutting down and the core has already been taken out, so there is no \
             preview window left to follow the link into",
        ),
    }
}

/// Whether Preview in Browser may issue its BLOCKING `open_in_browser` at all.
///
/// TOTAL by construction, [`preview_press`]'s shape and its reason: liveness is read
/// BEFORE the `markdown_active` gate, so a user whose Neovim has wedged is told
/// their Neovim has wedged rather than being told about a buffer kind read out of a
/// snapshot that stopped advancing when the editor did.
#[must_use]
pub fn browser_press(liveness: Option<Liveness>, markdown_active: bool) -> BrowserPress {
    match liveness {
        Some(Liveness::Alive) => {
            if markdown_active {
                BrowserPress::Open
            } else {
                BrowserPress::Refuse(
                    "the buffer being edited is not Markdown, and open_in_browser renders nothing \
                     for anything else, so the blocking RPC would be spent to be handed an empty \
                     URL",
                )
            }
        }
        Some(Liveness::NotResponding) => BrowserPress::Refuse(
            "the captive Neovim is not responding, and rendering the document for the loopback \
             server is a blocking RPC that would freeze the window for another whole timeout \
             budget",
        ),
        Some(Liveness::Dead) => BrowserPress::Refuse(
            "the captive Neovim has crashed, so there is no buffer left for the loopback server \
             to serve",
        ),
        None => BrowserPress::Refuse(
            "the window is shutting down and the core has already been taken out, so there is \
             nothing left to serve over loopback",
        ),
    }
}

/// The URL to hand the platform's URI launcher, or `None` to launch NOTHING.
///
/// [`crate::clipboard_ui::clipboard_write`]'s shape: a filter, and `None` means do
/// not launch rather than launch something else. `WolfCore::open_in_browser` answers
/// `""` on each of its three error paths — an RPC failure, a non-Markdown buffer,
/// and a failure to bind the loopback socket — having already logged which; handing
/// that empty string to a launcher asks the desktop to open the empty URI.
///
/// There is deliberately NO `http://127.0.0.1:` prefix check here. The loopback-only
/// bind is core's own hard security requirement, stated in
/// `crates/wolf-core/DECISIONS.md`, and restating it as a second gate would give one
/// rule two owners that could drift apart — the more dangerous direction being the
/// one where this copy is the laxer.
#[must_use]
pub fn browser_url(raw: &str) -> Option<&str> {
    // Trimmed for the TEST, but the untrimmed string is what is returned: core
    // produces the URL, and a launcher should be handed exactly what core built.
    if raw.trim().is_empty() {
        None
    } else {
        Some(raw)
    }
}

/// Whether the coalesced `render_preview` may be issued at all.
///
/// TOTAL by construction, and `true` for [`Liveness::Alive`] alone. A render is a
/// blocking RPC like the two presses, so it carries the same gate rather than a
/// weaker one.
#[must_use]
pub fn may_render(liveness: Option<Liveness>) -> bool {
    // The three refusing arms are written SEPARATELY rather than as an or-pattern or
    // a wildcard, so a new `Liveness` variant is an E0004 here — which is the promise
    // the module docs make and the only thing that keeps a blocking render from
    // defaulting to "issue it" on a reading nobody has classified.
    match liveness {
        Some(Liveness::Alive) => true,
        Some(Liveness::NotResponding) => false,
        Some(Liveness::Dead) => false,
        None => false,
    }
}

/// Whether a preview pane's web view has to be reloaded.
///
/// `current` is what the pane is bound to now, `loaded` what its view was last
/// loaded from; `None` on either side means "this pane is not a preview" and "this
/// pane has never been loaded" respectively. A pane that is not a preview never
/// reloads, whatever was loaded into it before.
#[must_use]
pub fn needs_reload(current: Option<PreviewLoad>, loaded: Option<PreviewLoad>) -> bool {
    match (current, loaded) {
        (None, _) => false,
        (Some(_), None) => true,
        // The WHOLE key, both halves: the tick alone misses a reused preview pane
        // switching sources (two freshly opened files share a `changedtick`), and the
        // buffer number alone misses every edit of the file being previewed.
        (Some(current), Some(loaded)) => current != loaded,
    }
}

/// How a rendered preview document must reach the live web view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentUpdate {
    /// Replace the whole document through `load_html`, re-establishing the base URI
    /// and producing the `LoadEvent::Finished` edge a pending fragment scroll needs.
    FullLoad,
    /// The document MAY be swapped in place, preserving the reader's scroll offset —
    /// subject to [`crate::pane_view`]'s own head-offset guard, which is a separate
    /// condition and not implied by this one.
    InPlaceEligible,
}

/// Whether a rendered document may be swapped into the view in place.
///
/// Total, and the four rules are ORDERED. `loaded` of `None` is a first load and has
/// no committed `<head>` to swap under; a pending fragment needs a real load edge; a
/// changed source buffer needs a new base URI; everything else is eligible.
///
/// What the final arm preserves, and what changing it to [`DocumentUpdate::FullLoad`]
/// would cost, was MEASURED: with the in-place swap, `window.scrollY` 2000 was
/// preserved in 24 of 24 samples across a live edit, against 0 of 24 for a full
/// `load_html`. A reader thrown back to the top of the document on every keystroke
/// is exactly what a live preview is not.
///
/// The source-switch rule keys on `source_bufnr` and never on `source_tick`: two
/// freshly opened files share a `changedtick`, the same defect [`needs_reload`]'s
/// pairing records. See the module docs for the base-URI and load-edge couplings
/// this signature cannot show.
#[must_use]
pub fn document_update(
    current: PreviewLoad,
    loaded: Option<PreviewLoad>,
    fragment_pending: bool,
) -> DocumentUpdate {
    let Some(loaded) = loaded else {
        return DocumentUpdate::FullLoad;
    };
    if fragment_pending {
        return DocumentUpdate::FullLoad;
    }
    if loaded.source_bufnr != current.source_bufnr {
        return DocumentUpdate::FullLoad;
    }
    DocumentUpdate::InPlaceEligible
}

/// A preview pane's two statusbar lines, built from its source path rather than
/// from `WolfCore::pane_status` (see the module docs).
///
/// `display_path` is `core.display_path(source_path)`, passed in verbatim so this
/// stays free of the filesystem. An empty `source_path` yields a bare `"Preview"`
/// with both paths empty, since there is nothing to name and nothing to persist.
#[must_use]
pub fn preview_status(source_path: &str, display_path: &str) -> PaneStatus {
    // An empty source forces BOTH paths empty, whatever `display_path` was handed in:
    // there is no source to name and nothing a later launch could reopen, and a
    // `display_path` beneath a nameless pane would be a second line describing a file
    // the first line cannot identify.
    if source_path.is_empty() {
        return PaneStatus {
            filename: "Preview".to_string(),
            display_path: String::new(),
            abs_path: String::new(),
        };
    }

    let basename = Path::new(source_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source_path);

    PaneStatus {
        filename: format!("{PREVIEW_STATUS_PREFIX}{basename}"),
        display_path: display_path.to_string(),
        // VERBATIM, never the display path: `pane_view` persists this field as a
        // pane-layout leaf, and `state.json` records no working directory to resolve
        // a relativized one against.
        abs_path: source_path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::path::Path;

    // =========================================================================
    // The whole input domain
    // =========================================================================

    /// A one-clause name for a liveness reading, used in every failure message
    /// below.
    ///
    /// The `match` here is deliberately `_`-FREE, and that is its whole purpose
    /// beyond the prose: [`preview_press_table`], [`edit_press_table`] and
    /// [`may_render_is_alive_only`] all name their inputs through it, so appending
    /// a variant to [`wolf_core::Liveness`] is an `E0004` in this test module
    /// rather than a row that silently went missing from three tables at once.
    fn liveness_name(liveness: Option<Liveness>) -> &'static str {
        match liveness {
            Some(Liveness::Alive) => "a live core",
            Some(Liveness::NotResponding) => "a wedged (NotResponding) core",
            Some(Liveness::Dead) => "a dead core",
            None => "no core at all (the close handler took it out)",
        }
    }

    /// Every input the three gated seams are total over: the three readings a core
    /// can produce, plus the ABSENT core the window's close handler leaves behind.
    const ALL_LIVENESS_INPUTS: [Option<Liveness>; 4] = [
        Some(Liveness::Alive),
        Some(Liveness::NotResponding),
        Some(Liveness::Dead),
        None,
    ];

    /// How many inputs each gated seam is total over, written as a LITERAL rather
    /// than as `ALL_LIVENESS_INPUTS.len()`, and that is the whole point of it — the
    /// `clipboard_ui::tests::NAMED_LIVENESS_INPUTS` precedent. Comparing a table's
    /// length to the length of the array the table was built from is a TAUTOLOGY:
    /// both shrink together, so the check passes while the sweep covers less than
    /// the domain it claims to.
    const NAMED_LIVENESS_INPUTS: usize = 4;

    /// Both buffer-kind gates, so a press table can be stated over the full
    /// `(liveness, gate)` product.
    const ALL_GATES: [bool; 2] = [true, false];

    /// The pane counts the whole-domain sweeps walk: below the split branch, at it,
    /// at the reuse branch's exact arity, and two counts above it.
    const SWEEP_PANE_COUNTS: [usize; 5] = [0, 1, 2, 3, 4];

    /// Both values of `active_is_preview`.
    const SWEEP_FLAGS: [bool; 2] = [false, true];

    /// The counterpart-pane counts the sweeps walk: none, the one the reuse branch
    /// requires, and more than one.
    const SWEEP_COUNTERPARTS: [usize; 3] = [0, 1, 2];

    /// How many placements each sweep visits: five pane counts times two polarities
    /// times three counterpart counts.
    ///
    /// LITERALS rather than the product of the three arrays' lengths, for the reason
    /// `nav_ui::tests::SWEEP_INPUTS` states: an array that lost an entry shrinks both
    /// sides of such a comparison together, so it passes while the sweep silently
    /// covers less than the domain it names.
    const SWEEP_PLACEMENTS: usize = 5 * 2 * 3;

    /// How many `(pane_count, active_is_preview, other_preview_panes,
    /// other_editor_panes)` shapes the CROSSED sweep visits: the two counterpart
    /// counts varied INDEPENDENTLY, because the two seams read different ones and a
    /// window really can hold, say, one other preview and two other editors.
    ///
    /// Literals for the same reason [`SWEEP_PLACEMENTS`] is.
    const CROSSED_PLACEMENTS: usize = 5 * 2 * 3 * 3;

    /// Every [`PreviewPlacement`] the sweeps walk, in a fixed order.
    fn all_preview_placements() -> Vec<PreviewPlacement> {
        let mut out = Vec::new();
        for pane_count in SWEEP_PANE_COUNTS {
            for active_is_preview in SWEEP_FLAGS {
                for other_preview_panes in SWEEP_COUNTERPARTS {
                    out.push(PreviewPlacement {
                        pane_count,
                        active_is_preview,
                        other_preview_panes,
                    });
                }
            }
        }
        out
    }

    /// Every [`EditPlacement`] the sweeps walk, in a fixed order.
    fn all_edit_placements() -> Vec<EditPlacement> {
        let mut out = Vec::new();
        for pane_count in SWEEP_PANE_COUNTS {
            for active_is_preview in SWEEP_FLAGS {
                for other_editor_panes in SWEEP_COUNTERPARTS {
                    out.push(EditPlacement {
                        pane_count,
                        active_is_preview,
                        other_editor_panes,
                    });
                }
            }
        }
        out
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from `clipboard_ui.rs`'s helper of the same name, including
    /// its deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    // =========================================================================
    // preview_placement
    // =========================================================================

    /// One [`preview_placement`] table row.
    struct PreviewPlacementCase {
        desc: &'static str,
        input: PreviewPlacement,
        expected: &'static str,
    }

    /// How many named rows the preview placement table carries, as a literal.
    const NAMED_PREVIEW_PLACEMENT_ROWS: usize = 7;

    /// Pins where a Preview press opens its pane, one row per named shape.
    ///
    /// Property: the split branch is `pane_count <= 1` and dominates both preview
    /// inputs; the reuse branch is scoped to EXACTLY two panes, an editor active and
    /// EXACTLY one other preview; everything else replaces the active pane.
    ///
    /// The four `active` rows are what give the reuse row meaning. Row 4 breaks the
    /// "an editor is active" clause, rows 5 and 6 break the "exactly one other
    /// preview" clause from either side of it, and row 7 breaks the "exactly two
    /// panes" clause — so a seam that dropped any single clause reddens on a row
    /// that names which clause went, rather than on all of them at once.
    ///
    /// Falsifier: change `pane_count <= 1` to `pane_count < 1`. A lone pane then
    /// takes the `active` branch and the preview replaces the only source pane
    /// there is, leaving the user's document nowhere on screen.
    #[test]
    fn preview_placement_table() {
        let cases = [
            PreviewPlacementCase {
                desc: "0 panes: a degenerate count falls to the split branch, never to active",
                input: PreviewPlacement {
                    pane_count: 0,
                    active_is_preview: false,
                    other_preview_panes: 0,
                },
                expected: MODE_SPLIT_RIGHT,
            },
            PreviewPlacementCase {
                desc: "1 pane: the ordinary first Preview press, and the shape a user meets first",
                input: PreviewPlacement {
                    pane_count: 1,
                    active_is_preview: false,
                    other_preview_panes: 0,
                },
                expected: MODE_SPLIT_RIGHT,
            },
            PreviewPlacementCase {
                desc: "2 panes, editor active, exactly ONE other preview: the ONLY reuse shape",
                input: PreviewPlacement {
                    pane_count: 2,
                    active_is_preview: false,
                    other_preview_panes: 1,
                },
                expected: MODE_REUSE_PREVIEW_PANE,
            },
            PreviewPlacementCase {
                desc: "2 panes but the ACTIVE pane is itself a preview: reuse is declined",
                input: PreviewPlacement {
                    pane_count: 2,
                    active_is_preview: true,
                    other_preview_panes: 1,
                },
                expected: MODE_ACTIVE,
            },
            PreviewPlacementCase {
                desc: "2 panes with NO other preview: there is nothing to reuse",
                input: PreviewPlacement {
                    pane_count: 2,
                    active_is_preview: false,
                    other_preview_panes: 0,
                },
                expected: MODE_ACTIVE,
            },
            PreviewPlacementCase {
                desc: "2 panes with TWO other previews: ambiguous, so reuse is declined",
                input: PreviewPlacement {
                    pane_count: 2,
                    active_is_preview: false,
                    other_preview_panes: 2,
                },
                expected: MODE_ACTIVE,
            },
            PreviewPlacementCase {
                desc: "3 panes: the reuse branch is scoped to exactly two, so this stays active",
                input: PreviewPlacement {
                    pane_count: 3,
                    active_is_preview: false,
                    other_preview_panes: 1,
                },
                expected: MODE_ACTIVE,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_PREVIEW_PLACEMENT_ROWS,
            "the preview placement table must carry its {NAMED_PREVIEW_PLACEMENT_ROWS} named rows, \
             not {}. They are not a sample: three of them break one clause of the reuse condition \
             each, so dropping any single row leaves one clause of that condition unmeasured and a \
             seam that ignored it would pass.",
            cases.len()
        );

        for case in &cases {
            let got = preview_placement(case.input);
            assert_eq!(
                got, case.expected,
                "preview_placement({:?}) must be {:?}, got {got:?} — {}. The mode string is \
                 dispatched on LITERALLY by `runtime/nvim/init.lua`'s `wolf_open_preview(mode)`, so \
                 a wrong one is not a near miss: `split_right` where `active` was meant opens a \
                 pane the user did not ask for, `active` where `split_right` was meant replaces the \
                 source they were reading, and `reuse_preview_pane` where `active` was meant binds \
                 the preview into a pane that is not a preview at all.",
                case.input, case.expected, case.desc
            );
        }

        // The split branch DOMINATES both preview inputs, which no row above can
        // show: every split row carries the quiet inputs, so a seam that tested the
        // reuse condition FIRST would satisfy all seven.
        let dominated = PreviewPlacement {
            pane_count: 1,
            active_is_preview: true,
            other_preview_panes: 1,
        };
        assert_eq!(
            preview_placement(dominated),
            MODE_SPLIT_RIGHT,
            "a pane_count of 1 must split right whatever the two preview inputs say. They describe \
             a two-pane window and cannot both be true here, so a seam that consulted them before \
             the pane count would be reading a shape that does not exist — and would open the \
             preview by replacing the window's only pane."
        );
    }

    // =========================================================================
    // edit_placement
    // =========================================================================

    /// One [`edit_placement`] table row.
    struct EditPlacementCase {
        desc: &'static str,
        input: EditPlacement,
        expected: &'static str,
    }

    /// How many named rows the edit placement table carries, as a literal.
    const NAMED_EDIT_PLACEMENT_ROWS: usize = 7;

    /// Pins where an Edit press lands the source editor, one row per named shape —
    /// [`preview_placement_table`] mirrored, with the polarity INVERTED.
    ///
    /// Property: `pane_count <= 1` splits LEFT unconditionally; the reuse branch is
    /// exactly two panes with a PREVIEW active and exactly one other editor;
    /// everything else replaces the active pane.
    ///
    /// Falsifier: write the reuse condition as `!p.active_is_preview`, which is
    /// [`preview_placement`]'s clause copied across. Row 4 is then `reuse_editor_pane`
    /// and row 3 `active`, so an Edit fired from the preview pane clobbers the
    /// preview instead of landing in the editor beside it — and an Edit fired from an
    /// editor pane binds the source into the OTHER editor, silently replacing a
    /// second file the user had open.
    #[test]
    fn edit_placement_table() {
        let cases = [
            EditPlacementCase {
                desc: "0 panes: a degenerate count falls to the split branch, never to active",
                input: EditPlacement {
                    pane_count: 0,
                    active_is_preview: true,
                    other_editor_panes: 0,
                },
                expected: MODE_SPLIT_LEFT,
            },
            EditPlacementCase {
                desc: "1 pane: a lone preview, so the editor needs a new pane to its LEFT",
                input: EditPlacement {
                    pane_count: 1,
                    active_is_preview: true,
                    other_editor_panes: 0,
                },
                expected: MODE_SPLIT_LEFT,
            },
            EditPlacementCase {
                desc: "2 panes, PREVIEW active, exactly ONE other editor: the ONLY reuse shape",
                input: EditPlacement {
                    pane_count: 2,
                    active_is_preview: true,
                    other_editor_panes: 1,
                },
                expected: MODE_REUSE_EDITOR_PANE,
            },
            EditPlacementCase {
                desc: "2 panes but the ACTIVE pane is NOT a preview: reuse is declined (the \
                       INVERTED clause)",
                input: EditPlacement {
                    pane_count: 2,
                    active_is_preview: false,
                    other_editor_panes: 1,
                },
                expected: MODE_ACTIVE,
            },
            EditPlacementCase {
                desc: "2 panes with NO other editor: there is nothing to reuse",
                input: EditPlacement {
                    pane_count: 2,
                    active_is_preview: true,
                    other_editor_panes: 0,
                },
                expected: MODE_ACTIVE,
            },
            EditPlacementCase {
                desc: "2 panes with TWO other editors: ambiguous, so reuse is declined",
                input: EditPlacement {
                    pane_count: 2,
                    active_is_preview: true,
                    other_editor_panes: 2,
                },
                expected: MODE_ACTIVE,
            },
            EditPlacementCase {
                desc: "3 panes: the reuse branch is scoped to exactly two, so this stays active",
                input: EditPlacement {
                    pane_count: 3,
                    active_is_preview: true,
                    other_editor_panes: 1,
                },
                expected: MODE_ACTIVE,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_EDIT_PLACEMENT_ROWS,
            "the edit placement table must carry its {NAMED_EDIT_PLACEMENT_ROWS} named rows, not \
             {}. Rows 3 and 4 are the pair that must never go: they are the two whose expectations \
             depend on the polarity being INVERTED relative to preview_placement — row 3 reuses \
             with a preview active, row 4 declines with an editor active — so without them the \
             two seams could share one implementation and nothing here would notice.",
            cases.len()
        );

        for case in &cases {
            let got = edit_placement(case.input);
            assert_eq!(
                got, case.expected,
                "edit_placement({:?}) must be {:?}, got {got:?} — {}. The mode string is dispatched \
                 on LITERALLY by `runtime/nvim/init.lua`'s `wolf_edit_source(mode)`: `split_left` \
                 where `active` was meant opens a pane nobody asked for, `active` where \
                 `split_left` was meant replaces the lone preview so the preview vanishes, and \
                 `reuse_editor_pane` where `active` was meant binds the source over a second file \
                 the user had open in the other pane.",
                case.input, case.expected, case.desc
            );
        }

        // The split branch DOMINATES both inputs here too, for the same reason it
        // does on the preview side.
        let dominated = EditPlacement {
            pane_count: 1,
            active_is_preview: false,
            other_editor_panes: 1,
        };
        assert_eq!(
            edit_placement(dominated),
            MODE_SPLIT_LEFT,
            "a pane_count of 1 must split left whatever the two inputs say. They describe a \
             two-pane window and cannot hold here, so a seam that consulted them before the pane \
             count would be reading a shape that does not exist."
        );
    }

    // =========================================================================
    // the two placements, stated over the whole domain
    // =========================================================================

    /// Every mode constant, paired with the name a failure should quote.
    const ALL_MODES: [(&str, &str); 5] = [
        ("MODE_SPLIT_RIGHT", MODE_SPLIT_RIGHT),
        ("MODE_ACTIVE", MODE_ACTIVE),
        ("MODE_REUSE_PREVIEW_PANE", MODE_REUSE_PREVIEW_PANE),
        ("MODE_SPLIT_LEFT", MODE_SPLIT_LEFT),
        ("MODE_REUSE_EDITOR_PANE", MODE_REUSE_EDITOR_PANE),
    ];

    /// The five mode constants are distinct and non-empty, neither placement ever
    /// answers with the OTHER one's exclusive modes, and the two REUSE branches are
    /// separated by the polarity of `active_is_preview`.
    ///
    /// Property 1 — the RANGES. `preview_placement` ranges over exactly
    /// `{split_right, reuse_preview_pane, active}` and `edit_placement` over exactly
    /// `{split_left, reuse_editor_pane, active}`, over all
    /// [`SWEEP_PLACEMENTS`] inputs each. Asserting the range EXACTLY, rather than
    /// asserting the absence of the other side's modes, is what stops the rule
    /// holding vacuously: a seam that returned `MODE_ACTIVE` for everything answers
    /// with none of the other side's modes and would satisfy a negative-only test
    /// while making both splits unreachable.
    ///
    /// Property 2 — the SEPARATION, over the [`CROSSED_PLACEMENTS`] shapes both seams
    /// can be handed at once. They never take their reuse branch together, and each
    /// takes it at exactly ONE value of `active_is_preview` — Preview at `false`, Edit
    /// at `true`. **The ranges do not imply this and cannot see it.** Measured: with
    /// `edit_placement`'s reuse clause written as `!p.active_is_preview`, the whole
    /// `--lib` suite went 253 to 252 passed with `edit_placement_table` the single
    /// failure, and this test stayed GREEN — an inverted clause still produces all
    /// three modes, just for different inputs. Property 2 is what closes that, so the
    /// module docs' claim about the polarity is stated over the whole input domain
    /// rather than resting on one table row.
    ///
    /// The literal values are pinned here rather than left to the constants alone,
    /// because they are a contract with `runtime/nvim/init.lua`, which dispatches on
    /// the literal string and falls through to binding the current window when it
    /// does not recognise one. A rename is silent on the Lua side and would replace
    /// the pane the user is reading.
    ///
    /// Falsifiers, one per property: for the ranges, return [`MODE_SPLIT_RIGHT`] from
    /// `edit_placement`'s `pane_count <= 1` branch — the copy-paste that the two seams
    /// sitting in one file invites. For the separation, write `edit_placement`'s reuse
    /// clause as `!p.active_is_preview`, which is the SAME copy-paste made one clause
    /// lower down; deleting that clause outright fails it too, from the opposite side
    /// (an empty polarity set rather than an inverted one).
    #[test]
    fn the_two_placements_are_inverses() {
        // --- the constants themselves ----------------------------------------
        let mut seen: HashSet<&str> = HashSet::new();
        for (name, value) in ALL_MODES {
            assert!(
                !value.trim().is_empty(),
                "{name} is empty. It is threaded verbatim into `wolf_open_preview(mode)` / \
                 `wolf_edit_source(mode)`, whose dispatch is a chain of string comparisons — an \
                 empty mode matches none of them and falls through to binding the current window."
            );
            assert!(
                seen.insert(value),
                "{name} ({value:?}) duplicates another mode constant. The Lua dispatcher branches \
                 on the literal string, so two modes sharing one value are ONE branch: the \
                 placement decision this seam exists to make would silently stop being made."
            );
        }
        assert_eq!(
            seen.len(),
            ALL_MODES.len(),
            "the {} mode constants collapsed into {} distinct string(s); see the message above for \
             which pair.",
            ALL_MODES.len(),
            seen.len()
        );

        // The literal values, pinned against `runtime/nvim/init.lua`'s dispatch.
        assert_eq!(
            [
                MODE_SPLIT_RIGHT,
                MODE_ACTIVE,
                MODE_REUSE_PREVIEW_PANE,
                MODE_SPLIT_LEFT,
                MODE_REUSE_EDITOR_PANE
            ],
            [
                "split_right",
                "active",
                "reuse_preview_pane",
                "split_left",
                "reuse_editor_pane"
            ],
            "the mode strings are a CROSS-LANGUAGE contract: `runtime/nvim/init.lua`'s \
             `wolf_open_preview` and `wolf_edit_source` compare `mode` against these exact \
             literals and fall through to binding the CURRENT window when none matches. Renaming \
             one here compiles, ships, and silently turns every placement into \"replace the pane \
             the user is looking at\"."
        );

        // --- preview_placement's range ---------------------------------------
        let mut probed = 0_usize;
        let mut preview_modes: HashSet<&str> = HashSet::new();
        for placement in all_preview_placements() {
            probed += 1;
            preview_modes.insert(preview_placement(placement));
        }
        assert_eq!(
            probed, SWEEP_PLACEMENTS,
            "the preview sweep visited {probed} placements, not the {SWEEP_PLACEMENTS} it claims \
             to. A SHRUNK domain means the range assertion below is stated over less than the \
             input space it names."
        );

        let want_preview: HashSet<&str> = [MODE_SPLIT_RIGHT, MODE_REUSE_PREVIEW_PANE, MODE_ACTIVE]
            .into_iter()
            .collect();
        assert_eq!(
            preview_modes, want_preview,
            "preview_placement's range over all {SWEEP_PLACEMENTS} placements must be exactly \
             {{split_right, reuse_preview_pane, active}}. An EXTRA mode means it answered with an \
             edit-only mode — `split_left` opens the new pane on the wrong side and \
             `reuse_editor_pane` binds the preview into the pane holding the user's source. A \
             MISSING mode means a whole branch is unreachable: no `split_right` means Preview \
             never opens a second pane and always replaces the source in place."
        );

        // --- edit_placement's range ------------------------------------------
        let mut probed = 0_usize;
        let mut edit_modes: HashSet<&str> = HashSet::new();
        for placement in all_edit_placements() {
            probed += 1;
            edit_modes.insert(edit_placement(placement));
        }
        assert_eq!(
            probed, SWEEP_PLACEMENTS,
            "the edit sweep visited {probed} placements, not the {SWEEP_PLACEMENTS} it claims to."
        );

        let want_edit: HashSet<&str> = [MODE_SPLIT_LEFT, MODE_REUSE_EDITOR_PANE, MODE_ACTIVE]
            .into_iter()
            .collect();
        assert_eq!(
            edit_modes, want_edit,
            "edit_placement's range over all {SWEEP_PLACEMENTS} placements must be exactly \
             {{split_left, reuse_editor_pane, active}}. An EXTRA mode is the copy-paste this test \
             exists for: `split_right` here puts the source editor on the right of the preview, \
             inverting the one thing the split-left branch is named for, and \
             `reuse_preview_pane` binds the SOURCE into the preview pane."
        );

        // MODE_ACTIVE is deliberately shared, and that is a decision rather than an
        // oversight: it is the same instruction to Neovim on both sides. Stated here
        // so nobody "completes" the symmetry by inventing a second constant.
        assert!(
            preview_modes.contains(MODE_ACTIVE) && edit_modes.contains(MODE_ACTIVE),
            "both placements must be able to answer MODE_ACTIVE. It is deliberately SHARED — \
             `nvim_win_set_buf` on the current window is the same instruction whichever direction \
             asked for it — so splitting it into two constants would add a Lua branch for no \
             behaviour."
        );

        // --- the SEPARATION, over the CROSSED domain --------------------------
        //
        // The two range assertions above hold for an INVERTED `edit_placement` — an
        // inverted clause still produces all three of its modes, just for different
        // inputs — so they say nothing about the polarity at all. This is what does:
        // over every shape both seams can be handed at once, they never reuse
        // TOGETHER, and each reuses at exactly ONE value of `active_is_preview`.
        let mut crossed = 0_usize;
        let mut preview_reuse_polarities: HashSet<bool> = HashSet::new();
        let mut edit_reuse_polarities: HashSet<bool> = HashSet::new();
        for pane_count in SWEEP_PANE_COUNTS {
            for active_is_preview in SWEEP_FLAGS {
                for other_preview_panes in SWEEP_COUNTERPARTS {
                    for other_editor_panes in SWEEP_COUNTERPARTS {
                        crossed += 1;
                        let preview = preview_placement(PreviewPlacement {
                            pane_count,
                            active_is_preview,
                            other_preview_panes,
                        });
                        let edit = edit_placement(EditPlacement {
                            pane_count,
                            active_is_preview,
                            other_editor_panes,
                        });
                        let both_reuse =
                            preview == MODE_REUSE_PREVIEW_PANE && edit == MODE_REUSE_EDITOR_PANE;
                        assert!(
                            !both_reuse,
                            "both seams took their REUSE branch for one window shape (pane_count = \
                             {pane_count}, active_is_preview = {active_is_preview}, \
                             other_preview_panes = {other_preview_panes}, other_editor_panes = \
                             {other_editor_panes}). They are inverses: Preview reuses a counterpart \
                             when the active pane is NOT a preview, Edit when it IS, so no single \
                             window can satisfy both. Both firing means the two conditions agree on \
                             the polarity — the copy-paste this file's module docs name as the \
                             single most likely edit here — and an Edit fired from the preview pane \
                             would clobber the preview it was fired from."
                        );
                        if preview == MODE_REUSE_PREVIEW_PANE {
                            preview_reuse_polarities.insert(active_is_preview);
                        }
                        if edit == MODE_REUSE_EDITOR_PANE {
                            edit_reuse_polarities.insert(active_is_preview);
                        }
                    }
                }
            }
        }
        assert_eq!(
            crossed, CROSSED_PLACEMENTS,
            "the crossed sweep visited {crossed} shapes, not the {CROSSED_PLACEMENTS} it claims \
             to. A SHRUNK domain means the separation below is stated over less than the input \
             space it names — and the shape most likely to go missing is a two-pane window with \
             one counterpart of EACH kind, which is the only shape where both reuse branches are \
             otherwise eligible at once."
        );

        // Stated as a SET rather than as "never both", because "never both" alone is
        // satisfied by a seam whose reuse branch is unreachable. An EMPTY set is
        // exactly that, and it fails here.
        let editor_active: HashSet<bool> = [false].into_iter().collect();
        let preview_active: HashSet<bool> = [true].into_iter().collect();
        assert_eq!(
            preview_reuse_polarities, editor_active,
            "preview_placement reused a preview pane at active_is_preview values \
             {preview_reuse_polarities:?}, and it must be exactly {{false}}. An EMPTY set means the \
             reuse branch is unreachable, so a second Preview press opens a THIRD pane instead of \
             re-using the one already showing a preview. A set containing `true` means the clause \
             lost its negation, and a Preview fired from inside the preview pane binds the new \
             preview over the pane the user was reading."
        );
        assert_eq!(
            edit_reuse_polarities, preview_active,
            "edit_placement reused an editor pane at active_is_preview values \
             {edit_reuse_polarities:?}, and it must be exactly {{true}} — the INVERSE of \
             preview_placement's {{false}} above. `{{false}}` here is `preview_placement`'s clause \
             copied across verbatim: Edit then declines to reuse from the preview pane (the one \
             place it is ever fired from) and replaces that pane's buffer instead, while an Edit \
             from an editor pane binds the source over a second file the user had open. That is the \
             single most likely edit in this file, and without this clause \
             `edit_placement_table`'s row 4 is the only thing that can see it."
        );
    }

    // =========================================================================
    // preview_press
    // =========================================================================

    /// What a press-table row expects.
    ///
    /// A refusal is matched on its CLASS, not on its reason string: the reasons are
    /// documentation and their real contract is asserted by
    /// [`refusal_reasons_are_distinct_and_mid_sentence`] — the
    /// `clipboard_ui::tests::WantCopy` precedent.
    #[derive(Clone, Copy)]
    enum WantPress {
        /// The BLOCKING RPC is issued, at the mode the placement function gives.
        Act,
        /// No RPC at all.
        Refuse,
    }

    /// One press-table row. Shared by both directions, since the shape is identical.
    struct PressCase {
        liveness: Option<Liveness>,
        /// `markdown_active` on the preview side, `preview_active` on the edit side.
        gate: bool,
        want: WantPress,
    }

    /// The `(liveness, gate)` product both press seams are total over, stated once.
    ///
    /// The only accepting row is a live core with its gate open; the seven others
    /// refuse. Written out rather than generated, so a row cannot be lost to the
    /// same edit that would lose the generator.
    const PRESS_CASES: &[PressCase] = &[
        PressCase {
            liveness: Some(Liveness::Alive),
            gate: true,
            want: WantPress::Act,
        },
        PressCase {
            liveness: Some(Liveness::Alive),
            gate: false,
            want: WantPress::Refuse,
        },
        PressCase {
            liveness: Some(Liveness::NotResponding),
            gate: true,
            want: WantPress::Refuse,
        },
        PressCase {
            liveness: Some(Liveness::NotResponding),
            gate: false,
            want: WantPress::Refuse,
        },
        PressCase {
            liveness: Some(Liveness::Dead),
            gate: true,
            want: WantPress::Refuse,
        },
        PressCase {
            liveness: Some(Liveness::Dead),
            gate: false,
            want: WantPress::Refuse,
        },
        PressCase {
            liveness: None,
            gate: true,
            want: WantPress::Refuse,
        },
        PressCase {
            liveness: None,
            gate: false,
            want: WantPress::Refuse,
        },
    ];

    /// A lone editor pane — the placement every press row that is not about
    /// placement carries.
    const LONE_EDITOR: PreviewPlacement = PreviewPlacement {
        pane_count: 1,
        active_is_preview: false,
        other_preview_panes: 0,
    };

    /// A lone preview pane, the Edit side's counterpart to [`LONE_EDITOR`].
    const LONE_PREVIEW: EditPlacement = EditPlacement {
        pane_count: 1,
        active_is_preview: true,
        other_editor_panes: 0,
    };

    /// Pins the Preview press for every `(Option<Liveness>, markdown_active)` input,
    /// and pins that an accepted press carries [`preview_placement`]'s own answer.
    ///
    /// Three properties, and the second is the one no other test can see.
    ///
    /// 1. Only a live core with a Markdown buffer active may issue the BLOCKING
    ///    `open_preview`; the other seven inputs refuse and issue nothing.
    /// 2. LIVENESS IS READ FIRST. A dead or wedged core refuses with the SAME reason
    ///    whether or not the Markdown gate is open, which is only true if the gate is
    ///    never consulted on those arms. Falsifier: test `markdown_active` before the
    ///    liveness match. The two refusal reasons for a dead core then differ, and a
    ///    user whose Neovim has crashed is told their buffer is not Markdown — about
    ///    a snapshot that stopped advancing when the editor did.
    /// 3. An accepted press carries the placement mode for the shape it was handed,
    ///    over the whole placement sweep. Stated against `preview_placement` rather
    ///    than against literals so the two cannot drift; the modes themselves are
    ///    pinned independently by [`preview_placement_table`], which is what stops
    ///    this clause holding for two wrong answers that happen to agree.
    #[test]
    fn preview_press_table() {
        assert_eq!(
            PRESS_CASES.len(),
            NAMED_LIVENESS_INPUTS * ALL_GATES.len(),
            "the press table must carry one row per (liveness, gate) input the seam is total over: \
             {NAMED_LIVENESS_INPUTS} liveness inputs (three readings plus the absent core) times \
             {} gate values is {}, not {}. The row most likely to go missing is a non-Alive core \
             with the gate CLOSED, which is exactly the pair that measures the precedence rule.",
            ALL_GATES.len(),
            NAMED_LIVENESS_INPUTS * ALL_GATES.len(),
            PRESS_CASES.len()
        );
        assert_eq!(
            ALL_LIVENESS_INPUTS.len(),
            NAMED_LIVENESS_INPUTS,
            "ALL_LIVENESS_INPUTS carries {} entries but the seam is total over \
             {NAMED_LIVENESS_INPUTS}. If `wolf_core::Liveness` gained a variant, `liveness_name` \
             and the `_`-free matches in this module are an E0004 already and this constant is the \
             second thing to update.",
            ALL_LIVENESS_INPUTS.len()
        );

        // --- 1: the decision itself ------------------------------------------
        for case in PRESS_CASES {
            let got = preview_press(case.liveness, case.gate, LONE_EDITOR);
            match case.want {
                WantPress::Act => assert_eq!(
                    got,
                    PreviewPress::Open(MODE_SPLIT_RIGHT),
                    "preview_press({}, markdown_active = {}) returned {got:?}, but this input MUST \
                     open at {MODE_SPLIT_RIGHT:?}. A refusal here makes the Preview command do \
                     nothing at all on a perfectly healthy editor showing a Markdown file, and \
                     prints a reason for a hazard that is not present.",
                    liveness_name(case.liveness),
                    case.gate
                ),
                WantPress::Refuse => assert!(
                    matches!(got, PreviewPress::Refuse(_)),
                    "preview_press({}, markdown_active = {}) returned {got:?}, but this input MUST \
                     refuse. An Open funnels through `WolfCore::open_preview` into `block_on_rpc` \
                     — a BLOCKING call on the GTK main thread — against a core known not to answer \
                     or already taken out, so every press freezes the window for another full \
                     `rpc_timeout` budget. With the Markdown gate CLOSED the hazard is different \
                     and just as real: `wolf_open_preview` returns 0 for a non-Markdown buffer, so \
                     the RPC is spent to be told nothing happened.",
                    liveness_name(case.liveness),
                    case.gate
                ),
            }
        }

        // --- 2: liveness is read BEFORE the Markdown gate ---------------------
        for liveness in ALL_LIVENESS_INPUTS {
            if liveness == Some(Liveness::Alive) {
                continue;
            }
            let with_gate = preview_press(liveness, true, LONE_EDITOR);
            let without_gate = preview_press(liveness, false, LONE_EDITOR);
            assert_eq!(
                with_gate,
                without_gate,
                "preview_press for {} answered {with_gate:?} with the Markdown gate OPEN and \
                 {without_gate:?} with it CLOSED. The two must be identical: liveness is read \
                 first, so the gate is never consulted on this arm. A difference means the gate is \
                 tested first, and the user whose Neovim has crashed or wedged is told their \
                 buffer is not Markdown — a claim read out of a snapshot that stopped advancing \
                 when the editor did.",
                liveness_name(liveness)
            );
        }
        let alive_gate_shut = preview_press(Some(Liveness::Alive), false, LONE_EDITOR);
        assert!(
            matches!(alive_gate_shut, PreviewPress::Refuse(_)),
            "a LIVE core with a non-Markdown buffer active returned {alive_gate_shut:?} rather \
             than a refusal, so the clause above compares two refusals that mean nothing. This is \
             the one arm where the Markdown gate must decide: `wolf_open_preview` no-ops on a \
             non-Markdown buffer, and issuing the blocking RPC to learn that is the cost."
        );

        // --- 3: an accepted press carries the placement's own mode ------------
        let mut probed = 0_usize;
        let mut opened: HashSet<&str> = HashSet::new();
        for placement in all_preview_placements() {
            probed += 1;
            let got = preview_press(Some(Liveness::Alive), true, placement);
            let expected = preview_placement(placement);
            assert_eq!(
                got,
                PreviewPress::Open(expected),
                "preview_press(Alive, markdown_active = true, {placement:?}) returned {got:?}, but \
                 an accepted press must carry `preview_placement`'s own answer ({expected:?}). The \
                 press path and the placement path are the only two answers to \"where does this \
                 preview go\", and a press that decided for itself would open somewhere the \
                 placement table never pinned."
            );
            let PreviewPress::Open(mode) = got else {
                unreachable!("the assertion above already required an Open")
            };
            opened.insert(mode);
        }
        assert_eq!(
            probed, SWEEP_PLACEMENTS,
            "the accepted-press sweep visited {probed} placements, not {SWEEP_PLACEMENTS}."
        );
        assert!(
            opened.len() > 1,
            "every accepted press over all {SWEEP_PLACEMENTS} placements carried the same mode \
             ({opened:?}), so the clause above holds for a `preview_press` that ignores its \
             placement argument entirely AND a `preview_placement` that is a constant. The \
             placement table is what pins the individual modes; this is what proves the press \
             carries more than one of them."
        );
    }

    // =========================================================================
    // edit_press
    // =========================================================================

    /// Pins the Edit press for every `(Option<Liveness>, preview_active)` input, and
    /// pins that an accepted press carries [`edit_placement`]'s own answer.
    ///
    /// [`preview_press_table`]'s three properties, mirrored. The gate is
    /// `preview_active` here: `wolf_edit_source` reads `b:wolf_preview_source` off
    /// the current buffer and returns 0 unless it is a preview scratch, so an Edit
    /// from an ordinary buffer spends a blocking RPC to be told nothing happened.
    ///
    /// Falsifier: test `preview_active` before the liveness match, exactly as on the
    /// preview side.
    #[test]
    fn edit_press_table() {
        // --- 1: the decision itself ------------------------------------------
        for case in PRESS_CASES {
            let got = edit_press(case.liveness, case.gate, LONE_PREVIEW);
            match case.want {
                WantPress::Act => assert_eq!(
                    got,
                    EditPress::Edit(MODE_SPLIT_LEFT),
                    "edit_press({}, preview_active = {}) returned {got:?}, but this input MUST \
                     edit at {MODE_SPLIT_LEFT:?}. A refusal here makes Edit Source do nothing at \
                     all from a healthy preview pane, which is the ONE surface that pane offers.",
                    liveness_name(case.liveness),
                    case.gate
                ),
                WantPress::Refuse => assert!(
                    matches!(got, EditPress::Refuse(_)),
                    "edit_press({}, preview_active = {}) returned {got:?}, but this input MUST \
                     refuse. `WolfCore::edit_source` is a BLOCKING call on the GTK main thread, so \
                     an Edit against a core known not to answer freezes the window for another \
                     full `rpc_timeout` budget; with the preview gate CLOSED the RPC is spent to \
                     be told `wolf_edit_source` found no `b:wolf_preview_source` and did nothing.",
                    liveness_name(case.liveness),
                    case.gate
                ),
            }
        }

        // --- 2: liveness is read BEFORE the preview gate ----------------------
        for liveness in ALL_LIVENESS_INPUTS {
            if liveness == Some(Liveness::Alive) {
                continue;
            }
            let with_gate = edit_press(liveness, true, LONE_PREVIEW);
            let without_gate = edit_press(liveness, false, LONE_PREVIEW);
            assert_eq!(
                with_gate,
                without_gate,
                "edit_press for {} answered {with_gate:?} with the preview gate OPEN and \
                 {without_gate:?} with it CLOSED. The two must be identical: liveness is read \
                 first, so the gate is never consulted on this arm. A difference means a user \
                 whose Neovim has crashed is told they are not in a preview pane.",
                liveness_name(liveness)
            );
        }
        let alive_gate_shut = edit_press(Some(Liveness::Alive), false, LONE_PREVIEW);
        assert!(
            matches!(alive_gate_shut, EditPress::Refuse(_)),
            "a LIVE core with an ordinary (non-preview) buffer active returned {alive_gate_shut:?} \
             rather than a refusal, so the clause above compares two refusals that mean nothing. \
             This is the one arm where the preview gate must decide."
        );

        // --- 3: an accepted press carries the placement's own mode ------------
        let mut probed = 0_usize;
        let mut edited: HashSet<&str> = HashSet::new();
        for placement in all_edit_placements() {
            probed += 1;
            let got = edit_press(Some(Liveness::Alive), true, placement);
            let expected = edit_placement(placement);
            assert_eq!(
                got,
                EditPress::Edit(expected),
                "edit_press(Alive, preview_active = true, {placement:?}) returned {got:?}, but an \
                 accepted press must carry `edit_placement`'s own answer ({expected:?}). Deciding \
                 for itself would land the source editor somewhere the placement table never \
                 pinned."
            );
            let EditPress::Edit(mode) = got else {
                unreachable!("the assertion above already required an Edit")
            };
            edited.insert(mode);
        }
        assert_eq!(
            probed, SWEEP_PLACEMENTS,
            "the accepted-press sweep visited {probed} placements, not {SWEEP_PLACEMENTS}."
        );
        assert!(
            edited.len() > 1,
            "every accepted Edit over all {SWEEP_PLACEMENTS} placements carried the same mode \
             ({edited:?}), so the clause above holds for an `edit_press` that ignores its \
             placement argument entirely."
        );
    }

    // =========================================================================
    // preview_file_press
    // =========================================================================

    /// A followed preview link issues its BLOCKING `preview_file` for a live core and
    /// for nothing else.
    ///
    /// Property: [`preview_file_press`] answers [`PreviewFilePress::Follow`] for
    /// `Some(Liveness::Alive)` ALONE, over the whole `Option<Liveness>` domain. Stated
    /// as an exact SET as well as per input — [`may_render_is_alive_only`]'s shape —
    /// because "Alive follows" on its own is satisfied by a seam that follows for
    /// everything, which is exactly the regression the falsifier below produces.
    ///
    /// This seam takes ONE argument, and that is the decision the module docs record:
    /// whether the linked file is Markdown belongs to core's own `preview_file` gate,
    /// and answering it here would mean a filesystem touch in a seam whose whole value
    /// is purity. A `0` from core IS the refusal, and [`crate::pane_view`] prints it.
    /// So there is no `(liveness, gate)` product to sweep here, and no second gate
    /// whose ORDER against the liveness read could be got wrong — which is why this
    /// test has no counterpart to [`preview_press_table`]'s property 2.
    ///
    /// Falsifier: add `Some(Liveness::NotResponding) => PreviewFilePress::Follow`.
    /// Every link click in every preview then spends a blocking RPC against a Neovim
    /// known not to answer, freezing the GTK main loop for another full `rpc_timeout`
    /// budget — and unlike a chord, a click is something a reader does while simply
    /// reading.
    #[test]
    fn preview_file_press_is_alive_only() {
        let following: Vec<Option<Liveness>> = ALL_LIVENESS_INPUTS
            .into_iter()
            .filter(|liveness| preview_file_press(*liveness) == PreviewFilePress::Follow)
            .collect();
        let only_alive = [Some(Liveness::Alive)];

        assert_eq!(
            following.as_slice(),
            only_alive.as_slice(),
            "exactly ONE of the {NAMED_LIVENESS_INPUTS} liveness inputs may follow a preview \
             link — a core reading Alive. An EXTRA entry means the blocking `preview_file` is \
             issued against a core already known to be wedged, dead or taken out. An entry \
             MISSING means a clicked link never repoints the pane at all, which is the whole \
             feature, and the click would appear to do nothing with a printed reason naming a \
             hazard that is not present."
        );

        for liveness in ALL_LIVENESS_INPUTS {
            let got = preview_file_press(liveness);
            if liveness == Some(Liveness::Alive) {
                assert_eq!(
                    got,
                    PreviewFilePress::Follow,
                    "preview_file_press for {} must Follow. Stated per input as well as over the \
                     whole domain so a failure names the reading rather than quoting a slice.",
                    liveness_name(liveness)
                );
            } else {
                assert!(
                    matches!(got, PreviewFilePress::Refuse(_)),
                    "preview_file_press for {} returned {got:?}, but this input MUST refuse: \
                     `WolfCore::preview_file` funnels through `block_on_rpc` on the GTK main \
                     thread.",
                    liveness_name(liveness)
                );
            }
        }

        assert_eq!(
            ALL_LIVENESS_INPUTS.len(),
            NAMED_LIVENESS_INPUTS,
            "ALL_LIVENESS_INPUTS carries {} entries but the gate is total over \
             {NAMED_LIVENESS_INPUTS}; a shrunk array makes the sweep above cover less than the \
             domain it claims to.",
            ALL_LIVENESS_INPUTS.len()
        );
    }

    // =========================================================================
    // browser_press
    // =========================================================================

    /// Pins Preview in Browser for every `(Option<Liveness>, markdown_active)` input.
    ///
    /// [`preview_press_table`]'s first two properties, mirrored, and for the same
    /// reasons. There is no third: this seam carries no placement.
    ///
    /// 1. Only a live core with a Markdown buffer active may issue the BLOCKING
    ///    `open_in_browser`; the other seven inputs refuse and issue nothing.
    /// 2. LIVENESS IS READ FIRST. A dead or wedged core refuses with the SAME reason
    ///    whether or not the Markdown gate is open, which is only true if the gate is
    ///    never consulted on those arms.
    ///
    /// Falsifier for property 2: test `markdown_active` before the liveness match. The
    /// two refusal reasons for a dead core then differ, and a user whose Neovim has
    /// crashed is told their buffer is not Markdown — read out of a snapshot that
    /// stopped advancing when the editor did.
    ///
    /// **No smoke exercises this seam's accepting arm, deliberately**, so this table is
    /// the whole of its coverage. Under `cage` `gtk::UriLauncher::launch` reports
    /// `Ok(())` and invokes no handler, so there is nothing to observe; and on a host
    /// where it does work, `XDG_DATA_DIRS` reaches the system applications directory
    /// even under the smoke harness's `HOME` redirect, so a smoke would open a tab in
    /// the developer's real browser. Do not add one.
    #[test]
    fn browser_press_table() {
        // --- 1: the decision itself ------------------------------------------
        for case in PRESS_CASES {
            let got = browser_press(case.liveness, case.gate);
            match case.want {
                WantPress::Act => assert_eq!(
                    got,
                    BrowserPress::Open,
                    "browser_press({}, markdown_active = {}) returned {got:?}, but this input \
                     MUST open. A refusal here makes the View menu's Preview in Browser row do \
                     nothing at all on a healthy editor showing a Markdown file.",
                    liveness_name(case.liveness),
                    case.gate
                ),
                WantPress::Refuse => assert!(
                    matches!(got, BrowserPress::Refuse(_)),
                    "browser_press({}, markdown_active = {}) returned {got:?}, but this input \
                     MUST refuse. `WolfCore::open_in_browser` RENDERS the document and binds a \
                     loopback socket behind a blocking RPC on the GTK main thread, so an Open \
                     against a core known not to answer freezes the window for another full \
                     `rpc_timeout` budget; with the Markdown gate CLOSED it answers the empty \
                     string, so the RPC is spent to be handed nothing.",
                    liveness_name(case.liveness),
                    case.gate
                ),
            }
        }

        // --- 2: liveness is read BEFORE the Markdown gate ---------------------
        for liveness in ALL_LIVENESS_INPUTS {
            if liveness == Some(Liveness::Alive) {
                continue;
            }
            let with_gate = browser_press(liveness, true);
            let without_gate = browser_press(liveness, false);
            assert_eq!(
                with_gate,
                without_gate,
                "browser_press for {} answered {with_gate:?} with the Markdown gate OPEN and \
                 {without_gate:?} with it CLOSED. The two must be identical: liveness is read \
                 first, so the gate is never consulted on this arm.",
                liveness_name(liveness)
            );
        }
        let alive_gate_shut = browser_press(Some(Liveness::Alive), false);
        assert!(
            matches!(alive_gate_shut, BrowserPress::Refuse(_)),
            "a LIVE core with a non-Markdown buffer active returned {alive_gate_shut:?} rather \
             than a refusal, so the clause above compares two refusals that mean nothing. This is \
             the one arm where the Markdown gate must decide."
        );
    }

    // =========================================================================
    // browser_url
    // =========================================================================

    /// A loopback URL to hand the launcher, taken from what `WolfCore::open_in_browser`
    /// really answers.
    const BROWSER_URL: &str = "http://127.0.0.1:33393/alpha.md";

    /// One [`browser_url`] table row.
    struct BrowserUrlCase {
        desc: &'static str,
        raw: &'static str,
        want: Option<&'static str>,
    }

    /// How many named rows the browser-URL table carries, as a literal.
    const NAMED_BROWSER_URL_ROWS: usize = 6;

    /// Pins which strings reach the platform's URI launcher and which are dropped.
    ///
    /// Property 1: `None` for an empty or all-whitespace string, `Some(raw)` — the
    /// UNTRIMMED input, verbatim — for everything else. Property 2 (non-vacuity): the
    /// range over the sweep carries BOTH a `Some` and a `None`. As in
    /// [`document_update_table`], property 2 is BELT-AND-BRACES against `browser_url`
    /// — a function that launches everything fails property 1's `""` row long before
    /// the range is compared, and one that launches nothing fails its first row — and
    /// a live guard on this TABLE, catching a trim that deletes the last `Some` or the
    /// last `None` row and lowers [`NAMED_BROWSER_URL_ROWS`] to match.
    ///
    /// The last row is the one that looks wrong and is not. There is deliberately NO
    /// `http://127.0.0.1:` prefix check: the loopback-only bind is core's own hard
    /// security requirement (`crates/wolf-core/DECISIONS.md`), and restating it here
    /// would give one rule two owners that could drift apart — the dangerous direction
    /// being the one where this copy is the laxer, since it is the one the launcher
    /// actually consults. Deleting that row would turn its absence back into something
    /// a reader could mistake for an oversight.
    ///
    /// Falsifier: return `Some(raw)` unconditionally. The empty and all-whitespace rows
    /// redden, and the behaviour they describe is asking the desktop to open the empty
    /// URI on each of core's three error paths — an RPC failure, a non-Markdown buffer
    /// and a failure to bind the loopback socket — every one of which it has already
    /// logged.
    #[test]
    fn browser_url_table() {
        let cases = [
            BrowserUrlCase {
                desc: "the ordinary case: core's own address, passed straight through",
                raw: BROWSER_URL,
                want: Some(BROWSER_URL),
            },
            BrowserUrlCase {
                desc: "core's THREE error paths all answer the empty string",
                raw: "",
                want: None,
            },
            BrowserUrlCase {
                desc: "all-whitespace is the same nothing, spelled differently",
                raw: "   ",
                want: None,
            },
            BrowserUrlCase {
                desc: "tabs and newlines are whitespace too",
                raw: "\t\n ",
                want: None,
            },
            BrowserUrlCase {
                desc: "trimmed for the TEST, UNTRIMMED for the answer: the launcher is handed \
                       exactly what core built",
                raw: "  http://127.0.0.1:33393/alpha.md  ",
                want: Some("  http://127.0.0.1:33393/alpha.md  "),
            },
            BrowserUrlCase {
                desc: "a NON-loopback address passes: the loopback-only bind is core's rule and \
                       is deliberately not restated here",
                raw: "https://example.com/x",
                want: Some("https://example.com/x"),
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_BROWSER_URL_ROWS,
            "the browser-URL table must carry its {NAMED_BROWSER_URL_ROWS} named rows, not {}.",
            cases.len()
        );

        let mut answers: HashSet<bool> = HashSet::new();
        for case in &cases {
            let got = browser_url(case.raw);
            assert_eq!(
                got, case.want,
                "browser_url({:?}) must be {:?}, got {got:?} — {}. A spurious `Some` asks the \
                 desktop's URI launcher to open a string core has already logged as a failure; a \
                 spurious `None` makes Preview in Browser silently do nothing on a perfectly good \
                 address. A `Some` carrying a TRIMMED copy is the third failure and the quietest: \
                 the launcher would then be handed a URL this seam edited rather than the one \
                 core built.",
                case.raw, case.want, case.desc
            );
            answers.insert(got.is_some());
        }

        let both: HashSet<bool> = [true, false].into_iter().collect();
        assert_eq!(
            answers, both,
            "browser_url's range over all {NAMED_BROWSER_URL_ROWS} rows must contain BOTH a Some \
             and a None. Only `Some` means the filter is gone and every error path reaches the \
             launcher; only `None` means Preview in Browser can never launch anything at all."
        );
    }

    // =========================================================================
    // the refusal strings
    // =========================================================================

    /// How many refusal reasons this module carries, as a LITERAL: four from each of
    /// the three seams that have a buffer-kind gate ([`preview_press`], [`edit_press`],
    /// [`browser_press`]) plus three from the one that has none
    /// ([`preview_file_press`], whose `Alive` arm always follows).
    ///
    /// A literal rather than anything derived from the sweep below, for the reason
    /// [`NAMED_LIVENESS_INPUTS`] is one: a count computed from the probes shrinks with
    /// them, so a seam dropped from the sweep would leave the total agreeing with
    /// itself.
    const NAMED_REFUSAL_REASONS: usize = 15;

    /// All fifteen refusal reasons this module can print are non-empty, read as a
    /// mid-sentence clause, and are pairwise DISTINCT.
    ///
    /// Distinctness is the load-bearing half, and it is asserted ACROSS the four seams
    /// as well as within each. The reason is the whole of what a reader sees when a
    /// command appears to do nothing, and it is printed as `preview not opened:
    /// {reason}` / `source not edited: {reason}` / `preview link not followed:
    /// {reason}` / `preview in browser not opened: {reason}`. Two arms sharing a string
    /// make "the editor is wedged" indistinguishable from "the editor has crashed" —
    /// two states with completely different routes back, one of which resolves by
    /// itself — and one shared ACROSS seams makes a link that did nothing read exactly
    /// like a Preview that did nothing, which are different investigations.
    ///
    /// All four seams are swept in ONE test rather than one test each, because
    /// distinctness is a property of the whole module: a per-seam test checks a seam's
    /// strings against each other and leaves them free to collide with another seam's,
    /// which is the collision a reader is least able to spot — two files apart, two
    /// prefixes apart, and identical on the line the user actually reads.
    ///
    /// [`preview_file_press`]'s `Alive` arm is asserted to FOLLOW rather than skipped
    /// silently, so "this seam contributes three reasons and not four" is measured
    /// rather than assumed — it is the one seam here with no second gate.
    ///
    /// Falsifier: give `Some(Liveness::Dead)` and `Some(Liveness::NotResponding)` the
    /// same string on any one seam, or give two seams' `None` arms the same string.
    #[test]
    fn refusal_reasons_are_distinct_and_mid_sentence() {
        let probes = [
            (
                "a live core whose active buffer fails the gate",
                Some(Liveness::Alive),
                false,
            ),
            (
                "a wedged (NotResponding) core",
                Some(Liveness::NotResponding),
                true,
            ),
            ("a dead core", Some(Liveness::Dead), true),
            ("an absent core, after shutdown took it out", None, true),
        ];

        // The one seam with no second gate: its Alive arm ACTS, which is why it
        // contributes three reasons rather than four. Measured here so the count below
        // rests on a check rather than on a belief.
        assert_eq!(
            preview_file_press(Some(Liveness::Alive)),
            PreviewFilePress::Follow,
            "preview_file_press(Alive) refused. It has NO buffer-kind gate — the linked file's \
             Markdown-ness is core's own `preview_file` gate and this seam cannot answer it \
             without touching the filesystem — so its live arm must always follow. If it now \
             refuses it contributes a SIXTEENTH reason and the count below is wrong for a reason \
             that has nothing to do with distinctness."
        );

        let mut seen: HashSet<&'static str> = HashSet::new();
        let mut collected = 0_usize;

        for (source, liveness, gate) in probes {
            // Rendered BEFORE the destructuring, because none of the press types is
            // `Copy` and the `else` arm must be able to name what it actually got.
            let preview = preview_press(liveness, gate, LONE_EDITOR);
            let preview_shown = format!("{preview:?}");
            let PreviewPress::Refuse(preview_reason) = preview else {
                panic!(
                    "preview_press for {source} produced {preview_shown} rather than a refusal, so \
                     this test cannot check the reason it exists to check. Either that arm now \
                     issues the blocking open (in which case `preview_press_table` is also red, \
                     and that is the finding) or the probe no longer selects the arm it names."
                )
            };
            let edit = edit_press(liveness, gate, LONE_PREVIEW);
            let edit_shown = format!("{edit:?}");
            let EditPress::Refuse(edit_reason) = edit else {
                panic!(
                    "edit_press for {source} produced {edit_shown} rather than a refusal, so this \
                     test cannot check the reason it exists to check. Either that arm now issues \
                     the blocking edit (in which case `edit_press_table` is also red) or the probe \
                     no longer selects the arm it names."
                )
            };
            let browser = browser_press(liveness, gate);
            let browser_shown = format!("{browser:?}");
            let BrowserPress::Refuse(browser_reason) = browser else {
                panic!(
                    "browser_press for {source} produced {browser_shown} rather than a refusal, so \
                     this test cannot check the reason it exists to check. Either that arm now \
                     issues the blocking open_in_browser (in which case `browser_press_table` is \
                     also red) or the probe no longer selects the arm it names."
                )
            };

            let mut reasons = vec![
                ("preview", preview_reason),
                ("edit", edit_reason),
                ("preview in browser", browser_reason),
            ];

            // The link seam has no gate, so its `Alive` probe ACTS and contributes no
            // reason. Skipped by the liveness rather than by the probe index, so a
            // reordered probe list cannot silently drop a different arm.
            if liveness != Some(Liveness::Alive) {
                let link = preview_file_press(liveness);
                let link_shown = format!("{link:?}");
                let PreviewFilePress::Refuse(link_reason) = link else {
                    panic!(
                        "preview_file_press for {source} produced {link_shown} rather than a \
                         refusal, so this test cannot check the reason it exists to check. Either \
                         that arm now issues the blocking preview_file (in which case \
                         `preview_file_press_is_alive_only` is also red) or the probe no longer \
                         selects the arm it names."
                    )
                };
                reasons.push(("preview link", link_reason));
            }

            for (seam, reason) in reasons {
                collected += 1;
                assert!(
                    !reason.trim().is_empty(),
                    "the {seam} refusal reason for {source} is empty. It is printed as \
                     \"wolf-gnome: {seam} not performed: {{reason}}\", so an empty one makes a \
                     deliberate refusal indistinguishable from a command that silently did \
                     nothing."
                );
                assert!(
                    !is_sentence_cased(reason),
                    "the {seam} refusal reason for {source} is sentence-cased ({reason:?}). It is \
                     interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                     `command_action.rs`'s, `nav_ui.rs`'s and `clipboard_ui.rs`'s reasons are, so \
                     it must start lowercase (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
                );
                assert!(
                    seen.insert(reason),
                    "the {seam} refusal reason for {source} ({reason:?}) duplicates another arm's \
                     reason. All {NAMED_REFUSAL_REASONS} must be DISTINCT so the printed line \
                     names the real blocker: a wedged Neovim is recoverable and a dead one is \
                     not, an absent core means the window is already tearing down, and a closed \
                     buffer-kind gate is not a core problem at all. Across seams the collapse is \
                     worse still, because the four prefixes name four different surfaces — a \
                     command, a menu row, its inverse and a CLICK inside a rendered document — \
                     and the reason is the only part of the line that says which one refused."
                );
            }
        }

        assert_eq!(
            collected, NAMED_REFUSAL_REASONS,
            "the sweep collected {collected} refusal reason(s), not the \
             {NAMED_REFUSAL_REASONS} this module carries (four each from preview_press, \
             edit_press and browser_press, three from preview_file_press). FEWER means a seam \
             stopped being probed and its strings are unguarded; MORE means one gained an arm \
             nothing here classified."
        );
        assert_eq!(
            seen.len(),
            NAMED_REFUSAL_REASONS,
            "the {NAMED_REFUSAL_REASONS} probed refusals produced {} distinct reason(s). See the \
             per-probe message above for which pair collapsed.",
            seen.len()
        );
    }

    // =========================================================================
    // needs_reload
    // =========================================================================

    /// The source buffer a preview pane is bound to first.
    const SOURCE_A: PreviewLoad = PreviewLoad {
        source_bufnr: 7,
        source_tick: 5,
    };

    /// A DIFFERENT source buffer at the SAME `changedtick` — the shape two freshly
    /// opened files really take, and the one a tick-only gate cannot see.
    const SOURCE_B_SAME_TICK: PreviewLoad = PreviewLoad {
        source_bufnr: 12,
        source_tick: 5,
    };

    /// The SAME source buffer one edit later.
    const SOURCE_A_EDITED: PreviewLoad = PreviewLoad {
        source_bufnr: 7,
        source_tick: 6,
    };

    /// One [`needs_reload`] table row.
    struct ReloadCase {
        desc: &'static str,
        current: Option<PreviewLoad>,
        loaded: Option<PreviewLoad>,
        want: bool,
    }

    /// How many named rows the reload table carries, as a literal.
    const NAMED_RELOAD_ROWS: usize = 6;

    /// Pins when a preview pane's web view is reloaded.
    ///
    /// Property: reload exactly when the pane IS a preview and its live
    /// `(source_bufnr, source_tick)` differs from what was loaded.
    ///
    /// **Row 4 is the row that matters, and it is the reason this seam keys on a
    /// pair.** Two freshly opened files share a `changedtick`, so a reused preview
    /// pane switching sources presents a NEW buffer at the SAME tick. That is the
    /// defect `PreviewSeam.PreviewLoad` records on macOS, and its symptom is quiet:
    /// the previous document stays on screen under the new file's statusbar, so the
    /// preview looks alive and is simply wrong.
    ///
    /// Row 6 is what keeps row 4 from being satisfied by the opposite mistake. Rows
    /// 1 to 5 are ALL satisfied by a gate that compares `source_bufnr` alone, which
    /// would stop reloading on every edit of the file being previewed — the single
    /// most common thing a live preview has to do.
    ///
    /// Falsifier: compare `source_tick` alone. Row 4 goes red and nothing else does.
    #[test]
    fn needs_reload_table() {
        let cases = [
            ReloadCase {
                desc: "the pane is not a preview at all: nothing to reload",
                current: None,
                loaded: None,
                want: false,
            },
            ReloadCase {
                desc: "FIRST BIND: a preview with nothing loaded yet must load",
                current: Some(SOURCE_A),
                loaded: None,
                want: true,
            },
            ReloadCase {
                desc: "identical pair: no redundant blocking render_preview",
                current: Some(SOURCE_A),
                loaded: Some(SOURCE_A),
                want: false,
            },
            ReloadCase {
                desc: "SAME TICK, DIFFERENT BUFFER: a reused preview pane switching sources",
                current: Some(SOURCE_B_SAME_TICK),
                loaded: Some(SOURCE_A),
                want: true,
            },
            ReloadCase {
                desc: "the preview was dropped: current is None, so nothing reloads",
                current: None,
                loaded: Some(SOURCE_A),
                want: false,
            },
            ReloadCase {
                desc: "SAME BUFFER, ADVANCED TICK: a live edit of the previewed file",
                current: Some(SOURCE_A_EDITED),
                loaded: Some(SOURCE_A),
                want: true,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_RELOAD_ROWS,
            "the reload table must carry its {NAMED_RELOAD_ROWS} named rows, not {}. Rows 4 and 6 \
             are the pair that measures the KEY: row 4 alone is satisfied by a tick-only gate's \
             opposite, and rows 1 to 5 without row 6 are all satisfied by a bufnr-only gate. \
             Neither is redundant.",
            cases.len()
        );

        for case in &cases {
            assert_eq!(
                needs_reload(case.current, case.loaded),
                case.want,
                "needs_reload(current = {:?}, loaded = {:?}) must be {} — {}. A missing reload \
                 leaves the PREVIOUS document on screen under the new file's statusbar, so the \
                 preview looks alive and is simply wrong; a spurious one spends a BLOCKING \
                 render_preview on every wake, and nothing on this path is debounced by \
                 `Snapshot::seq` (the wolf_* notifications fire the wake without bumping it).",
                case.current,
                case.loaded,
                case.want,
                case.desc
            );
        }

        // The two halves of the key, stated on their own so a failure names which one
        // stopped being read rather than quoting a row index.
        assert!(
            needs_reload(Some(SOURCE_B_SAME_TICK), Some(SOURCE_A)),
            "a preview pane rebound from buffer {} to buffer {} at the SAME changedtick ({}) must \
             reload. Two freshly opened files share a changedtick, so this is not a corner case: \
             it is what a reused preview pane does every time the user previews a second file. A \
             gate keyed on the tick alone answers false here and leaves the FIRST file's rendered \
             HTML on screen.",
            SOURCE_A.source_bufnr,
            SOURCE_B_SAME_TICK.source_bufnr,
            SOURCE_A.source_tick
        );
        assert!(
            needs_reload(Some(SOURCE_A_EDITED), Some(SOURCE_A)),
            "an edit to the previewed buffer ({} -> tick {}) must reload. A gate keyed on the \
             buffer number alone answers false here, and the live preview stops being live: it \
             would update only when the user previewed a different file.",
            SOURCE_A.source_bufnr,
            SOURCE_A_EDITED.source_tick
        );

        // Non-vacuity for the two fixtures the rows above lean on.
        assert_ne!(
            SOURCE_A.source_bufnr, SOURCE_B_SAME_TICK.source_bufnr,
            "precondition: the two fixtures must name DIFFERENT buffers, or row 4 is an identical \
             pair wearing a different name"
        );
        assert_eq!(
            SOURCE_A.source_tick, SOURCE_B_SAME_TICK.source_tick,
            "precondition: the two fixtures must share a changedtick, or row 4 is an ordinary \
             tick-advance row and the defect it exists for goes unmeasured"
        );
    }

    // =========================================================================
    // document_update
    // =========================================================================

    /// A source buffer nvim has NEVER reported a `changedtick` for — the `0` sentinel
    /// `refresh_pane_state` produces when `Snapshot::markdown_ticks` carries no entry
    /// for the bound source yet (the `wolf_markdown_dirty` notification upserts it, so
    /// a freshly bound preview has none until the first edit or write).
    ///
    /// It is a real reading rather than an invented one, and it is here because `0` is
    /// the value most likely to be special-cased by an "optimization" that treats an
    /// unknown tick as "nothing has changed".
    const SOURCE_C_NEVER_TICKED: PreviewLoad = PreviewLoad {
        source_bufnr: 9,
        source_tick: 0,
    };

    /// A [`DocumentUpdate`] as the name a failure quotes.
    ///
    /// A name rather than the value itself because [`DocumentUpdate`] deliberately
    /// derives no `Hash` — nothing in production keys a map on it — and the range
    /// assertion below needs a set. The `match` is `_`-FREE, so appending a variant is
    /// an `E0004` here rather than a strategy that silently stops being swept.
    fn update_name(update: DocumentUpdate) -> &'static str {
        match update {
            DocumentUpdate::FullLoad => "FullLoad",
            DocumentUpdate::InPlaceEligible => "InPlaceEligible",
        }
    }

    /// One [`document_update`] table row.
    struct UpdateCase {
        desc: &'static str,
        current: PreviewLoad,
        loaded: Option<PreviewLoad>,
        fragment_pending: bool,
        want: DocumentUpdate,
    }

    /// How many named rows the document-update table carries, as a literal.
    const NAMED_UPDATE_ROWS: usize = 8;

    /// Pins how a freshly rendered document reaches the live web view.
    ///
    /// Property 1: each named shape gets the strategy the rules give it. Property 2:
    /// the range over the sweep is EXACTLY both variants.
    ///
    /// Property 2 is BELT-AND-BRACES against `document_update` and a guard on this
    /// TABLE, exactly as [`crate::preview_link`]'s classification table records for its
    /// own range assertion. A `document_update` that answered
    /// [`DocumentUpdate::FullLoad`] for everything does not reach it: the range is
    /// accumulated inside the loop and row 3 fails first. What property 2 does catch,
    /// and catches alone, is a TRIMMED table — delete the three `InPlaceEligible` rows
    /// and lower [`NAMED_UPDATE_ROWS`] to match, and the length assertion and property 1
    /// both stay green while the strategy that preserves the reader's scroll offset
    /// stops being swept at all.
    ///
    /// **Rows 3 and 5 are the pair that measures the KEY.** Row 3 is a live edit of the
    /// file on screen and MUST stay in place; row 5 is a reused preview pane switching
    /// sources at the SAME `changedtick`, which two freshly opened files really do
    /// share, and MUST reload. A rule keyed on the tick answers BOTH wrong — row 5 by
    /// reading a source switch as an in-place edit, row 3 by reading an edit as a
    /// source switch — and the final arm turned into a full load answers row 3 wrong.
    /// Neither row alone can see both, which is why they are a pair and not a
    /// duplicate.
    ///
    /// **Row 4 is the fragment coupling**, and it is the one this signature cannot
    /// show: a cross-file scroll is applied on a `LoadEvent::Finished` edge, and an
    /// in-place body swap fires no load event at all — so with the clause gone the
    /// scroll simply never runs and the reader lands at the top of the linked file
    /// instead of at the anchor, with no error anywhere.
    ///
    /// PREMISE, stated rather than assumed: rules 2 and 3 both answer `FullLoad`, so
    /// their ORDER against each other is unobservable and this table deliberately makes
    /// no claim about it. Rule 1 must precede both structurally — there is nothing to
    /// compare against until `loaded` is `Some` — which Rust enforces rather than this
    /// test.
    ///
    /// Falsifiers, and which row each one actually reports — the loop aborts on its
    /// FIRST failure, so "reddens" without "first" is not a usable prediction:
    /// * Change the final arm to [`DocumentUpdate::FullLoad`] — rows 3, 6 and 7 all
    ///   fail and row 3 is reported. What that costs was MEASURED in unit 17:
    ///   `window.scrollY` 2000 was preserved in 24 of 24 samples across a live edit
    ///   with the in-place swap, against 0 of 24 for a full `load_html`.
    /// * DELETE rule 3 — this is the edit that isolates row 5, and MEASURED it is the
    ///   row reported. (Row 8 fails under it too and is reached later; nothing else
    ///   moves, because every other row either answers through rule 1 or 2 or wants
    ///   `InPlaceEligible` anyway.)
    /// * Compare `source_tick` instead of `source_bufnr` in rule 3 — rows 3 AND 5 both
    ///   fail, and MEASURED the report names row 3. It falsifies the PAIR without
    ///   isolating either half, so it is not evidence about row 5; the deletion above
    ///   is.
    /// * Delete the `fragment_pending` clause — row 4 reddens and nothing else does
    ///   (row 2 still answers `FullLoad` through rule 1, which is what isolates it).
    #[test]
    fn document_update_table() {
        let cases = [
            UpdateCase {
                desc: "FIRST LOAD: nothing has ever been loaded, so there is no committed \
                       <head> to swap a body under",
                current: SOURCE_A,
                loaded: None,
                fragment_pending: false,
                want: DocumentUpdate::FullLoad,
            },
            UpdateCase {
                desc: "first load WITH a fragment waiting: rule 1 already answers, so rule 2 \
                       cannot be what decides this row",
                current: SOURCE_A,
                loaded: None,
                fragment_pending: true,
                want: DocumentUpdate::FullLoad,
            },
            UpdateCase {
                desc: "SAME source, NEW tick: a live edit of the file on screen — the row that \
                       preserves the reader's scroll offset",
                current: SOURCE_A_EDITED,
                loaded: Some(SOURCE_A),
                fragment_pending: false,
                want: DocumentUpdate::InPlaceEligible,
            },
            UpdateCase {
                desc: "the same edit with a CROSS-FILE fragment waiting: the scroll needs a \
                       LoadEvent::Finished edge, which only a real load produces",
                current: SOURCE_A_EDITED,
                loaded: Some(SOURCE_A),
                fragment_pending: true,
                want: DocumentUpdate::FullLoad,
            },
            UpdateCase {
                desc: "DIFFERENT source, SAME tick: a reused preview pane switching files, \
                       which needs a new BASE URI",
                current: SOURCE_B_SAME_TICK,
                loaded: Some(SOURCE_A),
                fragment_pending: false,
                want: DocumentUpdate::FullLoad,
            },
            UpdateCase {
                desc: "an identical pair: the same document re-rendered, still eligible",
                current: SOURCE_A,
                loaded: Some(SOURCE_A),
                fragment_pending: false,
                want: DocumentUpdate::InPlaceEligible,
            },
            UpdateCase {
                desc: "the `0` tick SENTINEL on both sides: an unreported changedtick is not a \
                       source switch",
                current: SOURCE_C_NEVER_TICKED,
                loaded: Some(SOURCE_C_NEVER_TICKED),
                fragment_pending: false,
                want: DocumentUpdate::InPlaceEligible,
            },
            UpdateCase {
                desc: "the `0` tick sentinel against a DIFFERENT buffer: still a source switch, \
                       decided on the bufnr and not on the tick",
                current: SOURCE_C_NEVER_TICKED,
                loaded: Some(SOURCE_A),
                fragment_pending: false,
                want: DocumentUpdate::FullLoad,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_UPDATE_ROWS,
            "the document-update table must carry its {NAMED_UPDATE_ROWS} named rows, not {}. \
             Rows 3, 4 and 5 are the three that name one rule each, and no two of them can be \
             collapsed: 3 is the FIRST row the final arm decides (6 and 7 follow it), 4 the only \
             row the fragment clause decides, and 5 the FIRST row that separates the bufnr from \
             the tick (8 follows it). The loop reports its first failure, so those two \
             \"first\"s are what the falsifier list in the doc comment predicts.",
            cases.len()
        );

        let mut seen: HashSet<&'static str> = HashSet::new();
        for case in &cases {
            let got = document_update(case.current, case.loaded, case.fragment_pending);
            assert_eq!(
                got, case.want,
                "document_update(current = {:?}, loaded = {:?}, fragment_pending = {}) must be \
                 {:?}, got {got:?} — {}. A wrong FullLoad costs the reader their scroll position \
                 on every keystroke (MEASURED: 2000 preserved in 24 of 24 samples in place, 0 of \
                 24 through load_html). A wrong InPlaceEligible is quieter and worse: the view's \
                 BASE URI still belongs to the previous file, so every relative link in the new \
                 document resolves against the old document's directory and core is handed a path \
                 that does not exist — and because the rendered <head> is byte-identical across \
                 files by design, `pane_view`'s head-offset guard cannot see the switch on its \
                 own.",
                case.current, case.loaded, case.fragment_pending, case.want, case.desc
            );
            seen.insert(update_name(got));
        }

        // --- Property 2: the range, which no individual row can state --------
        //
        // Reached only when every row above passed, so it is a statement about this
        // TABLE's coverage rather than about `document_update` — see the doc comment.
        // The two names are typed out as LITERALS rather than derived from a sweep of
        // the enum, because comparing the observed range against a list computed the
        // same way would be a tautology.
        let both: HashSet<&'static str> = ["FullLoad", "InPlaceEligible"].into_iter().collect();
        assert_eq!(
            seen, both,
            "document_update's range over all {NAMED_UPDATE_ROWS} rows must be exactly both \
             variants. A MISSING variant means no row left in the table exercises that strategy: \
             every surviving row pins one and they all passed, so the only way to get here is by \
             DELETING the rows that carried it and lowering NAMED_UPDATE_ROWS to match. Restore \
             them rather than shrinking this set. Losing InPlaceEligible leaves nothing measuring \
             the swap that preserves the reader's scroll offset on a live edit; losing FullLoad \
             leaves nothing measuring the first load, the source switch that re-establishes the \
             base URI, or the load edge a pending cross-file fragment scrolls on."
        );

        // The two fixtures rows 3 and 5 lean on, checked rather than trusted — the
        // `needs_reload_table` precedent, restated here because a fixture edited for
        // THAT table would silently disarm this one.
        assert_ne!(
            SOURCE_A.source_bufnr, SOURCE_B_SAME_TICK.source_bufnr,
            "precondition: the two fixtures must name DIFFERENT buffers, or row 5 is an identical \
             pair wearing a different name and the bufnr rule goes unmeasured"
        );
        assert_eq!(
            SOURCE_A.source_tick, SOURCE_B_SAME_TICK.source_tick,
            "precondition: the two fixtures must share a changedtick, or row 5 is satisfied by a \
             tick comparison too and stops separating the two keys"
        );
        assert_eq!(
            SOURCE_A.source_bufnr, SOURCE_A_EDITED.source_bufnr,
            "precondition: row 3's two loads must name the SAME buffer, or it is a source switch \
             wearing an edit's name"
        );
        assert_ne!(
            SOURCE_A.source_tick, SOURCE_A_EDITED.source_tick,
            "precondition: row 3's two loads must differ in their tick, or it is an identical pair \
             and says nothing about a live edit"
        );
    }

    // =========================================================================
    // may_render
    // =========================================================================

    /// The coalesced `render_preview` runs for a live core and nothing else.
    ///
    /// Property: `may_render` is `true` for `Some(Liveness::Alive)` alone, over the
    /// whole `Option<Liveness>` domain. The `_`-free match in [`liveness_name`] is
    /// what makes a new `wolf_core::Liveness` variant an `E0004` here rather than a
    /// silently missing row.
    ///
    /// Falsifier: let `Some(Liveness::NotResponding)` through. `render_preview` is a
    /// blocking RPC issued from the render path, so every wake against a wedged
    /// Neovim then freezes the main loop for another full timeout budget — and the
    /// render path fires far more often than any chord does, which is what makes
    /// this the worst place in the crate to leak that gate.
    #[test]
    fn may_render_is_alive_only() {
        let accepting: Vec<Option<Liveness>> = ALL_LIVENESS_INPUTS
            .into_iter()
            .filter(|liveness| may_render(*liveness))
            .collect();
        let only_alive = [Some(Liveness::Alive)];

        assert_eq!(
            accepting.as_slice(),
            only_alive.as_slice(),
            "exactly ONE of the {NAMED_LIVENESS_INPUTS} liveness inputs may issue the blocking \
             render — a core reading Alive. An EXTRA entry means the render is issued against a \
             core already known to be wedged, dead or taken out, on EVERY wake rather than on a \
             chord. An entry MISSING means the preview never renders at all, since Alive is what a \
             healthy editor always reports."
        );

        for liveness in ALL_LIVENESS_INPUTS {
            let expected = liveness == Some(Liveness::Alive);
            assert_eq!(
                may_render(liveness),
                expected,
                "may_render for {} must be {expected}. Stated per input as well as over the whole \
                 domain so a failure names the reading rather than quoting a slice.",
                liveness_name(liveness)
            );
        }

        assert_eq!(
            ALL_LIVENESS_INPUTS.len(),
            NAMED_LIVENESS_INPUTS,
            "ALL_LIVENESS_INPUTS carries {} entries but the gate is total over \
             {NAMED_LIVENESS_INPUTS}; a shrunk array makes the sweep above cover less than the \
             domain it claims to.",
            ALL_LIVENESS_INPUTS.len()
        );
    }

    // =========================================================================
    // preview_status
    // =========================================================================

    /// A source living under the window's working directory. Its basename shares no
    /// text with any directory segment above it, so a `filename` built from the
    /// wrong component is visible in the failure rather than passing by accident.
    const SOURCE_UNDER_CWD: &str = "/proj/notes/design.md";

    /// [`SOURCE_UNDER_CWD`] as `core.display_path` renders it.
    const DISPLAY_UNDER_CWD: &str = "notes/design.md";

    /// A source OUTSIDE the working directory, which `core.display_path` leaves
    /// absolute.
    const SOURCE_OUTSIDE_CWD: &str = "/opt/shared/handbook.md";

    /// A path with no file-name component at all.
    const SOURCE_NO_FILE_NAME: &str = "/";

    /// A [`PaneStatus`] written out field by field.
    fn status(filename: &str, display_path: &str, abs_path: &str) -> PaneStatus {
        PaneStatus {
            filename: filename.to_string(),
            display_path: display_path.to_string(),
            abs_path: abs_path.to_string(),
        }
    }

    /// One [`preview_status`] table row.
    struct StatusCase {
        desc: &'static str,
        source_path: &'static str,
        display_path: &'static str,
        expected: PaneStatus,
    }

    /// Pins a preview pane's two statusbar lines.
    ///
    /// Property: `filename` is [`PREVIEW_STATUS_PREFIX`] followed by the source's
    /// BASENAME (the whole string when it has none), `display_path` is passed
    /// through verbatim, and `abs_path` is the source path verbatim — except for an
    /// empty source, which is a bare `"Preview"` with both paths empty.
    ///
    /// PREMISE, measured below rather than assumed: `std::path::Path::file_name`
    /// answers `None` for `"/"`. The fallback branch exists for exactly that answer,
    /// so a test that assumed it without measuring would be pinning a branch it had
    /// not shown was reachable.
    ///
    /// The expected filenames are written as LITERALS while
    /// [`PREVIEW_STATUS_PREFIX`] is pinned separately, so editing the constant alone
    /// reddens this test instead of moving the expectation with it. The prefix is a
    /// contract with `runtime/nvim/init.lua`, which names the preview scratch buffer
    /// the same way, and with the cage smoke that reads it back through OCR.
    ///
    /// Falsifier: use the whole `source_path` where the basename is called for. Row
    /// 1's filename becomes `"Preview: /proj/notes/design.md"`, which is what the
    /// second line already says — so the statusbar shows the path twice and the
    /// filename line, clipped to its own pane, shows the least useful end of it.
    #[test]
    fn preview_status_table() {
        assert_eq!(
            PREVIEW_STATUS_PREFIX, "Preview: ",
            "the statusbar prefix is a contract: `runtime/nvim/init.lua` names the preview scratch \
             buffer \"Preview: <cwd-relative path>\", so the buffer picker and this statusbar must \
             read the same way, and the cage smoke recognises the pane by OCR-ing this exact text. \
             The trailing SPACE is part of it — without it the filename reads \"Preview:design.md\"."
        );

        // The premise the no-file-name row rests on, measured against the real std.
        assert!(
            Path::new(SOURCE_NO_FILE_NAME).file_name().is_none(),
            "precondition: `Path::new({SOURCE_NO_FILE_NAME:?}).file_name()` must be None, or the \
             fallback row below never reaches the branch it exists to pin and would pass whatever \
             that branch did."
        );
        assert_eq!(
            Path::new(SOURCE_UNDER_CWD)
                .file_name()
                .and_then(|n| n.to_str()),
            Some("design.md"),
            "precondition: the basename of {SOURCE_UNDER_CWD:?} must be \"design.md\", or the \
             expectations below are describing a different string than the seam can produce."
        );

        let cases = [
            StatusCase {
                desc: "a source UNDER the cwd: the basename above, the relative path beneath",
                source_path: SOURCE_UNDER_CWD,
                display_path: DISPLAY_UNDER_CWD,
                expected: status("Preview: design.md", DISPLAY_UNDER_CWD, SOURCE_UNDER_CWD),
            },
            StatusCase {
                desc: "a source OUTSIDE the cwd: the display path stays absolute, verbatim",
                source_path: SOURCE_OUTSIDE_CWD,
                display_path: SOURCE_OUTSIDE_CWD,
                expected: status(
                    "Preview: handbook.md",
                    SOURCE_OUTSIDE_CWD,
                    SOURCE_OUTSIDE_CWD,
                ),
            },
            StatusCase {
                desc: "an EMPTY display path: the second line is blank, the first still names the \
                       file",
                source_path: SOURCE_UNDER_CWD,
                display_path: "",
                expected: status("Preview: design.md", "", SOURCE_UNDER_CWD),
            },
            StatusCase {
                // The display path handed in is deliberately NON-empty, so the rule
                // being measured is "an empty source forces both paths empty" rather
                // than "an empty input produced an empty output".
                desc: "an EMPTY source: a bare \"Preview\", with BOTH paths forced empty",
                source_path: "",
                display_path: DISPLAY_UNDER_CWD,
                expected: status("Preview", "", ""),
            },
            StatusCase {
                desc: "a source with NO file-name component: the whole string is the name",
                source_path: SOURCE_NO_FILE_NAME,
                display_path: SOURCE_NO_FILE_NAME,
                expected: status("Preview: /", SOURCE_NO_FILE_NAME, SOURCE_NO_FILE_NAME),
            },
        ];

        for case in &cases {
            let got = preview_status(case.source_path, case.display_path);
            assert_eq!(
                got, case.expected,
                "preview_status({:?}, {:?}) must be {:?}, got {got:?} — {}. `WolfCore::pane_status` \
                 answers \"[No Name]\" for a preview window (preview windows are omitted from \
                 win_buffers entirely), so these three strings are the ONLY thing a preview pane's \
                 statusbar has to draw with.",
                case.source_path, case.display_path, case.expected, case.desc
            );
        }

        // --- the discriminating negatives ------------------------------------
        //
        // Equality above would also be satisfied by several plausible near-misses if
        // the expectations were ever loosened, so the two that matter are stated
        // outright against the value the seam really returns.
        let under_cwd = preview_status(SOURCE_UNDER_CWD, DISPLAY_UNDER_CWD);
        assert_ne!(
            under_cwd.filename, "design.md",
            "the filename must carry the {PREVIEW_STATUS_PREFIX:?} prefix. Without it a preview \
             pane's statusbar is indistinguishable from the SOURCE pane's beside it, which is the \
             one thing a two-pane preview layout needs its statusbars to tell apart."
        );
        assert_ne!(
            under_cwd.filename,
            format!("{PREVIEW_STATUS_PREFIX}{SOURCE_UNDER_CWD}"),
            "the filename must carry the BASENAME, not the whole path. The second line already \
             shows the path, and this line is clipped to its own pane rather than ellipsised — so \
             a full path here shows the least identifying end of it and repeats what is directly \
             beneath."
        );

        let empty = preview_status("", DISPLAY_UNDER_CWD);
        assert!(
            !empty.filename.starts_with(PREVIEW_STATUS_PREFIX),
            "an empty source must give a BARE \"Preview\" ({:?} was returned). The prefix ends in \
             a colon and a space, which announces a name that is about to follow; with nothing \
             after it the statusbar reads as a truncated string rather than as a pane with no \
             source yet.",
            empty.filename
        );
        assert!(
            !empty.filename.ends_with(' '),
            "an empty source's filename ({:?}) ends in a space, so it is the prefix with the name \
             removed rather than the bare word the rule asks for.",
            empty.filename
        );
    }

    /// `abs_path` is the source path VERBATIM, never the display path.
    ///
    /// Property: whatever relativization `core.display_path` performed for the
    /// second line, `abs_path` carries the raw path through untouched.
    ///
    /// [`preview_status_table`] overlaps this: two of its five rows hand in a source
    /// and a display path that differ, so it would also redden. This states the rule
    /// on its own, with a source whose two forms are chosen to DISAGREE, so the
    /// failure names the PERSISTENCE consequence instead of pointing at a table row
    /// — and it adds the two assertions a whole-value equality cannot express, that
    /// the field is not the display path and that it is absolute.
    ///
    /// [`crate::pane_view`] persists this field as a pane-layout leaf path, so a
    /// relativized value is a path relative to a working directory `state.json` has
    /// no record of: the next launch reopens it against whatever cwd it happens to
    /// have, or fails to reopen it at all.
    ///
    /// Falsifier: build the returned `PaneStatus` with `abs_path: display_path`.
    #[test]
    fn preview_status_carries_the_raw_path_for_persistence() {
        assert_ne!(
            SOURCE_UNDER_CWD, DISPLAY_UNDER_CWD,
            "precondition: the raw path and the display path must DIFFER, or this test cannot tell \
             which of the two `abs_path` was built from"
        );

        let got = preview_status(SOURCE_UNDER_CWD, DISPLAY_UNDER_CWD);

        assert_eq!(
            got.abs_path, SOURCE_UNDER_CWD,
            "abs_path must be the source path verbatim. `pane_view::layout_snapshot` reads exactly \
             this field into the persisted pane-layout leaf, and `state.json` records no working \
             directory to resolve a relative one against — so a display path here is silently \
             unopenable on the next launch."
        );
        assert_ne!(
            got.abs_path, got.display_path,
            "abs_path came back equal to display_path ({:?}) for a source whose two forms differ. \
             The display path is the RELATIVIZED one; persisting it loses the only absolute \
             reference the pane had.",
            got.display_path
        );
        assert!(
            got.abs_path.starts_with('/'),
            "abs_path ({:?}) is not absolute. Whatever else changed, the field's whole purpose is \
             to be the one string a later launch can reopen without knowing where the window was \
             rooted.",
            got.abs_path
        );
        assert_eq!(
            got.display_path, DISPLAY_UNDER_CWD,
            "display_path must be passed through verbatim. This seam performs no path arithmetic \
             of its own — `core.display_path` already decided between cwd-relative, ~-prefixed and \
             absolute — so rewriting it here would give the preview pane a different second line \
             from every editor pane beside it."
        );
    }
}
