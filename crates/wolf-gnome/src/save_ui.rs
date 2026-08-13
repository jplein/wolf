//! The pure, GTK-free rulebook for SAVE and SAVE AS in `wolf-gnome`: whether a
//! press may act at all, whether it writes or opens a dialog, what that dialog
//! proposes, and what a chosen path becomes.
//!
//! The saving counterpart of [`crate::nav_ui`] and [`crate::clipboard_ui`]. Data
//! in, data out — no widget type, no `WolfCore`, no filesystem — which is what lets
//! the whole rulebook be unit-tested headlessly, the same way
//! [`crate::command_action`], [`crate::geometry`] and [`crate::cursor`] keep their
//! decisions pure. The widget work of carrying the answers out lives in
//! [`crate::shell`].
//!
//! ## A named buffer is WRITTEN, never prompted for
//!
//! [`save_press`] answers [`SavePress::Write`] for any buffer that has a file name,
//! **including one that has never been written to disk**, and that is deliberate.
//! `runtime/nvim/init.lua`'s `wolf_name_untitled` gives a fresh anonymous buffer a
//! concrete `<cwd>/Untitled N.md` name — collision-checked against files on disk,
//! against every other buffer and against stale swap files — without writing it. So
//! "never saved" here means "named but not on disk", not "unnamed", and that name is
//! already on screen in the tab title, the pane statusbar and the window's file row.
//! Saving to the name the user is looking at is the least surprising outcome
//! available.
//!
//! **Do not "fix" this into a redirect.** Making Save conditional on an existence
//! probe would take three things away. Ctrl+S and a typed `:w` would stop agreeing,
//! though they sit in one window over one buffer. The probe's answer drifts — a file
//! deleted out from under a buffer that has been saved all session would suddenly
//! start popping a modal on every press. And Save is the safety-critical chord: when
//! a dialog fails to present, a conditional Save leaves the buffer unwritten while
//! the user believes it saved, where an unwanted `Untitled.md` is trivially undone.
//!
//! The genuinely NAMELESS buffer — a typed `:enew`, whose `Snapshot::current_file`
//! is `None` — is the one case that takes the dialog. `write` there raises
//! `E32: No file name`, which the input pump degrades to a warning nobody sees, so
//! there is no working behaviour to diverge from.
//!
//! **`None` is the only spelling of nameless that production produces.**
//! `wolf_core::redraw`'s `apply_current_file` maps an empty `expand('%:p')` to `None`
//! before it ever lands in `Snapshot::current_file`, so `Some("")` never reaches this
//! seam from a snapshot — a focused preview pane included, though `runtime/nvim`'s
//! current-file autocmd does emit the empty string for one. The empty-path filter in
//! `save_name` is therefore DEFENSIVE rather than a shape the snapshot presents:
//! [`SaveTarget`] and [`save_dialog_proposal`] are public, so a caller can hand an
//! empty path in by other means.
//!
//! ## Neither press blocks, and the gate is not the blocking gate
//!
//! **The blocking-call inventory in `crates/wolf-gnome/DECISIONS.md` is unchanged by
//! this module.** `InputEvent::Save` and `InputEvent::SaveAs` are fire-and-forget on
//! `wolf-core`'s ordered input channel, and the native dialog resolves on a later
//! main-loop turn, so nothing here can time out and nothing here can mark the core
//! `NotResponding`.
//!
//! Both presses are gated on `Some(Liveness::Alive)` anyway, for
//! [`crate::clipboard_ui::paste_decision`]'s reason rather than
//! [`crate::nav_ui::nav_press`]'s: a save queued behind an opaque crash overlay
//! would reach a RESTARTED Neovim minutes later and be applied to whatever buffer
//! that fresh instance happens to be on.
//!
//! [`save_as_press`] adds a second, sharper reason that must not be collapsed into
//! the first — the [`crate::clipboard_ui`] convention for Copy and Paste. A native
//! MODAL dialog opening over the opaque stall overlay invites the user to name a
//! file that will never be written, and it covers the overlay's Restart button,
//! which is the only route out of a stall.
//!
//! ## Where the target comes from
//!
//! The path is `Snapshot::current_file` and never `WolfCore::pane_status`. Both are
//! RPC-free, but `pane_status` joins through the current-tabpage-only
//! `Snapshot::windows` map and answers a DISPLAY string, with a placeholder label for
//! an unnamed buffer — the trap [`crate::tabs_ui`]'s module docs record. Save needs
//! "is there a path" as a value, not a label.
//!
//! ## Totality is the contract
//!
//! [`save_press`] and [`save_as_press`] each match `Option<Liveness>` with **no `_`
//! arm**, so appending a variant to [`wolf_core::Liveness`] is an `E0004` compile
//! error here until it is classified — the guard [`crate::nav_ui::nav_press`] and
//! [`crate::buffer_picker::open_decision`] rely on. `Option<Liveness>` (not
//! `Liveness`) is the input type precisely so the ABSENT core, which the window's
//! close handler leaves behind, is a classified arm rather than an early return no
//! headless test could see.
//!
//! Liveness is checked FIRST, before the buffer's shape, so a wedged Neovim is
//! reported as wedged rather than as a preview pane — [`crate::preview_ui`]'s
//! recorded ordering rule.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use wolf_core::Liveness;

/// The file name [`save_dialog_proposal`] offers for a buffer that has none.
///
/// It must match what `runtime/nvim/init.lua`'s `wolf_next_untitled` produces at
/// `i == 1`, because that is the name the rest of the application makes for a new
/// buffer. A different spelling here would propose a name the app never produces,
/// and a different extension would leave the saved buffer outside the Markdown
/// filetype every preview, statusbar and treesitter path keys on.
pub const UNTITLED_SAVE_NAME: &str = "Untitled.md";

/// What the focused buffer looks like to a save press, read from ONE snapshot pass.
///
/// The three fields must describe the SAME instant: a path taken from one snapshot
/// and a preview flag from another can name a shape that never existed, and this
/// seam would then answer `Write` for a pane that cannot be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveTarget<'a> {
    /// `Snapshot::current_file` — the focused buffer's path, `None` for a buffer with
    /// no file name, which is the ONLY spelling of nameless a snapshot carries (see
    /// the module docs). An empty path counts as no name too, DEFENSIVELY, because
    /// this struct is public and a caller can build one that a snapshot never would.
    pub path: Option<&'a Path>,
    /// `Snapshot::is_preview_active()` — whether the focused pane is a Markdown
    /// preview scratch, which has nothing to write.
    pub is_preview: bool,
    /// `Snapshot::win_buffers[active_win].is_terminal` — whether the focused pane is
    /// a terminal buffer, whose `term://…` name Neovim will not `write`. `false`
    /// when the window is absent from the map.
    pub is_terminal: bool,
}

/// What pressing Save does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePress {
    /// Send `InputEvent::Save`; the buffer has a name to write to.
    Write,
    /// Open the native save dialog; the buffer has no file name at all.
    Choose,
    /// Do nothing; the string names why, as a mid-sentence clause (the
    /// [`crate::command_action`] / [`crate::nav_ui`] convention).
    Refuse(&'static str),
}

/// What pressing Save As does.
///
/// A SECOND enum rather than a reused [`SavePress`], so [`crate::shell`]'s Save As
/// match has no dead `Write` branch: Save As always goes through the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAsPress {
    /// Open the native save dialog.
    Choose,
    /// Do nothing; the string names why, as a mid-sentence clause.
    Refuse(&'static str),
}

/// What the native save dialog is opened with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveProposal {
    /// The directory the dialog opens in, or `None` to leave GTK its own default.
    pub folder: Option<PathBuf>,
    /// The name pre-filled in the dialog's name entry.
    pub name: String,
}

/// What a path chosen in the save dialog becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveAsRequest {
    /// The chosen path, VERBATIM.
    pub path: PathBuf,
    /// Always `true` — see [`save_as_target`].
    pub force: bool,
}

/// Refusal reason for a wedged core on the Save path.
const SAVE_NOT_RESPONDING: &str =
    "the captive Neovim is already known not to be responding, so a write queued behind the stall \
     would be applied by a restarted instance to whatever buffer it happens to be on";

/// Refusal reason for a wedged core on the Save As path.
///
/// Deliberately NOT [`SAVE_NOT_RESPONDING`]: this path would put a native MODAL over
/// the stall overlay, and the two hazards must stay distinguishable in the printed
/// line.
const SAVE_AS_NOT_RESPONDING: &str =
    "the captive Neovim is already known not to be responding, and a native modal save dialog \
     would cover the stall overlay's Restart button — the only route out of a stall — while \
     inviting the user to name a file that would never be written";

/// Refusal reason for a dead core, shared by both presses: the cause is the same.
const CORE_DEAD: &str =
    "the captive Neovim has died, so there is no buffer left to write and the restart it is \
     waiting on will not carry a pending save across";

/// Refusal reason for an absent core, shared by both presses.
const NO_CORE: &str =
    "the core has already been taken out by the window's close handler, so there is nothing left \
     to save";

/// Refusal reason for a preview scratch, shared by both presses.
const PREVIEW_PANE: &str =
    "the focused pane is a Markdown preview scratch, which is rendered output rather than a file \
     and has nothing to write";

/// Refusal reason for a terminal buffer, shared by both presses.
const TERMINAL_PANE: &str =
    "the focused pane is a terminal buffer, whose term:// name Neovim refuses to write";

/// The buffer's usable file NAME, or `None` when it has none.
///
/// The empty-path filter is DEFENSIVE. A snapshot never carries one:
/// `wolf_core::redraw`'s `apply_current_file` maps an empty `expand('%:p')` to `None`,
/// so production spells nameless `None` and never `Some("")`. But [`SaveTarget`] and
/// [`save_dialog_proposal`] are public, so an empty path can arrive from a caller, and
/// this states where it lands rather than leaving it to `Path::file_name`'s incidental
/// `None`. Keep both halves: `file_name` is what covers `/` and `..`, which are
/// non-empty and still nameless.
fn save_name(path: Option<&Path>) -> Option<&OsStr> {
    path.filter(|p| !p.as_os_str().is_empty())
        .and_then(Path::file_name)
}

