//! The pure rulebook behind the command-line strip, the message toast and the
//! completion popup.
//!
//! No cairo, no pango, no widget: every decision is a function of values, so it runs
//! in the fast unit suite the way [`crate::scrollbar`] and [`crate::editor_inset`]
//! do. `pane_view` paints the rectangles these functions describe and decides
//! nothing itself.
//!
//! ## Three surfaces, one strip's worth of layout space
//!
//! `ext_messages` empties grid 1 and forces `cmdheight=0`, so the command line, the
//! messages and the completion menu all arrive as folded snapshot fields rather than
//! as cells. The strip at the window's bottom is one grid row and stays one grid row:
//! its height is subtracted from the pane area, so a strip that grew with its content
//! would issue a `resize_grid` on every message and reflow the buffer under the
//! reader. The toast and the popup are therefore overlay surfaces, which consume no
//! layout space at all.
//!
//! ## What "scrollable" means for a toast
//!
//! A height budget plus an offset. [`toast_budget_rows`] converts the space above the
//! strip into a line count, [`toast_view`] picks which lines that budget shows, and
//! the offset moves the window back through the older lines. Zero is pinned to the
//! newest line, which is the one a single-line message shows.
//!
//! **Key-driven paging is impossible here, not merely undesirable.**
//! `wolf_core::InputEvent::is_toast_dismissing` makes every `Key` dismiss a
//! non-prompt toast, so a key bound to page the toast would destroy the surface it
//! was paging. The offset is therefore driven by a scroll gesture over the toast, and
//! that gesture is consumed rather than forwarded — which is the front-end-owned
//! exemption `architecture.md` already records under "Input: events".
//!
//! ## The answered-prompt rule belongs to core
//!
//! A `confirm` dialog is resolved inside Neovim, and `wolf_core::redraw::cmdline_hide`
//! moves the confirm toast's messages into `PendingConfirm` and hides the toast,
//! restoring them only when a matching re-prompt arrives in a later flush. So the
//! front-end owes no dismissal logic of its own: [`prompt_answered`] names the
//! observable that follows from core's rule, and the only thing it drives is
//! forgetting a scroll offset into messages that are gone.

use wolf_core::PumItem;

/// The fraction of the space above the strip a toast may occupy.
///
/// Half, so a long `:messages` dump never hides the buffer it is describing. The
/// budget is a line count derived from it rather than a pixel height, because a
/// partly drawn last line reads as a rendering fault.
pub const TOAST_MAX_HEIGHT_FRACTION: f64 = 0.5;

/// Inner padding around the toast's and the popup's text, in device pixels.
///
/// Device pixels rather than points, the unit this crate paints in — the same
/// reinterpretation `crate::statusbar_layout`'s constants carry.
pub const SURFACE_PADDING_PX: f64 = 6.0;

/// The most completion rows the popup shows at once.
///
/// Ten, matching Neovim's own `pumheight` idiom. A taller popup would cover most of
/// a pane for a list the user scrolls through one item at a time.
pub const PUM_MAX_ROWS: usize = 10;

/// The most columns the popup is allowed to grow to.
///
/// A completion `info` field can be a whole paragraph; without a cap one item would
/// stretch the popup past the window edge.
pub const PUM_MAX_COLS: usize = 60;

/// The column a byte offset into the command line lands on, counting from the
/// strip's left edge.
///
/// `Cmdline::pos` and a cmdline-anchored `Pum::col` are both BYTE offsets into
/// `Cmdline::text()`, while the strip is painted in whole cells from its left edge
/// with no indent. This is the one conversion between the two, and it is one
/// function because the caret and the popup anchor must not drift apart: a second
/// copy is exactly how a popup ends up one column from the caret it belongs to.
///
/// `prefix` is everything drawn ahead of the content — the `prompt` a `confirm()` or
/// `input()` dialog supplies, followed by `firstc`. Both are drawn, so both are paid
/// for here.
///
/// An offset inside a multi-byte character is snapped DOWN to a character boundary,
/// and one past the end is clamped, so no input panics the paint path. Column
/// counting is by character, which makes it approximate for a double-width glyph —
/// the same approximation the grid paint makes.
#[must_use]
pub fn cmdline_columns(prefix: &str, text: &str, byte: usize) -> usize {
    let mut b = byte.min(text.len());
    while b > 0 && !text.is_char_boundary(b) {
        b -= 1;
    }
    prefix.chars().count() + text[..b].chars().count()
}

