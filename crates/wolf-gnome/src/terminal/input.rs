//! Where a terminal pane's committed text, focus changes and pointer events go. Pure: GDK value
//! types in, `wolf-core` values out.

use wolf_core::{Key, KeyEvent, MouseButton, TerminalId};

/// The first printable scalar (space); every scalar below it is a C0 control.
const FIRST_PRINTABLE: char = ' ';
/// The last printable ASCII scalar.
const LAST_PRINTABLE_ASCII: char = '~';
/// The DEL control scalar.
const DEL: char = '\u{7f}';

/// Where text an input method committed in a terminal pane goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalCommit {
    /// Nothing to send.
    Nothing,
    /// One printable ASCII character, sent through `terminal_key`.
    Key(KeyEvent),
    /// Any other text, sent through `terminal_text`.
    Text(String),
}

/// Routes committed `text`. Exactly one printable ASCII scalar is a modifier-free key, so core's
/// `<C-w>` router sees it; other text loses its C0 and DEL scalars; nothing left is nothing.
#[must_use]
pub fn commit_route(text: &str) -> TerminalCommit {
    let mut chars = text.chars();
    if let (Some(only), None) = (chars.next(), chars.next()) {
        if (FIRST_PRINTABLE..=LAST_PRINTABLE_ASCII).contains(&only) {
            return TerminalCommit::Key(KeyEvent {
                key: Key::Char(only),
                ctrl: false,
                shift: false,
                alt: false,
                meta: false,
            });
        }
    }
    let kept: String = text
        .chars()
        .filter(|&c| c >= FIRST_PRINTABLE && c != DEL)
        .collect();
    if kept.is_empty() {
        TerminalCommit::Nothing
    } else {
        TerminalCommit::Text(kept)
    }
}

/// One `terminal_focus` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusReport {
    pub session: TerminalId,
    pub gained: bool,
}

/// The `terminal_focus` calls for focus moving from `previous` to `current`: none when equal,
/// else the loss and then the gain, so no program sees two sessions focused at once.
#[must_use]
pub fn focus_reports(
    previous: Option<TerminalId>,
    current: Option<TerminalId>,
) -> Vec<FocusReport> {
    if previous == current {
        return Vec::new();
    }
    let loss = previous.map(|session| FocusReport {
        session,
        gained: false,
    });
    let gain = current.map(|session| FocusReport {
        session,
        gained: true,
    });
    loss.into_iter().chain(gain).collect()
}

/// The terminal button for GDK button `button`: primary, middle and secondary; any other
/// button sends nothing.
#[must_use]
pub fn mouse_button(button: u32) -> Option<MouseButton> {
    match button {
        gdk::BUTTON_PRIMARY => Some(MouseButton::Left),
        gdk::BUTTON_MIDDLE => Some(MouseButton::Middle),
        gdk::BUTTON_SECONDARY => Some(MouseButton::Right),
        _ => None,
    }
}

/// The modifiers a terminal mouse or scroll event carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerMods {
    /// Bypasses the program's mouse tracking, so a drag selects.
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Shift, Control and Alt from `state`; every other bit is dropped.
#[must_use]
pub fn pointer_mods(state: gdk::ModifierType) -> PointerMods {
    PointerMods {
        shift: state.contains(gdk::ModifierType::SHIFT_MASK),
        ctrl: state.contains(gdk::ModifierType::CONTROL_MASK),
        alt: state.contains(gdk::ModifierType::ALT_MASK),
    }
}