/// Whether a Save press writes, opens the dialog, or does nothing.
///
/// TOTAL by construction: a `match` over `Option<Liveness>` with **no `_` arm** (see
/// the module docs). Liveness is decided FIRST; only inside the live arm does the
/// buffer's shape matter, in the order preview, terminal, name.
#[must_use]
pub fn save_press(liveness: Option<Liveness>, target: SaveTarget<'_>) -> SavePress {
    match liveness {
        Some(Liveness::Alive) => {
            if target.is_preview {
                return SavePress::Refuse(PREVIEW_PANE);
            }
            if target.is_terminal {
                return SavePress::Refuse(TERMINAL_PANE);
            }
            match save_name(target.path) {
                Some(_) => SavePress::Write,
                None => SavePress::Choose,
            }
        }
        Some(Liveness::NotResponding) => SavePress::Refuse(SAVE_NOT_RESPONDING),
        Some(Liveness::Dead) => SavePress::Refuse(CORE_DEAD),
        None => SavePress::Refuse(NO_CORE),
    }
}

/// Whether a Save As press opens the dialog, or does nothing.
///
/// [`save_press`]'s rules exactly, except that the accepted arm is always
/// [`SaveAsPress::Choose`] — a named buffer is what Save As exists to RENAME, so a
/// name is no reason to skip the dialog.
#[must_use]
pub fn save_as_press(liveness: Option<Liveness>, target: SaveTarget<'_>) -> SaveAsPress {
    match liveness {
        Some(Liveness::Alive) => {
            if target.is_preview {
                return SaveAsPress::Refuse(PREVIEW_PANE);
            }
            if target.is_terminal {
                return SaveAsPress::Refuse(TERMINAL_PANE);
            }
            SaveAsPress::Choose
        }
        Some(Liveness::NotResponding) => SaveAsPress::Refuse(SAVE_AS_NOT_RESPONDING),
        Some(Liveness::Dead) => SaveAsPress::Refuse(CORE_DEAD),
        None => SaveAsPress::Refuse(NO_CORE),
    }
}

/// What the native save dialog opens with, for a buffer at `path` in a window
/// rooted at `cwd`.
///
/// The name is the path's own file name, or [`UNTITLED_SAVE_NAME`] when it has none.
///
/// The folder is the BUFFER'S OWN parent when the buffer has a name, and the
/// window's `cwd` otherwise; `None` when neither is available, which leaves GTK its
/// own default rather than synthesizing one.
///
/// **Why the buffer's own parent rather than the window's cwd:** Save As on
/// `src/deep/thing.md` must open where the user is working, or "save a copy beside
/// it" becomes a navigation task. For an `Untitled.md` in the project root the two
/// rules coincide, so nothing is lost.
///
/// **Why the `cwd` passed in is NOT [`crate::tabs_ui::new_tab_cwd`]'s answer**, and
/// the divergence is deliberate: that one refuses the empty state on purpose, so a
/// tab is never `:tcd`'d into `$HOME`. A save dialog's folder is a starting point the
/// user can change, not a declaration that a directory is a project, so the empty
/// state's home directory is a correct default here.
#[must_use]
pub fn save_dialog_proposal(path: Option<&Path>, cwd: Option<&Path>) -> SaveProposal {
    match save_name(path) {
        Some(name) => SaveProposal {
            folder: path
                .and_then(Path::parent)
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .or_else(|| cwd.map(Path::to_path_buf)),
            name: name.to_string_lossy().into_owned(),
        },
        None => SaveProposal {
            folder: cwd.map(Path::to_path_buf),
            name: UNTITLED_SAVE_NAME.to_string(),
        },
    }
}