/// How many toast lines fit in `available_px` of space above the strip.
///
/// At least one: a budget of zero would hide a message entirely, which is worse than
/// a cramped one. A non-finite input or a non-positive cell collapses to one line
/// rather than propagating, the same defensive shape
/// [`crate::editor_inset::grid_rect`] uses.
#[must_use]
pub fn toast_budget_rows(available_px: f64, cell_h: f64) -> usize {
    if !available_px.is_finite() || !cell_h.is_finite() || cell_h <= 0.0 {
        return 1;
    }
    let rows = (available_px.max(0.0) * TOAST_MAX_HEIGHT_FRACTION / cell_h).floor();
    if rows < 1.0 {
        return 1;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rows is finite, floored and >= 1.0 here"
    )]
    {
        (rows as usize).max(1)
    }
}

/// The slice of a toast's lines that is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToastView {
    /// Index of the first visible line.
    pub first: usize,
    /// How many lines are visible.
    pub len: usize,
    /// Whether any line is off screen, so the surface is worth scrolling.
    pub scrollable: bool,
}

/// Which lines of a `total`-line toast a `budget`-line surface shows at `scroll`.
///
/// `scroll` counts lines BACK from the newest: zero pins the window to the end, so a
/// message that arrives while the user has scrolled back does not silently replace
/// the line being read — the caller decides that, through [`carried_scroll`].
#[must_use]
pub fn toast_view(total: usize, budget: usize, scroll: usize) -> ToastView {
    let budget = budget.max(1);
    let len = total.min(budget);
    let max_scroll = total.saturating_sub(len);
    let scroll = scroll.min(max_scroll);
    ToastView {
        first: max_scroll - scroll,
        len,
        scrollable: total > len,
    }
}

/// Clamps a requested scroll offset into what `total` and `budget` allow.
///
/// Takes an `i64` because a scroll gesture accumulates a signed delta, and returning
/// the clamp rather than refusing means a fast flick lands at the top instead of
/// being dropped.
#[must_use]
pub fn toast_scroll_clamp(total: usize, budget: usize, requested: i64) -> usize {
    let budget = budget.max(1);
    let max = total.saturating_sub(total.min(budget));
    if requested <= 0 {
        return 0;
    }
    #[allow(
        clippy::cast_sign_loss,
        reason = "requested is strictly positive on this arm"
    )]
    let requested = requested as usize;
    requested.min(max)
}

/// The scroll offset to carry into the next frame.
///
/// A changed line count is a different message, so the surface re-pins to the newest
/// line rather than keeping an offset into lines that no longer exist. Otherwise the
/// offset survives, re-clamped against the current budget so a window resize cannot
/// strand it past the end.
#[must_use]
pub fn carried_scroll(prev_lines: usize, next_lines: usize, budget: usize, scroll: usize) -> usize {
    if prev_lines == next_lines {
        toast_scroll_clamp(
            next_lines,
            budget,
            i64::try_from(scroll).unwrap_or(i64::MAX),
        )
    } else {
        0
    }
}

/// Whether the toast surface is on screen this frame.
///
/// A visible toast with no lines is an empty box, not a message, and Neovim does emit
/// one: a `msg_show` whose content is a single empty chunk.
#[must_use]
pub fn toast_is_shown(visible: bool, lines: usize) -> bool {
    visible && lines > 0
}

/// Whether a `confirm` prompt that was on screen has been answered.
///
/// Core owns the decision and this is only its observable: `redraw::cmdline_hide`
/// hides a confirm toast the instant its dialog closes and restores it only for a
/// matching re-prompt, so from here "answered" is exactly "a prompt was showing and
/// the toast is no longer visible". Nothing in this crate may dismiss a prompt on its
/// own — an invalid keypress re-prompts, and a front-end that hid the toast on the
/// keypress would leave the user answering a question they can no longer read.
#[must_use]
pub fn prompt_answered(was_prompt: bool, now_visible: bool) -> bool {
    was_prompt && !now_visible
}

/// The selected completion item, or `None`.
///
/// `Pum::selected` is an `i64` carrying `-1` for "nothing selected", which is the
/// state `:` completion starts in and returns to when the user types past every
/// match. An index past the end is refused rather than clamped: a selection the item
/// list cannot explain is a fold error, and highlighting the last row would hide it.
#[must_use]
pub fn pum_selection(items: usize, selected: i64) -> Option<usize> {
    let idx = usize::try_from(selected).ok()?;
    (idx < items).then_some(idx)
}

/// The slice of a completion list that is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PumView {
    /// Index of the first visible item.
    pub first: usize,
    /// How many items are visible.
    pub len: usize,
}

/// Which completion items a `budget`-row popup shows, given the selection.
///
/// The window scrolls only as far as the selection forces it, so moving down through
/// a long list keeps the popup still until the selection reaches the last visible
/// row. With nothing selected the list is shown from its start, which is what a
/// freshly opened `:` completion looks like.
#[must_use]
pub fn pum_view(items: usize, selected: Option<usize>, budget: usize) -> PumView {
    let budget = budget.max(1);
    let len = items.min(budget);
    let first = match selected {
        Some(sel) if sel >= len => (sel + 1 - len).min(items.saturating_sub(len)),
        _ => 0,
    };
    PumView { first, len }
}

