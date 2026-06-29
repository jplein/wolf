//! Layout intents: the Neovim->shell INTENT CHANNEL types.
//!
//! Under the split-panes design (see the crate README and `architecture.md`), the
//! SHELL owns the pane layout. Neovim's split/focus chords and Ex-commands are
//! overridden in `runtime/nvim/init.lua` to `rpcnotify` a `wolf_layout` payload
//! instead of (or in addition to) performing the native window op. [`crate::redraw`]
//! parses that payload into a [`LayoutIntent`] and pushes it onto a drainable
//! queue; the front-end consumes intents on wake via [`crate::api::WolfCore::drain_intents`].
//!
//! Asymmetry (decision D4): a SPLIT is PERFORMED by nvim (so it really creates the
//! window + grid and emits `win_pos`) AND reported as a [`LayoutIntent::Split`] so
//! the shell learns the orientation and which pane to split. A focus MOVE is a
//! PURE intent ([`LayoutIntent::FocusDir`]): nvim does NOT change focus; the shell
//! computes the neighbouring pane in its own layout and drives the focus change.
//! A CLOSE is performed by nvim (guarded against closing the last window) and the
//! shell observes it via the `win_close` redraw event — there is no close intent.

use rmpv::Value;

/// The orientation of a split, from the shell's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    /// A horizontal divider: the panes are stacked one above the other
    /// (`:split` / `<C-w>s`).
    Horizontal,
    /// A vertical divider: the panes sit side by side (`:vsplit` / `<C-w>v`).
    Vertical,
}

/// A direction for a focus move (`<C-w>h/j/k/l` and the arrow variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDir {
    /// Move focus to the pane on the left.
    Left,
    /// Move focus to the pane on the right.
    Right,
    /// Move focus to the pane above.
    Up,
    /// Move focus to the pane below.
    Down,
}

/// A layout intent decoded from a `wolf_layout` notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutIntent {
    /// A split was performed by nvim. `from` is the window that was current
    /// BEFORE the split; `win` is the newly created window (current AFTER the
    /// split). The shell splits the `from` pane in the given `orientation` and
    /// binds the new pane to `win` (resolving its grid via `win_pos`).
    Split {
        /// The split orientation.
        orientation: SplitOrientation,
        /// The origin window handle (current before the split).
        from: i64,
        /// The new window handle (current after the split).
        win: i64,
    },
    /// A PURE focus-move request. `from` is the window that was current when the
    /// chord fired. nvim did NOT change focus; the shell resolves the neighbour
    /// in `dir` and drives the focus change itself.
    FocusDir {
        /// The direction to move focus.
        dir: FocusDir,
        /// The window the move originates from.
        from: i64,
    },
}

impl LayoutIntent {
    /// Parses a `wolf_layout` notification payload (the single map argument) into
    /// a [`LayoutIntent`], tolerating any malformed/partial payload by returning
    /// `None`.
    ///
    /// The Lua side sends a map with an `action` discriminator:
    /// `action == "split"` carries `orientation` (`"h"`/`"v"`), `from`, `win`;
    /// `action == "focus_dir"` carries `dir` (`"left"`/`"right"`/`"up"`/`"down"`)
    /// and `from`. Window handles arrive as plain integers (Lua numbers), but we
    /// also tolerate the msgpack-`Ext` encoding for robustness.
    #[must_use]
    pub fn from_payload(payload: &Value) -> Option<Self> {
        let map = payload.as_map()?;
        let get = |key: &str| -> Option<&Value> {
            map.iter()
                .find(|(k, _)| k.as_str() == Some(key))
                .map(|(_, v)| v)
        };

        match get("action").and_then(Value::as_str)? {
            "split" => {
                let orientation = match get("orientation").and_then(Value::as_str)? {
                    "h" => SplitOrientation::Horizontal,
                    "v" => SplitOrientation::Vertical,
                    _ => return None,
                };
                let from = get("from").and_then(handle_i64)?;
                let win = get("win").and_then(handle_i64)?;
                Some(LayoutIntent::Split {
                    orientation,
                    from,
                    win,
                })
            }
            "focus_dir" => {
                let dir = match get("dir").and_then(Value::as_str)? {
                    "left" => FocusDir::Left,
                    "right" => FocusDir::Right,
                    "up" => FocusDir::Up,
                    "down" => FocusDir::Down,
                    _ => return None,
                };
                let from = get("from").and_then(handle_i64)?;
                Some(LayoutIntent::FocusDir { dir, from })
            }
            _ => None,
        }
    }
}