/// What a path chosen in the save dialog becomes, or `None` when it may not be used.
///
/// `None` for an empty path, a path with no file name, or a RELATIVE one —
/// [`crate::shell::AppShell`]'s `chdir_to` records the reason: core would resolve a
/// relative path against the PROCESS working directory rather than Neovim's, so the
/// file would land somewhere the user did not choose. `gio::File::path()` is absolute
/// in practice; the rule is stated so a later change of source cannot smuggle one in.
///
/// **The path is passed through VERBATIM — no extension is ever appended.** Proposing
/// `.md` is the dialog's job. Rewriting the user's typed name would mean GTK's
/// overwrite confirmation was asked about a DIFFERENT file than the one written,
/// which turns a confirmed save into an unconfirmed clobber.
///
/// `force` is always `true`, and the reason belongs here rather than at the call
/// site: the dialog has already confirmed the overwrite, and a plain `saveas` would
/// then fail `E13: File exists` — an error `wolf_core::input` degrades to a
/// `tracing::warn!` the user never sees, losing the write the user just confirmed.
#[must_use]
pub fn save_as_target(chosen: &Path) -> Option<SaveAsRequest> {
    if !chosen.is_absolute() {
        return None;
    }
    save_name(Some(chosen))?;
    Some(SaveAsRequest {
        path: chosen.to_path_buf(),
        force: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    // =========================================================================
    // The whole input domain
    // =========================================================================
    //
    // Several rules below are stated over EVERY input rather than over one table
    // row, so that a failure NAMES the rule instead of quoting an index — the
    // `nav_ui` / `tabs_ui` / `buffer_picker` convention.

    /// Every input [`save_press`] and [`save_as_press`] are total over on their
    /// LIVENESS axis: the three readings a core can produce, plus the ABSENT core
    /// (`None`) the window's close handler leaves behind.
    const ALL_LIVENESS_INPUTS: [Option<Liveness>; 4] = [
        Some(Liveness::Alive),
        Some(Liveness::NotResponding),
        Some(Liveness::Dead),
        None,
    ];

    /// The window's working directory in every proposal row. Deliberately a PREFIX
    /// of [`NESTED_FILE`], because that is the arrangement in which "the buffer's own
    /// parent" and "the window's cwd" are DIFFERENT answers — a cwd unrelated to the
    /// file would make the nested row pass for a seam that answered either one.
    const CWD: &str = "/home/u/proj";

    /// A buffer named but NEVER WRITTEN — the `wolf_name_untitled` shape, and the
    /// case the module docs forbid turning into a prompt.
    ///
    /// Its parent is [`CWD`] itself, so the "buffer's own parent" and "the window's
    /// cwd" rules COINCIDE here on purpose: that is the reason §2.2's divergence
    /// costs nothing for a fresh Untitled buffer, and it is why [`NESTED_FILE`]
    /// rather than this one is what discriminates the folder rule.
    const UNTITLED_FILE: &str = "/home/u/proj/Untitled.md";

    /// A file several directories BELOW the window's cwd. None of its directory
    /// components is its basename, so a proposal that answered with a parent
    /// directory — or with the whole path — is visible rather than accidentally
    /// equal.
    const NESTED_FILE: &str = "/home/u/proj/src/deep/thing.md";

    /// [`NESTED_FILE`]'s own parent: the folder a Save As over it must open in.
    const NESTED_PARENT: &str = "/home/u/proj/src/deep";

    /// [`NESTED_FILE`]'s basename.
    const NESTED_NAME: &str = "thing.md";

    /// A terminal buffer's name, as Neovim spells it. It HAS a usable file name
    /// (`Path::file_name` answers `12345:bash`), which is exactly why the terminal
    /// arm has to be a check of its own: without it this row would answer `Write`.
    const TERMINAL_FILE: &str = "term://home/u/proj//12345:bash";

    /// One named row of the `SaveTarget` axis.
    ///
    /// A plain struct rather than a `const SaveTarget` array because
    /// [`std::path::Path::new`] is not a `const fn`, so a `SaveTarget` cannot be
    /// spelled in a `const` at all. [`NamedTarget::target`] builds the real value.
    #[derive(Clone, Copy)]
    struct NamedTarget {
        /// Short identifier, used in the whole-domain sweeps' expected sets so a
        /// failure names the shape rather than quoting a path.
        name: &'static str,
        desc: &'static str,
        path: Option<&'static str>,
        is_preview: bool,
        is_terminal: bool,
    }

    impl NamedTarget {
        fn target(self) -> SaveTarget<'static> {
            SaveTarget {
                path: self.path.map(Path::new),
                is_preview: self.is_preview,
                is_terminal: self.is_terminal,
            }
        }
    }

    /// A buffer named but NEVER WRITTEN — `wolf_name_untitled`'s own shape, and the
    /// case the module docs forbid turning into a prompt.
    const NAMED: NamedTarget = NamedTarget {
        name: "NAMED",
        desc: "a buffer named but NEVER WRITTEN — wolf_name_untitled's own shape",
        path: Some(UNTITLED_FILE),
        is_preview: false,
        is_terminal: false,
    };

    /// An ordinary file several directories BELOW the window's cwd. The row that
    /// discriminates "the buffer's own folder" from "the window's cwd".
    const NAMED_NESTED: NamedTarget = NamedTarget {
        name: "NAMED_NESTED",
        desc: "an ordinary file several directories below the window's cwd",
        path: Some(NESTED_FILE),
        is_preview: false,
        is_terminal: false,
    };

    /// A genuinely nameless buffer — a TYPED `:enew`, which nothing names. (The
    /// front-end's own New Buffer goes through `wolf_new_untitled` and DOES name one,
    /// which is why this shape needs a deliberate route to reach.)
    const UNNAMED: NamedTarget = NamedTarget {
        name: "UNNAMED",
        desc: "a genuinely nameless buffer (a typed :enew), whose current_file is None",
        path: None,
        is_preview: false,
        is_terminal: false,
    };

    /// The DEFENSIVE row: an empty path, which no snapshot produces —
    /// `redraw::apply_current_file` maps an empty `expand('%:p')` to `None` — but
    /// which a hand-built [`SaveTarget`] can carry, the struct being public. It pins
    /// that an empty path is treated as NO NAME rather than as one.
    const EMPTY_PATH: NamedTarget = NamedTarget {
        name: "EMPTY_PATH",
        desc: "an EMPTY path — the defensive row: production spells nameless as None, and only a \
               hand-built SaveTarget can carry this",
        path: Some(""),
        is_preview: false,
        is_terminal: false,
    };

    /// A focused Markdown preview pane in the shape production really presents:
    /// `is_preview` set and NO path at all. `runtime/nvim/init.lua`'s current-file
    /// autocmd emits the empty string for a preview scratch, and
    /// `redraw::apply_current_file` maps that to `None` on the way in.
    const PREVIEW: NamedTarget = NamedTarget {
        name: "PREVIEW",
        desc: "a focused Markdown preview pane, in the shape production presents it — no path, \
               is_preview set",
        path: None,
        is_preview: true,
        is_terminal: false,
    };

    /// A preview scratch that DOES carry a path, so its refusal cannot be coming from
    /// the name check. This is the row the ORDERING rule is stated on.
    const PREVIEW_WITH_PATH: NamedTarget = NamedTarget {
        name: "PREVIEW_WITH_PATH",
        desc: "a preview scratch that DOES carry a path, so the refusal cannot be coming from the \
               name check",
        path: Some(UNTITLED_FILE),
        is_preview: true,
        is_terminal: false,
    };

    /// A terminal buffer, whose `term://` name IS a usable file name — without the
    /// terminal arm this row answers `Write`.
    const TERMINAL_WITH_PATH: NamedTarget = NamedTarget {
        name: "TERMINAL_WITH_PATH",
        desc: "a terminal buffer, whose term:// name IS a usable file name",
        path: Some(TERMINAL_FILE),
        is_preview: false,
        is_terminal: true,
    };

    /// Every shape the focused buffer can present to a save press.
    ///
    /// Seven, and none is a variation on another: two NAMED buffers (one of which is
    /// a never-written `Untitled.md`), two NAMELESS ones (`None`, which is the only
    /// spelling `Snapshot::current_file` produces, and the empty path, which is
    /// defensive), and three that must be REFUSED, of which two carry a usable file
    /// name so the refusal cannot be mistaken for the nameless answer.
    const ALL_TARGETS: [NamedTarget; 7] = [
        NAMED,
        NAMED_NESTED,
        UNNAMED,
        EMPTY_PATH,
        PREVIEW,
        PREVIEW_WITH_PATH,
        TERMINAL_WITH_PATH,
    ];

    /// How many `(Option<Liveness>, SaveTarget)` inputs each press seam is total
    /// over: four liveness readings times seven buffer shapes.
    ///
    /// Written as LITERALS rather than as the product of the two domain consts'
    /// lengths, and that is the whole point of it — the `nav_ui::tests::SWEEP_INPUTS`
    /// and `tabs_ui::tests::NAMED_WAKE_CASES` precedent. Comparing a table's length
    /// to the product of the arrays it was built from is a TAUTOLOGY: an array that
    /// lost an entry shrinks both sides together, so the check passes while the table
    /// covers less than the domain it claims to.
    const NAMED_PRESS_CASES: usize = 4 * 7;

    /// A hashable key for one `Option<Liveness>`, matched TOTALLY with no `_` arm so
    /// a new [`Liveness`] variant is an `E0004` here rather than a silently uncovered
    /// input. `Option<Liveness>` is not `Hash`, and the duplicate-row checks below
    /// need one. Copied from `tabs_ui.rs`'s helper of the same name.
    fn liveness_key(liveness: Option<Liveness>) -> u8 {
        match liveness {
            Some(Liveness::Alive) => 0,
            Some(Liveness::NotResponding) => 1,
            Some(Liveness::Dead) => 2,
            None => 3,
        }
    }

    /// Whether `reason` opens like a SENTENCE (`Xy…`) rather than as a mid-sentence
    /// clause. Copied from `nav_ui.rs`'s helper of the same name, including its
    /// deliberate allowance for an ALL-CAPS opener such as `GNOME`.
    fn is_sentence_cased(reason: &str) -> bool {
        let mut chars = reason.chars();
        let (Some(first), Some(second)) = (chars.next(), chars.next()) else {
            return false;
        };
        first.is_uppercase() && second.is_lowercase()
    }

    // =========================================================================
    // save_press
    // =========================================================================

    /// What a press-table row expects a press seam to return.
    ///
    /// A refusal is matched on its CLASS, not on its reason string: the reasons are
    /// documentation, and their real contract (non-empty, distinct, clause-shaped) is
    /// asserted by [`save_press_reasons_are_distinct_and_non_empty`] and its twin —
    /// the `nav_ui::tests::WantPress` precedent. The one place a row DOES pin a
    /// particular string is [`liveness_is_decided_before_the_buffers_shape`], because
    /// there the string is the only thing that says which arm won.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Want {
        /// `SavePress::Write` — `InputEvent::Save` is sent and no dialog opens.
        Write,
        /// `SavePress::Choose` / `SaveAsPress::Choose` — the native dialog opens.
        Choose,
        /// `Refuse(_)` — nothing at all happens, and a reason is printed.
        Refuse,
    }

    /// One press-table row.
    ///
    /// The expectation is DATA — a literal `want` per row, deliberately NOT re-derived
    /// from the same conjunction the function under test uses, which is
    /// `tabs_ui::tests::RestorePressCase`'s recorded rule. A table computed from the
    /// seam's own rule cannot disagree with it.
    struct PressCase {
        desc: &'static str,
        liveness: Option<Liveness>,
        target: NamedTarget,
        want: Want,
    }

    /// The twenty-eight rows of the SAVE gate, one per `(liveness, target)` input.
    ///
    /// A `const` item rather than a local so the coverage sweep and the assertions
    /// walk the same rows — the `tabs_ui::tests::WAKE_CASES` convention.
    const SAVE_PRESS_CASES: &[PressCase] = &[
        // --- A LIVE core: the only arm that may write or prompt ---------------
        PressCase {
            desc: "THE ROW FACT 3 IS ABOUT — a named but never-written buffer is WRITTEN, not \
                   prompted for: the name is already in the tab title, the pane statusbar and the \
                   window's file row",
            liveness: Some(Liveness::Alive),
            target: NAMED,
            want: Want::Write,
        },
        PressCase {
            desc: "an ordinary nested file writes, and Save opens no dialog it has nothing to add \
                   to",
            liveness: Some(Liveness::Alive),
            target: NAMED_NESTED,
            want: Want::Write,
        },
        PressCase {
            desc: "a genuinely nameless buffer is the ONE case that takes the dialog: `write` \
                   there raises E32, which the input pump degrades to a warning nobody sees",
            liveness: Some(Liveness::Alive),
            target: UNNAMED,
            want: Want::Choose,
        },
        PressCase {
            desc: "the EMPTY path is no name either, so it prompts too — defensive, since a \
                   snapshot spells nameless as None",
            liveness: Some(Liveness::Alive),
            target: EMPTY_PATH,
            want: Want::Choose,
        },
        PressCase {
            desc: "a preview scratch is rendered output, not a file",
            liveness: Some(Liveness::Alive),
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc:
                "a preview scratch WITH a path is still refused, so the preview arm is doing the \
                   work rather than the name check",
            liveness: Some(Liveness::Alive),
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc:
                "a terminal buffer is refused even though its term:// name IS a usable file name \
                   — without that arm this row answers Write",
            liveness: Some(Liveness::Alive),
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
        // --- A WEDGED core: the queued write would land in a RESTARTED instance
        PressCase {
            desc: "a wedged core refuses a named buffer",
            liveness: Some(Liveness::NotResponding),
            target: NAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core refuses a nested file",
            liveness: Some(Liveness::NotResponding),
            target: NAMED_NESTED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core refuses a nameless buffer rather than raising a modal",
            liveness: Some(Liveness::NotResponding),
            target: UNNAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core refuses the empty path",
            liveness: Some(Liveness::NotResponding),
            target: EMPTY_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core over a preview pane",
            liveness: Some(Liveness::NotResponding),
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core over a preview pane that carries a path — the row the ORDERING \
                   rule is stated on, in liveness_is_decided_before_the_buffers_shape",
            liveness: Some(Liveness::NotResponding),
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core over a terminal",
            liveness: Some(Liveness::NotResponding),
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
        // --- A DEAD core: there is no buffer left to write --------------------
        PressCase {
            desc: "a dead core refuses a named buffer",
            liveness: Some(Liveness::Dead),
            target: NAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core refuses a nested file",
            liveness: Some(Liveness::Dead),
            target: NAMED_NESTED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core refuses a nameless buffer",
            liveness: Some(Liveness::Dead),
            target: UNNAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core refuses the empty path",
            liveness: Some(Liveness::Dead),
            target: EMPTY_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core over a preview pane",
            liveness: Some(Liveness::Dead),
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core over a preview pane that carries a path",
            liveness: Some(Liveness::Dead),
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core over a terminal",
            liveness: Some(Liveness::Dead),
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
        // --- NO core at all: the post-shutdown shape --------------------------
        PressCase {
            desc: "no core (the close handler took it out) refuses a named buffer",
            liveness: None,
            target: NAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core refuses a nested file",
            liveness: None,
            target: NAMED_NESTED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core refuses a nameless buffer",
            liveness: None,
            target: UNNAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core refuses the empty path",
            liveness: None,
            target: EMPTY_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core over a preview pane",
            liveness: None,
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core over a preview pane that carries a path",
            liveness: None,
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core over a terminal",
            liveness: None,
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
    ];

    /// The twenty-eight rows of the SAVE AS gate.
    ///
    /// Identical to [`SAVE_PRESS_CASES`] except in the four rows a live core accepts:
    /// Save As always goes through the dialog, because a named buffer is exactly what
    /// it exists to RENAME. Written out rather than derived from the other table, so
    /// the one difference between the two seams is visible as data.
    const SAVE_AS_PRESS_CASES: &[PressCase] = &[
        // --- A LIVE core: every writable buffer takes the dialog --------------
        PressCase {
            desc:
                "THE DIFFERENCE FROM SAVE — a named buffer opens the dialog rather than writing, \
                   because renaming it is the whole point",
            liveness: Some(Liveness::Alive),
            target: NAMED,
            want: Want::Choose,
        },
        PressCase {
            desc: "a nested file opens the dialog too",
            liveness: Some(Liveness::Alive),
            target: NAMED_NESTED,
            want: Want::Choose,
        },
        PressCase {
            desc: "a nameless buffer opens the dialog, the same as under Save",
            liveness: Some(Liveness::Alive),
            target: UNNAMED,
            want: Want::Choose,
        },
        PressCase {
            desc: "the empty path opens the dialog",
            liveness: Some(Liveness::Alive),
            target: EMPTY_PATH,
            want: Want::Choose,
        },
        PressCase {
            desc: "a preview scratch is refused: there is nothing to rename",
            liveness: Some(Liveness::Alive),
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a preview scratch WITH a path is refused too",
            liveness: Some(Liveness::Alive),
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a terminal buffer is refused",
            liveness: Some(Liveness::Alive),
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
        // --- A WEDGED core: THE MODAL-OVER-THE-OVERLAY ROWS -------------------
        PressCase {
            desc: "THE ROW THE SECOND REASON EXISTS FOR — a wedged core must not raise a native \
                   modal over the stall overlay, whose Restart button is the only route out",
            liveness: Some(Liveness::NotResponding),
            target: NAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core refuses a nested file",
            liveness: Some(Liveness::NotResponding),
            target: NAMED_NESTED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core refuses a nameless buffer",
            liveness: Some(Liveness::NotResponding),
            target: UNNAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core refuses the empty path",
            liveness: Some(Liveness::NotResponding),
            target: EMPTY_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core over a preview pane",
            liveness: Some(Liveness::NotResponding),
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core over a preview pane that carries a path — the ORDERING row",
            liveness: Some(Liveness::NotResponding),
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a wedged core over a terminal",
            liveness: Some(Liveness::NotResponding),
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
        // --- A DEAD core ------------------------------------------------------
        PressCase {
            desc: "a dead core refuses a named buffer",
            liveness: Some(Liveness::Dead),
            target: NAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core refuses a nested file",
            liveness: Some(Liveness::Dead),
            target: NAMED_NESTED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core refuses a nameless buffer",
            liveness: Some(Liveness::Dead),
            target: UNNAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core refuses the empty path",
            liveness: Some(Liveness::Dead),
            target: EMPTY_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core over a preview pane",
            liveness: Some(Liveness::Dead),
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core over a preview pane that carries a path",
            liveness: Some(Liveness::Dead),
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "a dead core over a terminal",
            liveness: Some(Liveness::Dead),
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
        // --- NO core at all ---------------------------------------------------
        PressCase {
            desc: "no core (the close handler took it out) refuses a named buffer",
            liveness: None,
            target: NAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core refuses a nested file",
            liveness: None,
            target: NAMED_NESTED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core refuses a nameless buffer",
            liveness: None,
            target: UNNAMED,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core refuses the empty path",
            liveness: None,
            target: EMPTY_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core over a preview pane",
            liveness: None,
            target: PREVIEW,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core over a preview pane that carries a path",
            liveness: None,
            target: PREVIEW_WITH_PATH,
            want: Want::Refuse,
        },
        PressCase {
            desc: "no core over a terminal",
            liveness: None,
            target: TERMINAL_WITH_PATH,
            want: Want::Refuse,
        },
    ];

    /// Asserts a press table is neither short nor made of duplicates, and that every
    /// `(liveness, target)` input the seam is total over really has a row.
    fn assert_table_covers_the_domain(seam: &str, cases: &[PressCase]) {
        assert_eq!(
            cases.len(),
            NAMED_PRESS_CASES,
            "the {seam} table must carry exactly one row per input the seam is total over: four \
             liveness readings (three a core can produce, plus the ABSENT core) times seven buffer \
             shapes is {NAMED_PRESS_CASES} rows, not {}. A missing row means an input nothing \
             checks.",
            cases.len()
        );

        let mut seen: HashSet<(u8, &'static str)> = HashSet::new();
        for case in cases {
            assert!(
                seen.insert((liveness_key(case.liveness), case.target.name)),
                "two {seam} rows carry the same (liveness = {:?}, target = {}) input, so the table \
                 is shorter than its length suggests and some other input is unmeasured",
                case.liveness,
                case.target.name
            );
        }
        for liveness in ALL_LIVENESS_INPUTS {
            for target in ALL_TARGETS {
                assert!(
                    seen.contains(&(liveness_key(liveness), target.name)),
                    "no {seam} row covers (liveness = {liveness:?}, target = {}), so that input's \
                     answer is whatever the seam happens to do — {}",
                    target.name,
                    target.desc
                );
            }
        }
    }

    /// Pins the Save decision for EVERY `(Option<Liveness>, SaveTarget)` input.
    ///
    /// ## What it discriminates, and the edits that redden it
    ///
    /// FALSIFYING EDIT 1 (behaviour 1) — change the `save_name(..) == None` arm from
    /// `Choose` to `Write`. That is the arm that turns Save on a nameless buffer into
    /// an `E32: No file name` the input pump degrades to a `tracing::warn!` nobody
    /// sees. MEASURED: red, and ISOLATED — this test alone, 366 passed / 1 failed —
    /// `save_press(Some(Alive), UNNAMED) must be Choose, got Write`, `left: Write`,
    /// `right: Choose`.
    ///
    /// FALSIFYING EDIT 2 (behaviour 2) — add an existence probe and redirect a
    /// named-but-absent buffer to `Choose`, i.e. the design §2.2 rejects.
    /// `wolf_name_untitled` has already picked a collision-checked name that is on
    /// screen in the tab title, the pane statusbar and the window's file row, so Save
    /// writes it; Ctrl+S and a typed `:w` must not disagree. MEASURED with
    /// `Some(_) if target.path.is_some_and(Path::exists) => Write` and a `Choose`
    /// fall-through: red, and isolated to this test — `save_press(Some(Alive), NAMED)
    /// must be Write, got Choose`, `left: Choose`, `right: Write`. **Not a ROW-level
    /// isolation**, and the difference matters to anyone reading this table: the row
    /// loop aborts at its first failure, and `NAMED_NESTED` takes the same wrong
    /// answer, since neither fixture path exists on any host. This table's paths are
    /// synthetic, so no row of it can tell "absent from disk" from "present".
    ///
    /// FALSIFYING EDIT 3 (behaviour 5) — delete the `is_terminal` arm. The
    /// `TERMINAL_WITH_PATH` row under `Alive` answers `Write`, because a `term://`
    /// name IS a usable file name; Neovim then refuses the write and the failure is
    /// again a warning nobody sees. MEASURED: red, and **not isolated** — FIVE tests
    /// fail together (362 passed / 5 failed), and the set is the diagnosis. This one;
    /// [`only_a_live_core_may_save`], whose accepting set grows by exactly
    /// `(0, "TERMINAL_WITH_PATH")`; [`save_press_reasons_are_distinct_and_non_empty`]
    /// and [`the_two_presses_share_four_causes_and_split_the_wedged_one`], which lose
    /// a probe; and [`liveness_is_decided_before_the_buffers_shape`], on its premise
    /// panic `save_press(Alive, TERMINAL_WITH_PATH) must refuse, or the pane arms are
    /// not reached at all`.
    #[test]
    fn save_press_table() {
        let cases = SAVE_PRESS_CASES;
        assert_table_covers_the_domain("save_press", cases);

        for case in cases {
            let got = save_press(case.liveness, case.target.target());
            let got_class = match got {
                SavePress::Write => Want::Write,
                SavePress::Choose => Want::Choose,
                SavePress::Refuse(_) => Want::Refuse,
            };
            assert_eq!(
                got_class, case.want,
                "save_press({:?}, {}) must be {:?}, got {got:?} — {}. Each wrong answer costs \
                 something different. A CHOOSE where Write belongs pops a native modal over a \
                 buffer that already has a name the user is looking at, so Ctrl+S and a typed :w \
                 stop agreeing about what one press does. A WRITE where Choose belongs sends \
                 `write` at a buffer with no file name, which raises E32 and is degraded to a \
                 `tracing::warn!` the user never sees — the press looks like it worked. A WRITE or \
                 a CHOOSE where a REFUSAL belongs acts on a pane that cannot be written at all (a \
                 preview scratch is rendered output; a term:// name Neovim will not write) or \
                 against a core that is wedged, dead or already taken out, where the queued save \
                 would be applied by a RESTARTED instance to whatever buffer it happens to be on. \
                 A REFUSAL where Write belongs makes Ctrl+Shift+S do nothing on a perfectly \
                 healthy editor.",
                case.liveness, case.target.name, case.want, case.desc
            );
        }

        // --- Non-vacuity: all three answers are produced ----------------------
        //
        // BACKSTOPS rather than the red a single-arm edit produces — the row loop
        // above dominates every one of those. What they catch is the table and the
        // seam moving TOGETHER: a rewrite that made `save_press` a constant function
        // and rewrote every `want` to agree would satisfy the length check, the
        // coverage sweep and every row.
        let answers: Vec<Want> = cases.iter().map(|case| case.want).collect();
        for (want, consequence) in [
            (
                Want::Write,
                "no input would ever write, so Save could not save anything at all",
            ),
            (
                Want::Choose,
                "a nameless buffer would have no route to a dialog, so Save on a typed :enew would \
                 be a silent E32 forever",
            ),
            (
                Want::Refuse,
                "nothing would be refused, so a wedged core, a dead one, a preview scratch and a \
                 terminal would all be acted on",
            ),
        ] {
            assert!(
                answers.contains(&want),
                "NOT ONE of the {NAMED_PRESS_CASES} save_press rows expects {want:?}, so that arm \
                 is not stated at all: {consequence}."
            );
        }
    }

    /// Pins the Save As decision for EVERY `(Option<Liveness>, SaveTarget)` input.
    ///
    /// The one difference from [`save_press_table`] is the whole reason there are two
    /// enums: a NAMED buffer is what Save As exists to RENAME, so it opens the dialog
    /// rather than writing.
    ///
    /// FALSIFYING EDIT (behaviour 4) — make the `Some(Liveness::Dead)` or
    /// `Some(Liveness::NotResponding)` arm fall through to the live behaviour. Seven
    /// rows fail apiece, and the sharp one is the modal-over-the-stall-overlay case:
    /// a native modal opening over the opaque overlay covers the Restart button,
    /// which `liveness_ui` records as the only route out of a stall, while inviting
    /// the user to name a file that will never be written. MEASURED on the `Dead`
    /// arm: red, and **not isolated** — five tests fail together (362 passed / 5
    /// failed). This one, on `save_as_press(Some(Dead), NAMED) must be Refuse, got
    /// Choose`; [`only_a_live_core_may_open_the_save_dialog`], whose accepting set
    /// grows by the whole `(2, …)` row of the domain;
    /// [`save_as_press_reasons_are_distinct_and_non_empty`];
    /// [`the_two_presses_share_four_causes_and_split_the_wedged_one`]; and
    /// [`liveness_is_decided_before_the_buffers_shape`] on its baseline panic
    /// `save_as_press(Some(Dead), NAMED) accepted, so this test has no baseline`.
    #[test]
    fn save_as_press_table() {
        let cases = SAVE_AS_PRESS_CASES;
        assert_table_covers_the_domain("save_as_press", cases);

        for case in cases {
            let got = save_as_press(case.liveness, case.target.target());
            let got_class = match got {
                SaveAsPress::Choose => Want::Choose,
                SaveAsPress::Refuse(_) => Want::Refuse,
            };
            assert_eq!(
                got_class, case.want,
                "save_as_press({:?}, {}) must be {:?}, got {got:?} — {}. A CHOOSE where a refusal \
                 belongs opens a NATIVE MODAL over a pane that cannot be written, or over the \
                 opaque crash/stall overlay — where it covers the Restart button that is the only \
                 route out of a stall, and invites the user to name a file that will never be \
                 written. A REFUSAL where Choose belongs makes the Save As menu item dead on a \
                 healthy editor, and it is the row's ONLY route: the item carries no chord.",
                case.liveness, case.target.name, case.want, case.desc
            );
        }

        // Non-vacuity, the same backstop shape as `save_press_table`'s. A `Write`
        // answer is not among them because `SaveAsPress` has no such variant, which
        // is the point of the second enum: `shell`'s Save As match has no dead branch.
        let answers: Vec<Want> = cases.iter().map(|case| case.want).collect();
        assert!(
            answers.contains(&Want::Choose),
            "NOT ONE of the {NAMED_PRESS_CASES} save_as_press rows expects Choose, so Save As \
             could never open its dialog and the menu item would be permanently dead."
        );
        assert!(
            answers.contains(&Want::Refuse),
            "NOT ONE of the {NAMED_PRESS_CASES} save_as_press rows expects a refusal, so nothing \
             would be refused: a wedged core, a dead one, a preview scratch and a terminal would \
             all raise the modal."
        );
    }

    /// Only a LIVE core may act, stated over the WHOLE domain rather than over one
    /// table row, so a failure names the rule instead of quoting an index — the
    /// `nav_ui::only_a_live_core_with_a_real_window_may_move` convention.
    ///
    /// FALSIFYING EDIT (behaviour 3) — make the `Some(Liveness::Dead)` arm fall
    /// through to the live behaviour. The accepting set gains four entries here, and
    /// `save_press_table` reddens on the same rows; the PAIR moving together is what
    /// says the classification changed rather than this list. MEASURED: red, and
    /// **not isolated** by design — five tests fail together (362 passed / 5 failed),
    /// and here `left` reads `[(0, "NAMED"), (0, "NAMED_NESTED"), (0, "UNNAMED"),
    /// (0, "EMPTY_PATH"), (2, "NAMED"), (2, "NAMED_NESTED"), (2, "UNNAMED"),
    /// (2, "EMPTY_PATH")]` against a `right` of the four `(0, …)` entries alone. The
    /// four companions are `save_press_table` (`save_press(Some(Dead), NAMED) must be
    /// Refuse, got Write`), [`save_press_reasons_are_distinct_and_non_empty`],
    /// [`the_two_presses_share_four_causes_and_split_the_wedged_one`], and
    /// [`liveness_is_decided_before_the_buffers_shape`].
    ///
    /// The non-vacuity assertion is what stops a `save_press` of `|_, _| Refuse(..)`
    /// satisfying it while Save never saves anything again.
    #[test]
    fn only_a_live_core_may_save() {
        let mut accepting: Vec<(u8, &'static str)> = Vec::new();
        for liveness in ALL_LIVENESS_INPUTS {
            for target in ALL_TARGETS {
                if matches!(save_press(liveness, target.target()), SavePress::Refuse(_)) {
                    continue;
                }
                accepting.push((liveness_key(liveness), target.name));
            }
        }

        let expected = [
            (liveness_key(Some(Liveness::Alive)), "NAMED"),
            (liveness_key(Some(Liveness::Alive)), "NAMED_NESTED"),
            (liveness_key(Some(Liveness::Alive)), "UNNAMED"),
            (liveness_key(Some(Liveness::Alive)), "EMPTY_PATH"),
        ];
        assert_eq!(
            accepting.as_slice(),
            expected.as_slice(),
            "exactly FOUR of the {NAMED_PRESS_CASES} (liveness, target) inputs may act on a Save \
             press — a core reading Alive, focused on an ordinary editor buffer, whether or not it \
             has a name. An EXTRA entry means Save acts against a core already known to be wedged \
             or dead (so the write is applied by the RESTARTED instance to whatever buffer it \
             happens to be on), against a core the close handler has already taken out, or over a \
             preview scratch or a terminal that cannot be written. An entry MISSING means \
             Ctrl+Shift+S does nothing on a healthy editor, since Alive with an ordinary buffer is \
             what one always reports. (The keys are `liveness_key`'s: 0 Alive, 1 NotResponding, 2 \
             Dead, 3 absent.)"
        );
        assert!(
            !accepting.is_empty(),
            "NOT ONE of the {NAMED_PRESS_CASES} inputs was accepted, so the rule above held \
             vacuously. A save_press that refused everything would satisfy it while Ctrl+Shift+S \
             and the Save menu item did nothing at all, forever."
        );
    }

    /// The same rule for Save As, stated over the whole domain.
    ///
    /// FALSIFYING EDIT (behaviour 4) — make `save_as_press` accept on `Dead`. The
    /// accepting set gains four entries, and `save_as_press_table` reddens with it.
    /// MEASURED: red, `left` reading `[(0, "NAMED"), (0, "NAMED_NESTED"),
    /// (0, "UNNAMED"), (0, "EMPTY_PATH"), (2, "NAMED"), (2, "NAMED_NESTED"),
    /// (2, "UNNAMED"), (2, "EMPTY_PATH")]`. Not isolated, and the companion set is
    /// [`save_as_press_table`]'s — see it for the whole list.
    #[test]
    fn only_a_live_core_may_open_the_save_dialog() {
        let mut accepting: Vec<(u8, &'static str)> = Vec::new();
        for liveness in ALL_LIVENESS_INPUTS {
            for target in ALL_TARGETS {
                if matches!(
                    save_as_press(liveness, target.target()),
                    SaveAsPress::Refuse(_)
                ) {
                    continue;
                }
                accepting.push((liveness_key(liveness), target.name));
            }
        }

        let expected = [
            (liveness_key(Some(Liveness::Alive)), "NAMED"),
            (liveness_key(Some(Liveness::Alive)), "NAMED_NESTED"),
            (liveness_key(Some(Liveness::Alive)), "UNNAMED"),
            (liveness_key(Some(Liveness::Alive)), "EMPTY_PATH"),
        ];
        assert_eq!(
            accepting.as_slice(),
            expected.as_slice(),
            "exactly FOUR of the {NAMED_PRESS_CASES} (liveness, target) inputs may open the save \
             dialog. An EXTRA entry raises a NATIVE MODAL over the opaque crash or stall overlay, \
             covering the Restart button that is the only route out of a stall, or over a pane \
             that cannot be written at all. An entry MISSING makes the Save As menu item — which \
             has no accelerator, so the menu is its only route — dead on a healthy editor. (The \
             keys are `liveness_key`'s: 0 Alive, 1 NotResponding, 2 Dead, 3 absent.)"
        );
        assert!(
            !accepting.is_empty(),
            "NOT ONE of the {NAMED_PRESS_CASES} inputs was accepted, so the rule above held \
             vacuously and Save As could never open its dialog."
        );
    }

    /// LIVENESS IS DECIDED FIRST: a wedged core over a preview pane is reported as
    /// WEDGED, not as a preview — `preview_ui`'s recorded ordering rule.
    ///
    /// This is the one place a row pins a particular reason STRING, and it has to:
    /// both orderings answer `Refuse`, so the class says nothing and only the string
    /// distinguishes which arm won.
    ///
    /// FALSIFYING EDIT (behaviour 5) — move the `is_preview` (or `is_terminal`) check
    /// ABOVE the liveness match. This test fails on that row alone; every table row
    /// stays green, because they compare classes. The cost of the wrong order is that
    /// the printed line blames the pane for a hazard that belongs to the core, and a
    /// user reading it goes looking at the wrong thing while a wedged Neovim goes
    /// unnamed. MEASURED with the `is_preview` check hoisted: red, and ISOLATED —
    /// this test alone, 366 passed / 1 failed —
    /// `save_press(Some(NotResponding), PREVIEW_WITH_PATH) blamed a preview pane
    /// rather than the CORE`, with `left` the preview string and `right` the wedged
    /// one. That the whole rest of the module stays green is the point: no class
    /// comparison can see an ordering change.
    #[test]
    fn liveness_is_decided_before_the_buffers_shape() {
        // Both probes carry a PATH on purpose. A pane-shaped refusal over a target
        // with no name at all would be indistinguishable from the nameless answer,
        // so the comparison below would not say which check ran first.
        assert!(
            PREVIEW_WITH_PATH.path.is_some_and(|p| !p.is_empty())
                && TERMINAL_WITH_PATH.path.is_some_and(|p| !p.is_empty()),
            "precondition: both ordering probes must carry a non-empty path, or a refusal over \
             them says nothing about which check fired first"
        );

        for (label, probe) in [
            ("a preview pane", PREVIEW_WITH_PATH),
            ("a terminal pane", TERMINAL_WITH_PATH),
        ] {
            for wedged in [Some(Liveness::NotResponding), Some(Liveness::Dead), None] {
                let SavePress::Refuse(over_probe) = save_press(wedged, probe.target()) else {
                    panic!(
                        "save_press({wedged:?}, {}) accepted, so there is no reason to compare and \
                         `save_press_table` is red too.",
                        probe.name
                    )
                };
                let SavePress::Refuse(over_plain) = save_press(wedged, NAMED.target()) else {
                    panic!(
                        "save_press({wedged:?}, NAMED) accepted, so this test has no baseline; \
                         `save_press_table` and `only_a_live_core_may_save` are red too."
                    )
                };
                assert_eq!(
                    over_probe, over_plain,
                    "save_press({wedged:?}, {}) blamed {label} rather than the CORE. Liveness must \
                     be decided FIRST: over an ordinary buffer the very same core answers \
                     {over_plain:?}, and that is the reason a user has to read — a wedged Neovim \
                     is recoverable, a dead one restarts itself, and an absent core means the \
                     window is already tearing down. Blaming the pane sends the reader to look at \
                     a buffer while the real blocker is unnamed. Both orderings REFUSE, so no \
                     table row can see this: the string is the only witness.",
                    probe.name
                );

                let SaveAsPress::Refuse(as_over_probe) = save_as_press(wedged, probe.target())
                else {
                    panic!(
                        "save_as_press({wedged:?}, {}) accepted; `save_as_press_table` is red too.",
                        probe.name
                    )
                };
                let SaveAsPress::Refuse(as_over_plain) = save_as_press(wedged, NAMED.target())
                else {
                    panic!(
                        "save_as_press({wedged:?}, NAMED) accepted, so this test has no baseline."
                    )
                };
                assert_eq!(
                    as_over_probe, as_over_plain,
                    "save_as_press({wedged:?}, {}) blamed {label} rather than the core; see the \
                     save_press message above. Save As carries the sharper of the two wedged \
                     reasons — the modal covering the stall overlay's Restart button — so losing \
                     it to a pane-shaped reason costs the more actionable line of the two.",
                    probe.name
                );
            }
        }

        // The premise the comparison rests on: over a LIVE core the two probes really
        // do produce a pane-shaped refusal, distinct from the wedged one. Without
        // this, a seam whose arms all shared one string would satisfy every assertion
        // above while proving nothing about ordering.
        for probe in [PREVIEW_WITH_PATH, TERMINAL_WITH_PATH] {
            let SavePress::Refuse(live_reason) = save_press(Some(Liveness::Alive), probe.target())
            else {
                panic!(
                    "premise: save_press(Alive, {}) must refuse, or the pane arms are not reached \
                     at all and this test compares a wedged reason against itself.",
                    probe.name
                )
            };
            let SavePress::Refuse(wedged_reason) =
                save_press(Some(Liveness::NotResponding), NAMED.target())
            else {
                panic!("premise: save_press(NotResponding, NAMED) must refuse")
            };
            assert_ne!(
                live_reason, wedged_reason,
                "premise: the pane refusal for {} and the wedged-core refusal are the SAME string, \
                 so the assertions above hold whatever order the checks are in and this test \
                 proves nothing. `save_press_reasons_are_distinct_and_non_empty` is red too, and \
                 it is the one to read.",
                probe.name
            );
        }
    }

    /// The FIVE refusal causes on the Save path are non-empty, mutually DISTINCT, and
    /// read as a mid-sentence clause.
    ///
    /// Distinctness is the load-bearing half, for the reason
    /// `nav_ui::nav_press_reasons_are_distinct_and_non_empty` gives: the reason is the
    /// whole of what a reader sees when the chord appears to do nothing, and two arms
    /// sharing a string make "the editor is wedged" indistinguishable from "the editor
    /// has crashed" — two states with completely different routes back, one of which
    /// resolves by itself.
    ///
    /// FALSIFYING EDIT (behaviour 6) — give the `Dead` and `NotResponding` arms the
    /// same string. MEASURED: red, and **not isolated** — two tests fail (365 passed
    /// / 2 failed). Here: `the refusal reason for a dead core ("the captive Neovim is
    /// already known not to be responding, …") duplicates another arm's reason`.
    /// With it [`the_two_presses_share_four_causes_and_split_the_wedged_one`], on `a
    /// dead core produces a DIFFERENT reason on the Save and Save As paths` — the
    /// Save path having taken the wedged string while Save As kept [`CORE_DEAD`]. The
    /// pair is the diagnosis: this test says two causes collapsed, and the other says
    /// which side moved.
    #[test]
    fn save_press_reasons_are_distinct_and_non_empty() {
        let probes = [
            (
                "a wedged (NotResponding) core",
                Some(Liveness::NotResponding),
                NAMED,
            ),
            ("a dead core", Some(Liveness::Dead), NAMED),
            (
                "an absent core, after the close handler took it out",
                None,
                NAMED,
            ),
            (
                "a live core over a preview scratch",
                Some(Liveness::Alive),
                PREVIEW_WITH_PATH,
            ),
            (
                "a live core over a terminal buffer",
                Some(Liveness::Alive),
                TERMINAL_WITH_PATH,
            ),
        ];

        let seen = collect_distinct_reasons("save not performed", &probes, |liveness, target| {
            match save_press(liveness, target) {
                SavePress::Refuse(reason) => reason,
                other => panic!(
                    "a probe produced {other:?} rather than a refusal, so this test cannot check \
                     the reason it exists to check. Either that arm now acts (in which case \
                     `save_press_table` and `only_a_live_core_may_save` are also red, and one of \
                     them is the finding) or the probe no longer selects the arm it names."
                ),
            }
        });

        assert_eq!(
            seen.len(),
            probes.len(),
            "the five probed Save refusals produced {} distinct reason(s), not {}. There are FIVE \
             causes on this path — a wedged core, a dead one, no core at all, a preview scratch \
             and a terminal — and each has a different route back. See the per-probe message above \
             for which pair collapsed.",
            seen.len(),
            probes.len()
        );
    }

    /// The FIVE refusal causes on the Save As path, the same way.
    ///
    /// It is five here too, and NOT six: the sixth distinct string in this module is
    /// the SECOND spelling of the wedged cause, which lives on this function rather
    /// than beside another cause. [`the_two_presses_share_four_causes_and_split_the_wedged_one`]
    /// is where that is asserted.
    #[test]
    fn save_as_press_reasons_are_distinct_and_non_empty() {
        let probes = [
            (
                "a wedged (NotResponding) core",
                Some(Liveness::NotResponding),
                NAMED,
            ),
            ("a dead core", Some(Liveness::Dead), NAMED),
            (
                "an absent core, after the close handler took it out",
                None,
                NAMED,
            ),
            (
                "a live core over a preview scratch",
                Some(Liveness::Alive),
                PREVIEW_WITH_PATH,
            ),
            (
                "a live core over a terminal buffer",
                Some(Liveness::Alive),
                TERMINAL_WITH_PATH,
            ),
        ];

        let seen =
            collect_distinct_reasons("save as not performed", &probes, |liveness, target| {
                match save_as_press(liveness, target) {
                    SaveAsPress::Refuse(reason) => reason,
                    other => panic!(
                        "a probe produced {other:?} rather than a refusal, so this test cannot \
                         check the reason it exists to check. `save_as_press_table` and \
                         `only_a_live_core_may_open_the_save_dialog` are the tests to read."
                    ),
                }
            });

        assert_eq!(
            seen.len(),
            probes.len(),
            "the five probed Save As refusals produced {} distinct reason(s), not {}. See the \
             per-probe message above for which pair collapsed.",
            seen.len(),
            probes.len()
        );
    }

    /// Runs the shared reason contract over one seam's probes and returns the distinct
    /// strings it saw.
    ///
    /// `printed_as` is the diagnostic the shell interpolates the reason into, so the
    /// emptiness message names the line a user would actually be reading.
    fn collect_distinct_reasons(
        printed_as: &str,
        probes: &[(&'static str, Option<Liveness>, NamedTarget)],
        refuse: impl Fn(Option<Liveness>, SaveTarget<'static>) -> &'static str,
    ) -> HashSet<&'static str> {
        let mut seen: HashSet<&'static str> = HashSet::new();
        for &(source, liveness, target) in probes {
            let reason = refuse(liveness, target.target());

            assert!(
                !reason.trim().is_empty(),
                "the refusal reason for {source} is empty. It is printed as \
                 \"wolf-gnome: {printed_as}: {{reason}}\", so an empty one makes a deliberate \
                 refusal indistinguishable from a press that silently did nothing."
            );
            assert!(
                !is_sentence_cased(reason),
                "the refusal reason for {source} is sentence-cased ({reason:?}). It is \
                 interpolated MID-SENTENCE into the printed diagnostic, exactly as \
                 `command_action.rs`'s, `nav_ui.rs`'s and `clipboard_ui.rs`'s reasons are, so it \
                 must start lowercase (an ALL-CAPS proper noun such as \"GNOME\" is fine)."
            );
            assert!(
                seen.insert(reason),
                "the refusal reason for {source} ({reason:?}) duplicates another arm's reason. The \
                 reasons must be DISTINCT so the printed line names the real blocker: a wedged \
                 Neovim is recoverable and a dead one restarts itself, an absent core means the \
                 window is already tearing down, a preview scratch is rendered output with nothing \
                 to write, and a terminal's term:// name Neovim refuses. One shared string \
                 collapses them into a message that cannot be acted on."
            );
        }
        seen
    }

    /// SIX distinct strings across the pair, not ten and not five: four causes are
    /// SHARED because the cause is the same, and the wedged one SPLITS because the
    /// hazard is not.
    ///
    /// Save As's wedged reason has to say what Save's cannot — that a native modal
    /// would cover the stall overlay's Restart button, which `liveness_ui` records as
    /// the only route out of `NotResponding` (a `Dead` core restarts itself; a stalled
    /// one does not). The `clipboard_ui` convention for Copy and Paste, applied here.
    ///
    /// FALSIFYING EDIT 1 — give `save_as_press`'s `NotResponding` arm
    /// `SAVE_NOT_RESPONDING`. The union drops to five and the split assertion names it.
    /// MEASURED: red, and ISOLATED — this test alone, 366 passed / 1 failed — on
    /// `assertion left != right failed: the wedged-core reason is the SAME string on
    /// both paths`, with both sides printing [`SAVE_NOT_RESPONDING`]. Nothing else can
    /// see it: every other test compares refusal CLASSES or probes one seam at a time.
    ///
    /// FALSIFYING EDIT 2 — give either path a second copy of a shared reason rather
    /// than the shared const. The union grows to seven and the sharing assertion names
    /// which cause diverged. MEASURED with `save_as_press`'s preview arm given a
    /// hand-written string: red, and ISOLATED — this test alone, 366 passed / 1
    /// failed — `a preview scratch produces a DIFFERENT reason on the Save and Save As
    /// paths, but a preview pane has nothing to write and nothing to rename`, `left`
    /// [`PREVIEW_PANE`] and `right` the hand-written twin. Isolation is the finding
    /// here: the two `reasons_are_distinct` tests probe one seam each, so a divergence
    /// BETWEEN the seams is invisible to both.
    #[test]
    fn the_two_presses_share_four_causes_and_split_the_wedged_one() {
        let shared = [
            (
                "a dead core",
                Some(Liveness::Dead),
                NAMED,
                "the core is equally gone either way, so one string serves both",
            ),
            (
                "an absent core",
                None,
                NAMED,
                "the window is tearing down either way",
            ),
            (
                "a preview scratch",
                Some(Liveness::Alive),
                PREVIEW_WITH_PATH,
                "a preview pane has nothing to write and nothing to rename",
            ),
            (
                "a terminal buffer",
                Some(Liveness::Alive),
                TERMINAL_WITH_PATH,
                "Neovim refuses a term:// name for both",
            ),
        ];

        let mut union: HashSet<&'static str> = HashSet::new();
        for (source, liveness, target, why) in shared {
            let (SavePress::Refuse(save), SaveAsPress::Refuse(save_as)) = (
                save_press(liveness, target.target()),
                save_as_press(liveness, target.target()),
            ) else {
                panic!(
                    "{source} was not refused by both presses, so the sharing rule cannot be \
                     checked; the two press tables are red too."
                )
            };
            assert_eq!(
                save, save_as,
                "{source} produces a DIFFERENT reason on the Save and Save As paths, but {why}. \
                 Two strings for one cause is not a safety property, it is drift: the day one is \
                 corrected the other stays wrong, and a reader comparing two stderr lines about \
                 the same blocker cannot tell whether the difference is meaningful."
            );
            union.insert(save);
            union.insert(save_as);
        }

        let (SavePress::Refuse(save_wedged), SaveAsPress::Refuse(save_as_wedged)) = (
            save_press(Some(Liveness::NotResponding), NAMED.target()),
            save_as_press(Some(Liveness::NotResponding), NAMED.target()),
        ) else {
            panic!("a wedged core was not refused by both presses; the two press tables are red")
        };
        assert_ne!(
            save_wedged, save_as_wedged,
            "the wedged-core reason is the SAME string on both paths. It must not be: Save queues \
             a write that a restarted instance would apply to whatever buffer it lands on, while \
             Save As additionally opens a NATIVE MODAL over the opaque stall overlay — covering \
             the Restart button that is the only route out of a stall — and invites the user to \
             name a file that will never be written. Collapsing the two loses the sharper warning \
             at the one moment the user most needs it."
        );
        union.insert(save_wedged);
        union.insert(save_as_wedged);

        assert_eq!(
            union.len(),
            6,
            "the two presses must produce exactly SIX distinct refusal strings across FIVE causes \
             each — four shared plus two spellings of the wedged one — and produced {}. Fewer \
             means two causes collapsed into one message that cannot be acted on; more means a \
             cause that is genuinely the same on both paths has been given two strings, which \
             drifts. The distinct strings seen were: {:?}",
            union.len(),
            {
                let mut sorted: Vec<&'static str> = union.iter().copied().collect();
                sorted.sort_unstable();
                sorted
            }
        );
    }

    // =========================================================================
    // save_dialog_proposal
    // =========================================================================

    /// One [`save_dialog_proposal`] table row.
    struct ProposalCase {
        desc: &'static str,
        path: Option<&'static str>,
        cwd: Option<&'static str>,
        folder: Option<&'static str>,
        name: &'static str,
    }

    /// How many named rows the proposal table carries, as a LITERAL rather than as
    /// `cases.len()` — the `tabs_ui::tests::NAMED_WAKE_CASES` reasoning: a row deleted
    /// along with the number it is measured against is invisible.
    const NAMED_PROPOSAL_CASES: usize = 8;

    /// What the native save dialog opens with, for every shape the buffer's path and
    /// the window's cwd can take.
    ///
    /// FALSIFYING EDIT 1 (behaviour 7) — replace `path.parent()` with `cwd` in the
    /// named branch. The `NAMED_NESTED` row fails ALONE, which is what makes it the
    /// isolating one: the `NAMED` row's parent IS the cwd, deliberately, because that
    /// is why the divergence costs a fresh Untitled buffer nothing. What the wrong
    /// answer costs is that Save As on `src/deep/thing.md` opens at the project root,
    /// so "save a copy beside it" becomes a navigation task. MEASURED: red, and
    /// ISOLATED — this test alone, 366 passed / 1 failed — got
    /// `SaveProposal { folder: Some("/home/u/proj"), name: "thing.md" }` where
    /// `Some("/home/u/proj/src/deep")` belongs.
    ///
    /// FALSIFYING EDIT 2 (behaviour 8) — blank the fallback NAME (`name:
    /// String::new()` in the nameless branch). MEASURED: red, and ISOLATED — this
    /// test alone, 366 passed / 1 failed — `save_dialog_proposal(None,
    /// Some("/home/u/proj")) must be SaveProposal { folder: Some("/home/u/proj"),
    /// name: "Untitled.md" }, got SaveProposal { … name: "" }`, i.e. a dialog opening
    /// with a blank entry. **Re-pinning [`UNTITLED_SAVE_NAME`] is NOT a falsifier for
    /// this table** — measured, only
    /// [`the_untitled_save_name_is_the_new_buffer_spelling`] reddens on it — because
    /// four of these rows spell their expected name with that same const, so the
    /// answer and the expectation move together. That is precisely the tautology the
    /// tripwire exists to cover, and the two must not be conflated.
    ///
    /// FALSIFYING EDIT 3 (the approved extra row) — drop the `!parent.is_empty()`
    /// filter. The bare-name row answers `Some("")` for the folder, which GTK reads as
    /// a directory that is not there rather than as "use your default", so the dialog
    /// opens somewhere the user did not ask for. MEASURED: red, and ISOLATED — this
    /// test alone, 366 passed / 1 failed — `save_dialog_proposal(Some("notes.md"),
    /// Some("/home/u/proj"))` got `SaveProposal { folder: Some(""), name: "notes.md" }`.
    ///
    /// The premise assertions come first: three rows rest on `Path::parent` and
    /// `Path::file_name`'s behaviour for the root, the empty path and a bare name, and
    /// a premise about the standard library is exactly the kind that gets assumed
    /// rather than measured.
    #[test]
    fn save_dialog_proposal_table() {
        assert!(
            Path::new("/").file_name().is_none(),
            "precondition: the filesystem root must have no final component, or the root row below \
             never reaches the fallback branch it exists to pin"
        );
        assert_eq!(
            Path::new("notes.md").parent(),
            Some(Path::new("")),
            "precondition: a bare file name's parent must be the EMPTY path (not None), or the \
             bare-name row below is exercising the `None` branch rather than the empty-parent \
             filter it exists for"
        );
        assert_eq!(
            Path::new(NESTED_FILE).parent(),
            Some(Path::new(NESTED_PARENT)),
            "precondition: the nested fixture's parent must be NESTED_PARENT, or the row that \
             discriminates \"the buffer's own folder\" from \"the window's cwd\" is comparing \
             against the wrong directory"
        );

        let cases = [
            ProposalCase {
                desc: "a never-written Untitled buffer in the project root: name and folder both \
                       come from the path, and here that folder IS the cwd — which is why the \
                       divergence costs this case nothing",
                path: Some(UNTITLED_FILE),
                cwd: Some(CWD),
                folder: Some(CWD),
                name: "Untitled.md",
            },
            ProposalCase {
                desc: "THE ROW THIS TABLE EXISTS FOR — a nested file opens in ITS OWN folder, not \
                       in the window's cwd",
                path: Some(NESTED_FILE),
                cwd: Some(CWD),
                folder: Some(NESTED_PARENT),
                name: NESTED_NAME,
            },
            ProposalCase {
                desc: "a nameless buffer proposes the Untitled spelling in the window's cwd",
                path: None,
                cwd: Some(CWD),
                folder: Some(CWD),
                name: UNTITLED_SAVE_NAME,
            },
            ProposalCase {
                desc: "an EMPTY path is no name either — the DEFENSIVE row: a snapshot never \
                       carries one, but this function is public and takes a bare Option<&Path>",
                path: Some(""),
                cwd: Some(CWD),
                folder: Some(CWD),
                name: UNTITLED_SAVE_NAME,
            },
            ProposalCase {
                desc: "a path with no final component (the filesystem root) is nameless too",
                path: Some("/"),
                cwd: Some(CWD),
                folder: Some(CWD),
                name: UNTITLED_SAVE_NAME,
            },
            ProposalCase {
                desc: "no path and no cwd: GTK keeps its OWN default folder rather than being \
                       handed a synthesized one",
                path: None,
                cwd: None,
                folder: None,
                name: UNTITLED_SAVE_NAME,
            },
            ProposalCase {
                desc: "a named buffer with no cwd still opens in its own folder",
                path: Some(NESTED_FILE),
                cwd: None,
                folder: Some(NESTED_PARENT),
                name: NESTED_NAME,
            },
            ProposalCase {
                desc: "THE APPROVED EXTRA ROW — a bare name has a file name but an EMPTY parent, \
                       so the folder falls through to the cwd while the NAME still comes from the \
                       path. Deliberately not spelled `Untitled.md`: a name equal to the fallback \
                       could not tell the two branches apart",
                path: Some("notes.md"),
                cwd: Some(CWD),
                folder: Some(CWD),
                name: "notes.md",
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_PROPOSAL_CASES,
            "the save_dialog_proposal table must carry its {NAMED_PROPOSAL_CASES} named rows, not \
             {}. The rows are not a sample: two take their folder from a NESTED buffer's own \
             parent (which is what discriminates it from the cwd), one takes the empty-parent \
             fall-through, one the coinciding Untitled-in-the-project-root case, and four the \
             nameless fallback — one of those four being the only row that pins the GTK-default \
             answer of no folder at all.",
            cases.len()
        );

        for case in &cases {
            let got = save_dialog_proposal(case.path.map(Path::new), case.cwd.map(Path::new));
            let want = SaveProposal {
                folder: case.folder.map(PathBuf::from),
                name: case.name.to_string(),
            };
            assert_eq!(
                got, want,
                "save_dialog_proposal({:?}, {:?}) must be {want:?}, got {got:?} — {}. \
                 The proposal is what the user sees the instant the dialog opens: a FOLDER that is \
                 not the buffer's own turns \"save a copy beside it\" into a navigation task, a \
                 folder where None belongs hands GTK a directory instead of letting it use the \
                 place the user was last, and a NAME that is not the buffer's own silently \
                 proposes to write somewhere else.",
                case.path, case.cwd, case.desc
            );
        }

        // --- Non-vacuity, in both directions ----------------------------------
        assert!(
            cases.iter().any(|case| case.name != UNTITLED_SAVE_NAME),
            "no proposal row expects a name taken from the PATH, so a seam that always proposed \
             the Untitled fallback would satisfy every row while Save As on a real file offered to \
             rename it."
        );
        assert!(
            cases.iter().any(|case| case.name == UNTITLED_SAVE_NAME),
            "no proposal row expects the fallback, so the nameless branch is unmeasured and an \
             empty proposed name would go unnoticed — the dialog would open with a blank entry."
        );
        assert!(
            cases.iter().any(|case| case.folder.is_none()),
            "no proposal row expects a folder of None, so nothing states that GTK is left its own \
             default when there is neither a parent nor a cwd."
        );
        assert!(
            cases
                .iter()
                .any(|case| case.folder.is_some() && case.folder != case.cwd),
            "no proposal row expects a folder DIFFERENT from the cwd it was handed, so a seam that \
             always answered the cwd would satisfy every row and behaviour 7 would be unmeasured."
        );
    }

    /// The proposed name is the spelling the rest of the application produces.
    ///
    /// It must equal what `runtime/nvim/init.lua`'s `wolf_next_untitled` produces at
    /// `i == 1`, or Save As proposes a name the app never makes — and a different
    /// EXTENSION would leave the saved buffer outside the Markdown filetype every
    /// preview, statusbar and treesitter path keys on.
    ///
    /// **This test has no falsifier in `src/` by construction.** It is a tripwire on a
    /// PIN, the same shape as
    /// `smoke_headless::the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`:
    /// both of its red runs are edits to the constant it guards, not to behaviour.
    /// MEASURED with the const re-pinned to `"Untitled 1.md"`: red, and ISOLATED —
    /// this test alone, 366 passed / 1 failed, `left: "Untitled 1.md"`, `right:
    /// "Untitled.md"`. The isolation is itself the finding, and it is why this test
    /// exists: no proposal row can see a re-pin, because four of them spell their
    /// expectation with this very const.
    /// The literal on the right is deliberately RE-TYPED rather than read from
    /// [`UNTITLED_SAVE_NAME`], because a comparison of a const against itself is
    /// exactly the tautology this exists to avoid.
    #[test]
    fn the_untitled_save_name_is_the_new_buffer_spelling() {
        assert_eq!(
            UNTITLED_SAVE_NAME, "Untitled.md",
            "UNTITLED_SAVE_NAME drifted from the name `runtime/nvim/init.lua`'s \
             `wolf_next_untitled` produces at i == 1. The Lua side is the authority — it is what \
             names every fresh anonymous buffer, and the literal \"Untitled 1.md\" is never \
             generated — so a change here proposes a name the application never makes, and a \
             changed EXTENSION drops the saved buffer out of the markdown filetype that the \
             preview gate, the statusbar and the treesitter injections all key on. Fix this const, \
             not the Lua."
        );
    }

    // =========================================================================
    // save_as_target
    // =========================================================================

    /// One [`save_as_target`] table row.
    struct TargetCase {
        desc: &'static str,
        chosen: &'static str,
        accepted: bool,
    }

    /// How many named rows the target table carries, as a LITERAL. Same reasoning as
    /// [`NAMED_PROPOSAL_CASES`].
    const NAMED_TARGET_CASES: usize = 9;

    /// What a path chosen in the dialog becomes, for every shape it can take.
    ///
    /// Every edit below reddens THIS TEST ALONE, 366 passed / 1 failed each: nothing
    /// else in the crate calls [`save_as_target`] but [`crate::shell`]'s dialog
    /// callback, which no headless test reaches.
    ///
    /// FALSIFYING EDIT 1 (behaviour 10) — append `.md` when `extension()` is `None`.
    /// The extension-less row fails alone. Its real cost is not cosmetic: GTK's
    /// overwrite confirmation was asked about the path the user typed, so writing a
    /// DIFFERENT one turns a confirmed save into an unconfirmed clobber of a file
    /// nobody was asked about. MEASURED: `save_as_target("/home/u/proj/notes")` got
    /// `Some(SaveAsRequest { path: "/home/u/proj/notes.md", force: true })`.
    ///
    /// FALSIFYING EDIT 2 (behaviour 11) — drop the `is_absolute()` filter. The two
    /// relative rows are accepted, and `wolf_core::input` then resolves them against
    /// the PROCESS working directory rather than Neovim's — `chdir_to`'s recorded
    /// reason — so the file lands somewhere the user did not choose. MEASURED:
    /// `save_as_target("notes.md") must be refused, got Some(SaveAsRequest { path:
    /// "notes.md", force: true })`.
    ///
    /// FALSIFYING EDIT 3 (behaviour 12) — change `force` to `false`. EVERY accepting
    /// row fails. **State honestly what that pins**: it is what the front end
    /// REQUESTS, because the dialog has already confirmed the overwrite. That the
    /// request has the effect it claims — `saveas!` replacing an existing file where
    /// plain `saveas` fails `E13` — is `crates/wolf-core/tests/save_open.rs`'s to
    /// prove against a real Neovim, and nothing here can. MEASURED: the first
    /// accepting row reddens, `save_as_target("/home/u/proj/notes.md")` got
    /// `Some(SaveAsRequest { path: "/home/u/proj/notes.md", force: false })`.
    ///
    /// FALSIFYING EDIT 4 (the approved extra row) — drop the `file_name` check. The
    /// `/` row and the trailing-`..` row are accepted, and they are the only two that
    /// tell that check apart from the `is_absolute` one, since both are absolute.
    /// MEASURED: `save_as_target("/") must be refused, got Some(SaveAsRequest { path:
    /// "/", force: true })`.
    #[test]
    fn save_as_target_table() {
        assert!(
            Path::new("/").is_absolute() && Path::new("/").file_name().is_none(),
            "precondition: the filesystem root must be absolute AND nameless, or the row that \
             separates the file_name check from the is_absolute check separates nothing"
        );
        assert!(
            Path::new("/home/u/proj/..").file_name().is_none(),
            "precondition: a path ending in `..` must have no final component, or the second \
             nameless-absolute row is exercising the same branch as an ordinary file"
        );

        let cases = [
            TargetCase {
                desc: "an ordinary absolute markdown path is accepted VERBATIM",
                chosen: "/home/u/proj/notes.md",
                accepted: true,
            },
            TargetCase {
                desc: "THE ROW BEHAVIOUR 10 EXISTS FOR — no extension at all, and none is added: \
                       proposing `.md` is the DIALOG's job, and rewriting the typed name would \
                       write a file the overwrite confirmation was never asked about",
                chosen: "/home/u/proj/notes",
                accepted: true,
            },
            TargetCase {
                desc: "a NON-markdown extension is not rewritten either",
                chosen: "/home/u/proj/notes.txt",
                accepted: true,
            },
            TargetCase {
                desc:
                    "a name with a space and a dot survives unchanged, so nothing is normalizing \
                       the user's spelling behind the confirmation",
                chosen: "/home/u/proj/my notes.v2.md",
                accepted: true,
            },
            TargetCase {
                desc: "a bare RELATIVE name is refused: core resolves it against the PROCESS cwd \
                       rather than Neovim's, so the file lands somewhere the user did not choose",
                chosen: "notes.md",
                accepted: false,
            },
            TargetCase {
                desc: "a relative path WITH directories is refused for the same reason",
                chosen: "src/deep/notes.md",
                accepted: false,
            },
            TargetCase {
                desc: "the EMPTY path is refused",
                chosen: "",
                accepted: false,
            },
            TargetCase {
                desc: "THE APPROVED EXTRA ROW — the filesystem root is absolute and non-empty and \
                       still has no file name, so it is one of only two rows that tell the \
                       file_name check apart from the is_absolute check",
                chosen: "/",
                accepted: false,
            },
            TargetCase {
                desc:
                    "the second of that pair — an absolute path ending in `..` names a DIRECTORY, \
                       so `file_name` answers None even though the last component is not empty",
                chosen: "/home/u/proj/..",
                accepted: false,
            },
        ];

        assert_eq!(
            cases.len(),
            NAMED_TARGET_CASES,
            "the save_as_target table must carry its {NAMED_TARGET_CASES} named rows, not {}. The \
             rows are not a sample: four accept (one of them extension-less, which is the whole of \
             behaviour 10) and five refuse for three different reasons — relative, empty, and \
             absolute-but-nameless, the last of which needs two rows because it is the only check \
             the is_absolute filter cannot also account for.",
            cases.len()
        );

        for case in &cases {
            let got = save_as_target(Path::new(case.chosen));
            if case.accepted {
                let want = SaveAsRequest {
                    path: PathBuf::from(case.chosen),
                    force: true,
                };
                assert_eq!(
                    got.as_ref(),
                    Some(&want),
                    "save_as_target({:?}) must be accepted VERBATIM with force = true, got \
                     {got:?} — {}. Two distinguishable failures live here. A REFUSAL means the \
                     user answered a native save dialog and nothing at all was written, with no \
                     message anywhere. A REWRITTEN path — an appended extension, a normalized name \
                     — means the file GTK asked the overwrite question about is not the file that \
                     gets written, which turns a confirmed save into an unconfirmed clobber.",
                    case.chosen,
                    case.desc
                );
            } else {
                assert_eq!(
                    got, None,
                    "save_as_target({:?}) must be refused, got {got:?} — {}. A relative path is \
                     resolved by core against the PROCESS working directory rather than Neovim's \
                     (`chdir_to`'s recorded reason), so the file lands somewhere the user never \
                     chose and the buffer is renamed to it. A path with no file name names a \
                     directory, and `saveas` against one fails in a way the input pump degrades to \
                     a warning nobody sees.",
                    case.chosen, case.desc
                );
            }
        }

        // --- Behaviour 12, stated over every accepting row --------------------
        //
        // The loop above already compares against `force: true`, so this restates the
        // rule where a failure NAMES it rather than reporting a struct mismatch — and
        // it carries the honest scope of the claim in its message.
        let mut accepted = 0_usize;
        for case in &cases {
            let Some(request) = save_as_target(Path::new(case.chosen)) else {
                continue;
            };
            accepted += 1;
            assert!(
                request.force,
                "save_as_target({:?}) requested a NON-forcing saveas. The dialog has ALREADY \
                 confirmed the overwrite by the time this runs, and plain `saveas` fails `E13: \
                 File exists` on an existing path — an error `wolf_core::input::apply_batch` \
                 degrades to a `tracing::warn!` the user never sees. The user therefore answers a \
                 confirmation and nothing is written, with nothing on screen to say so. This pins \
                 what the front end REQUESTS; that the request has the effect it claims is \
                 `crates/wolf-core/tests/save_open.rs`'s pair to prove against a real Neovim.",
                case.chosen
            );
        }

        // Non-vacuity in both directions: a seam that refused everything would satisfy
        // every refusal row and every `force` assertion above while Save As silently
        // did nothing, and one that accepted everything would satisfy the accepting
        // rows while smuggling a relative path into core.
        assert!(
            accepted > 0,
            "NOT ONE of the {NAMED_TARGET_CASES} rows was accepted, so the force rule above held \
             vacuously and Save As would write nothing whatever the user chose."
        );
        assert!(
            cases.iter().any(|case| !case.accepted),
            "no save_as_target row expects a refusal, so the absolute-path and file-name rules are \
             unmeasured and a relative path could reach `InputEvent::SaveAs`."
        );
    }
}