/// The text one completion row draws.
///
/// `word` is what gets inserted and is always present; `kind` and `menu` are hints
/// Neovim may leave empty, and an empty one contributes no separator, so a row never
/// ends in trailing space that a width measurement would then pay for.
#[must_use]
pub fn pum_item_label(item: &PumItem) -> String {
    let mut label = item.word.clone();
    for extra in [&item.kind, &item.menu] {
        if !extra.is_empty() {
            label.push(' ');
            label.push_str(extra);
        }
    }
    label
}

/// The popup's width in cells: its widest visible row, capped at [`PUM_MAX_COLS`].
///
/// Measured over the VISIBLE rows only. Measuring the whole list would make the popup
/// jump wider the moment a long item scrolled into range, and narrower again when it
/// left.
#[must_use]
pub fn pum_width_cols(items: &[PumItem], view: PumView) -> usize {
    items
        .iter()
        .skip(view.first)
        .take(view.len)
        .map(|i| pum_item_label(i).chars().count())
        .max()
        .unwrap_or(0)
        .min(PUM_MAX_COLS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(word: &str, kind: &str, menu: &str) -> PumItem {
        PumItem {
            word: word.to_string(),
            kind: kind.to_string(),
            menu: menu.to_string(),
            info: String::new(),
        }
    }

    #[test]
    fn cmdline_columns_counts_the_prefix_and_the_bytes_before_the_offset() {
        // (firstc, text, byte, expected columns)
        let table: &[(&str, &str, usize, usize)] = &[
            (":", "", 0, 1),
            (":", "set", 0, 1),
            (":", "set", 3, 4),
            (":", "set", 1, 2),
            ("/", "abc", 2, 3),
            ("", "abc", 3, 3),
        ];
        for (firstc, text, byte, want) in table {
            assert_eq!(
                cmdline_columns(firstc, text, *byte),
                *want,
                "firstc {firstc:?} text {text:?} byte {byte}"
            );
        }
    }

    #[test]
    fn cmdline_columns_snaps_a_mid_character_offset_down_and_clamps_past_the_end() {
        // "é" is two bytes; byte 1 is inside it.
        assert_eq!(cmdline_columns(":", "é", 1), 1, "mid-character snaps down");
        assert_eq!(
            cmdline_columns(":", "é", 2),
            2,
            "the boundary counts one char"
        );
        assert_eq!(cmdline_columns(":", "é", 99), 2, "past the end clamps");
        assert_eq!(
            cmdline_columns(":", "", 99),
            1,
            "empty text clamps to firstc"
        );
    }

    #[test]
    fn toast_budget_rows_is_half_the_space_and_never_zero() {
        assert_eq!(
            toast_budget_rows(400.0, 20.0),
            10,
            "400 px, half, 20 px cells"
        );
        assert_eq!(toast_budget_rows(41.0, 20.0), 1, "one whole line fits");
        assert_eq!(
            toast_budget_rows(20.0, 20.0),
            1,
            "a partial line still gives one"
        );
        assert_eq!(toast_budget_rows(0.0, 20.0), 1, "no space still gives one");
        assert_eq!(
            toast_budget_rows(-100.0, 20.0),
            1,
            "a negative extent gives one"
        );
        assert_eq!(toast_budget_rows(f64::NAN, 20.0), 1, "NaN collapses");
        assert_eq!(toast_budget_rows(400.0, 0.0), 1, "a zero cell collapses");
        assert_eq!(
            toast_budget_rows(400.0, f64::INFINITY),
            1,
            "an infinite cell collapses"
        );
    }

    #[test]
    fn toast_view_pins_to_the_newest_line_and_scrolls_back_through_the_older_ones() {
        // (total, budget, scroll, first, len, scrollable)
        let table: &[(usize, usize, usize, usize, usize, bool)] = &[
            (1, 5, 0, 0, 1, false),
            (5, 5, 0, 0, 5, false),
            (5, 2, 0, 3, 2, true),
            (5, 2, 1, 2, 2, true),
            (5, 2, 3, 0, 2, true),
            (5, 2, 99, 0, 2, true),
            (0, 3, 0, 0, 0, false),
            (5, 0, 0, 4, 1, true),
        ];
        for (total, budget, scroll, first, len, scrollable) in table {
            assert_eq!(
                toast_view(*total, *budget, *scroll),
                ToastView {
                    first: *first,
                    len: *len,
                    scrollable: *scrollable
                },
                "total {total} budget {budget} scroll {scroll}"
            );
        }
    }

    #[test]
    fn toast_scroll_clamp_refuses_a_negative_and_stops_at_the_oldest_line() {
        assert_eq!(
            toast_scroll_clamp(5, 2, -3),
            0,
            "negative clamps to the newest"
        );
        assert_eq!(toast_scroll_clamp(5, 2, 0), 0);
        assert_eq!(toast_scroll_clamp(5, 2, 2), 2);
        assert_eq!(
            toast_scroll_clamp(5, 2, 3),
            3,
            "the oldest line is reachable"
        );
        assert_eq!(toast_scroll_clamp(5, 2, 400), 3, "past the oldest clamps");
        assert_eq!(toast_scroll_clamp(2, 5, 4), 0, "nothing to scroll");
    }

    #[test]
    fn carried_scroll_forgets_the_offset_when_the_message_changes() {
        assert_eq!(
            carried_scroll(5, 5, 2, 3),
            3,
            "the same message keeps its offset"
        );
        assert_eq!(carried_scroll(5, 6, 2, 3), 0, "a longer message re-pins");
        assert_eq!(carried_scroll(5, 1, 2, 3), 0, "a shorter message re-pins");
        assert_eq!(
            carried_scroll(9, 9, 8, 7),
            1,
            "a grown budget re-clamps a surviving offset"
        );
    }

    #[test]
    fn toast_is_shown_needs_both_a_visible_flag_and_a_line() {
        assert!(toast_is_shown(true, 1));
        assert!(
            !toast_is_shown(true, 0),
            "an empty message is not a surface"
        );
        assert!(!toast_is_shown(false, 3));
        assert!(!toast_is_shown(false, 0));
    }

    #[test]
    fn prompt_answered_is_a_prompt_that_stopped_being_visible() {
        assert!(prompt_answered(true, false), "the dialog closed");
        assert!(!prompt_answered(true, true), "still waiting for an answer");
        assert!(!prompt_answered(false, false), "no prompt was showing");
        assert!(!prompt_answered(false, true));
    }

    #[test]
    fn pum_selection_maps_minus_one_and_out_of_range_to_none() {
        assert_eq!(pum_selection(3, -1), None, "nothing selected");
        assert_eq!(pum_selection(3, 0), Some(0));
        assert_eq!(pum_selection(3, 2), Some(2));
        assert_eq!(pum_selection(3, 3), None, "one past the end is refused");
        assert_eq!(pum_selection(0, 0), None, "an empty list selects nothing");
        assert_eq!(pum_selection(3, -7), None);
        assert_eq!(pum_selection(3, i64::MAX), None);
    }

    #[test]
    fn pum_view_holds_still_until_the_selection_leaves_the_window() {
        // (items, selected, budget, first, len)
        let table: &[(usize, Option<usize>, usize, usize, usize)] = &[
            (10, None, 3, 0, 3),
            (10, Some(0), 3, 0, 3),
            (10, Some(2), 3, 0, 3),
            (10, Some(3), 3, 1, 3),
            (10, Some(9), 3, 7, 3),
            (2, Some(1), 5, 0, 2),
            (0, None, 5, 0, 0),
            (10, Some(9), 0, 9, 1),
        ];
        for (items, selected, budget, first, len) in table {
            assert_eq!(
                pum_view(*items, *selected, *budget),
                PumView {
                    first: *first,
                    len: *len
                },
                "items {items} selected {selected:?} budget {budget}"
            );
        }
    }

    #[test]
    fn pum_item_label_appends_only_the_hints_neovim_filled_in() {
        assert_eq!(pum_item_label(&item("set", "", "")), "set");
        assert_eq!(pum_item_label(&item("set", "f", "")), "set f");
        assert_eq!(pum_item_label(&item("set", "", "opt")), "set opt");
        assert_eq!(pum_item_label(&item("set", "f", "opt")), "set f opt");
    }

    #[test]
    fn pum_width_cols_measures_the_visible_rows_only_and_caps() {
        let items = vec![
            item("a", "", ""),
            item("bbbb", "", ""),
            item("cc", "", ""),
            item(&"z".repeat(200), "", ""),
        ];
        assert_eq!(
            pum_width_cols(&items, PumView { first: 0, len: 3 }),
            4,
            "the long item is off screen and is not paid for"
        );
        assert_eq!(
            pum_width_cols(&items, PumView { first: 3, len: 1 }),
            PUM_MAX_COLS,
            "a very long visible item is capped"
        );
        assert_eq!(
            pum_width_cols(&items, PumView { first: 0, len: 0 }),
            0,
            "nothing visible is zero wide"
        );
    }
}