/// Decodes a window handle that arrives either as a plain `Value::Integer` (the
/// common case — Lua sends numbers) or as a msgpack-`Ext` wrapping an encoded
/// integer. Returns the handle as an `i64`, or `None` if it is neither.
fn handle_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(_) => v.as_i64(),
        Value::Ext(_type, data) => {
            let mut bytes: &[u8] = data;
            rmpv::decode::read_value(&mut bytes)
                .ok()
                .and_then(|inner| inner.as_i64())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an `rmpv` map `Value` from `(key, value)` string-keyed pairs — the
    /// exact `Value` shape `wolf_layout` delivers (a map whose keys are strings).
    /// This mirrors what `init.lua`'s `rpcnotify` sends and what
    /// [`LayoutIntent::from_payload`] parses (`payload.as_map()`).
    fn map(pairs: Vec<(&str, Value)>) -> Value {
        Value::Map(
            pairs
                .into_iter()
                .map(|(k, v)| (Value::String(k.into()), v))
                .collect(),
        )
    }

    /// A plain integer handle, the common case Lua sends for `from`/`win`.
    fn int(n: i64) -> Value {
        Value::Integer(n.into())
    }

    /// A valid `split` map -> `Some(Split { Vertical, .. })`, preserving
    /// `from`/`win`.
    #[test]
    fn from_payload_valid_split_vertical() {
        let payload = map(vec![
            ("action", Value::from("split")),
            ("orientation", Value::from("v")),
            ("from", int(5)),
            ("win", int(7)),
        ]);
        assert_eq!(
            LayoutIntent::from_payload(&payload),
            Some(LayoutIntent::Split {
                orientation: SplitOrientation::Vertical,
                from: 5,
                win: 7,
            })
        );
    }

    /// `orientation == "h"` maps to [`SplitOrientation::Horizontal`].
    #[test]
    fn from_payload_valid_split_horizontal() {
        let payload = map(vec![
            ("action", Value::from("split")),
            ("orientation", Value::from("h")),
            ("from", int(1)),
            ("win", int(2)),
        ]);
        assert_eq!(
            LayoutIntent::from_payload(&payload),
            Some(LayoutIntent::Split {
                orientation: SplitOrientation::Horizontal,
                from: 1,
                win: 2,
            })
        );
    }

    /// A valid `focus_dir` map -> `Some(FocusDir { Left, .. })`.
    #[test]
    fn from_payload_valid_focus_left() {
        let payload = map(vec![
            ("action", Value::from("focus_dir")),
            ("dir", Value::from("left")),
            ("from", int(3)),
        ]);
        assert_eq!(
            LayoutIntent::from_payload(&payload),
            Some(LayoutIntent::FocusDir {
                dir: FocusDir::Left,
                from: 3,
            })
        );
    }

    /// Every `dir` string maps to the matching [`FocusDir`] variant.
    #[test]
    fn from_payload_focus_all_dirs_map_correctly() {
        let cases = [
            ("left", FocusDir::Left),
            ("right", FocusDir::Right),
            ("up", FocusDir::Up),
            ("down", FocusDir::Down),
        ];
        for (dir_str, want) in cases {
            let payload = map(vec![
                ("action", Value::from("focus_dir")),
                ("dir", Value::from(dir_str)),
                ("from", int(9)),
            ]);
            assert_eq!(
                LayoutIntent::from_payload(&payload),
                Some(LayoutIntent::FocusDir {
                    dir: want,
                    from: 9,
                }),
                "dir {dir_str:?} should map to {want:?}"
            );
        }
    }

    /// A missing `action` discriminator yields `None`.
    #[test]
    fn from_payload_missing_action_is_none() {
        let payload = map(vec![
            ("orientation", Value::from("v")),
            ("from", int(5)),
            ("win", int(7)),
        ]);
        assert_eq!(LayoutIntent::from_payload(&payload), None);
    }

    /// An unknown `action` value yields `None`.
    #[test]
    fn from_payload_unknown_action_is_none() {
        let payload = map(vec![
            ("action", Value::from("frobnicate")),
            ("from", int(5)),
            ("win", int(7)),
        ]);
        assert_eq!(LayoutIntent::from_payload(&payload), None);
    }

    /// A `split` missing `from` (or `win`) yields `None`.
    #[test]
    fn from_payload_split_missing_fields_is_none() {
        let missing_win = map(vec![
            ("action", Value::from("split")),
            ("orientation", Value::from("v")),
            ("from", int(5)),
        ]);
        assert_eq!(LayoutIntent::from_payload(&missing_win), None);

        let missing_from = map(vec![
            ("action", Value::from("split")),
            ("orientation", Value::from("h")),
            ("win", int(7)),
        ]);
        assert_eq!(LayoutIntent::from_payload(&missing_from), None);
    }

    /// A `split` with an unrecognized `orientation` yields `None`.
    #[test]
    fn from_payload_split_bad_orientation_is_none() {
        let payload = map(vec![
            ("action", Value::from("split")),
            ("orientation", Value::from("x")),
            ("from", int(5)),
            ("win", int(7)),
        ]);
        assert_eq!(LayoutIntent::from_payload(&payload), None);
    }

    /// A `focus_dir` with an unrecognized `dir` yields `None`.
    #[test]
    fn from_payload_focus_bad_dir_is_none() {
        let payload = map(vec![
            ("action", Value::from("focus_dir")),
            ("dir", Value::from("sideways")),
            ("from", int(3)),
        ]);
        assert_eq!(LayoutIntent::from_payload(&payload), None);
    }

    /// A non-map payload (e.g. a bare integer) yields `None`.
    #[test]
    fn from_payload_non_map_is_none() {
        assert_eq!(LayoutIntent::from_payload(&int(42)), None);
    }
}
