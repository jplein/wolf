//! The `<C-w>` prefix router a terminal session's keys pass through first: a pure state
//! machine over the held prefix and its deadline. See `specs/terminal-panes.md#input`.

use std::time::Instant;

use super::TERMINAL_PREFIX_TIMEOUT;
use crate::input::{Key, KeyEvent};
use crate::layout::{FocusDir, SplitOrientation};

/// What a key routed through the prefix does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// `<C-w>` is held; nothing is sent yet.
    Hold,
    /// These keys go to the program, in order.
    Send(Vec<KeyEvent>),
    /// Split the session's pane, with a new session in the new window.
    Split(SplitOrientation),
    /// Close the session's pane.
    Close,
    /// Move focus from the session's pane.
    Focus(FocusDir),
    /// The key is dropped.
    Ignore,
}

/// The held-prefix state.
#[derive(Debug, Default)]
pub(crate) struct Router {
    held_since: Option<Instant>,
}

/// The `<C-w>` key event a held prefix sends when it turns out to be no chord.
pub(crate) const CTRL_W: KeyEvent = KeyEvent {
    key: Key::Char('w'),
    ctrl: true,
    shift: false,
    alt: false,
    meta: false,
};

impl Router {
    /// Routes one key press at `now`. A `meta` key is a front-end accelerator and dropped.
    pub(crate) fn route(&mut self, key: &KeyEvent, now: Instant) -> Route {
        if key.meta {
            return Route::Ignore;
        }
        if self.held_since.take().is_none() {
            if is_prefix(key) {
                self.held_since = Some(now);
                return Route::Hold;
            }
            return Route::Send(vec![*key]);
        }
        match chord(key) {
            Some(route) => route,
            None => Route::Send(vec![CTRL_W, *key]),
        }
    }

    /// Releases a held prefix whose deadline has passed at `now`, returning whether one was.
    pub(crate) fn flush_due(&mut self, now: Instant) -> bool {
        match self.deadline() {
            Some(deadline) if now >= deadline => {
                self.held_since = None;
                true
            }
            _ => false,
        }
    }

    /// Releases a held prefix regardless of its deadline, for text or a paste that follows
    /// it, returning whether one was held.
    pub(crate) fn take_held(&mut self) -> bool {
        self.held_since.take().is_some()
    }

    /// When a held prefix is sent on its own.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.held_since.map(|since| since + TERMINAL_PREFIX_TIMEOUT)
    }
}

/// `<C-w>`: `w` or `W` with Ctrl and neither Alt nor meta.
fn is_prefix(key: &KeyEvent) -> bool {
    matches!(key.key, Key::Char('w' | 'W')) && key.ctrl && !key.alt && !key.meta
}

/// The chord a key completes after `<C-w>`: a plain `s v c q h j k l`, or an unmodified arrow.
fn chord(key: &KeyEvent) -> Option<Route> {
    let plain = !key.ctrl && !key.alt && !key.meta;
    if !plain {
        return None;
    }
    let route = match key.key {
        Key::Char('s') => Route::Split(SplitOrientation::Horizontal),
        Key::Char('v') => Route::Split(SplitOrientation::Vertical),
        Key::Char('c' | 'q') => Route::Close,
        Key::Char('h') => Route::Focus(FocusDir::Left),
        Key::Char('j') => Route::Focus(FocusDir::Down),
        Key::Char('k') => Route::Focus(FocusDir::Up),
        Key::Char('l') => Route::Focus(FocusDir::Right),
        Key::Left if !key.shift => Route::Focus(FocusDir::Left),
        Key::Down if !key.shift => Route::Focus(FocusDir::Down),
        Key::Up if !key.shift => Route::Focus(FocusDir::Up),
        Key::Right if !key.shift => Route::Focus(FocusDir::Right),
        _ => return None,
    };
    Some(route)
}
