//! Abstract input events (front-end -> core) and their translation into Neovim
//! input notation and RPC.
//!
//! Front-ends speak in toolkit-neutral [`InputEvent`]s; this module turns them
//! into Neovim's keystroke notation (`<Esc>`, `<C-w>`, `<CR>`, ...) and the
//! corresponding generic msgpack-RPC calls (`nvim_input`, `nvim_input_mouse`,
//! `nvim_ui_try_resize`, and Ex commands for open/save).

use std::path::{Path, PathBuf};

use nvim_rs::{compat::tokio::Compat, error::CallError, Neovim};
use rmpv::Value;
use tokio::process::ChildStdin;

/// The Neovim handle type used throughout core (generic untyped RPC only).
pub(crate) type Nvim = Neovim<Compat<ChildStdin>>;

/// A logical key, independent of any platform key-code scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// The Enter / Return key.
    Enter,
    /// The Escape key.
    Esc,
    /// The Tab key.
    Tab,
    /// The Backspace key.
    Backspace,
    /// The Delete (forward-delete) key.
    Delete,
    /// The Left arrow.
    Left,
    /// The Right arrow.
    Right,
    /// The Up arrow.
    Up,
    /// The Down arrow.
    Down,
    /// The Home key.
    Home,
    /// The End key.
    End,
    /// The Page Up key.
    PageUp,
    /// The Page Down key.
    PageDown,
    /// A function key `F1`..=`F12` (and beyond), carrying its number.
    F(u8),
}

/// A key press with its active modifiers.
///
/// Shift contract: for a [`Key::Char`], the front-end is expected to deliver the
/// already-shifted glyph in the char (e.g. `'A'`, `'!'`) and leave `shift =
/// false`. The `S-` notation prefix is only meaningful for, and only applied to,
/// named keys (`<S-Tab>`, `<S-F1>`, `<S-Left>`, ...), so `shift` is ignored when
/// `key` is a `Key::Char`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// The base key.
    pub key: Key,
    /// Control modifier.
    pub ctrl: bool,
    /// Shift modifier. For [`Key::Char`] this is ignored (deliver the shifted
    /// glyph in the char instead); it only applies to named keys.
    pub shift: bool,
    /// Alt / Option modifier.
    pub alt: bool,
    /// Meta / Command / Super modifier.
    pub meta: bool,
}

/// A mouse button for [`MouseEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Right button.
    Right,
    /// Middle button.
    Middle,
}

/// The kind of mouse action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    /// Button pressed.
    Press,
    /// Button released.
    Release,
    /// Pointer dragged with a button held.
    Drag,
}

/// A mouse event at a grid cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    /// Which button.
    pub button: MouseButton,
    /// Press / release / drag.
    pub action: MouseAction,
    /// Target grid id. Under `ext_multigrid` this must be the pane's WINDOW grid
    /// (`>= 2`) with GRID-LOCAL [`row`](Self::row)/[`col`](Self::col); grid 0 is
    /// the global/default grid.
    pub grid: u64,
    /// Grid row (grid-local under multigrid).
    pub row: u32,
    /// Grid column (grid-local under multigrid).
    pub col: u32,
    /// Control modifier.
    pub ctrl: bool,
    /// Shift modifier.
    pub shift: bool,
    /// Alt modifier.
    pub alt: bool,
}

/// A scroll direction for [`ScrollEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDirection {
    /// Scroll up.
    Up,
    /// Scroll down.
    Down,
    /// Scroll left.
    Left,
    /// Scroll right.
    Right,
}

/// A scroll-wheel event at a grid cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollEvent {
    /// Scroll direction.
    pub direction: ScrollDirection,
    /// Target grid id. Under `ext_multigrid` this must be the pane's WINDOW grid
    /// (`>= 2`) with GRID-LOCAL [`row`](Self::row)/[`col`](Self::col); grid 0 is
    /// the global/default grid.
    pub grid: u64,
    /// Grid row of the pointer (grid-local under multigrid).
    pub row: u32,
    /// Grid column of the pointer (grid-local under multigrid).
    pub col: u32,
}

/// An abstract input event from a front-end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// A key press.
    Key(KeyEvent),
    /// A mouse button event.
    Mouse(MouseEvent),
    /// A scroll-wheel event.
    Scroll(ScrollEvent),
    /// A grid resize request (in cells).
    Resize {
        /// New column count.
        cols: u16,
        /// New row count.
        rows: u16,
    },
    /// A per-grid resize request (in cells) for a specific window grid.
    ResizeGrid {
        /// Target grid id.
        grid: u64,
        /// New column count.
        cols: u16,
        /// New row count.
        rows: u16,
    },
    /// Focus a specific Neovim window (delegated to `nvim_set_current_win`).
    ///
    /// The shell uses this both for click-to-focus and for resolving a
    /// [`crate::layout::LayoutIntent::FocusDir`] into a concrete window.
    FocusWin {
        /// The target Neovim window handle.
        win: i64,
    },
    /// Open a file (delegated to Neovim's `edit`).
    Open {
        /// Path to open.
        path: PathBuf,
    },
    /// Save the current buffer (delegated to Neovim's `write`).
    Save,
    /// Save the current buffer to a new path (delegated to `saveas`).
    SaveAs {
        /// Destination path.
        path: PathBuf,
    },
    /// Change Neovim's global working directory (delegated to `:cd`).
    Chdir {
        /// The directory to change to.
        path: PathBuf,
    },
    /// Close the current split, i.e. the pane (delegated to `:close`).
    ClosePane,
    /// Open a new empty buffer in the current window (delegated to `:enew`).
    NewBuffer,
}

/// Translates a [`KeyEvent`] into Neovim keystroke notation.
///
/// Modifier order is `C-`, `S-`, `A-`, `D-` (Ctrl, Shift, Alt, Meta). Special
/// keys map to their angle-bracket names (`<CR>`, `<Esc>`, `<Tab>`, `<BS>`,
/// arrows, etc.). A bare printable character with no modifiers maps to itself,
/// except `<` which becomes `<lt>` (since `<` is special in nvim notation).
///
/// Shift contract: for a [`Key::Char`], deliver the shifted glyph in the char
/// and leave `shift = false`; the `S-` prefix is NOT emitted for printable
/// characters (it is not meaningful — Neovim expects the already-shifted glyph).
/// The `S-` prefix is only applied to named keys (`<S-Tab>`, `<S-F1>`,
/// `<S-Left>`, ...), where it is meaningful.
#[must_use]
pub fn key_to_notation(ev: &KeyEvent) -> String {
    let base = key_base(ev.key);
    let is_char = matches!(ev.key, Key::Char(_));
    // `S-` is only meaningful for named keys; for a printable char the shift
    // state is assumed to be already reflected in the char itself.
    let shift = ev.shift && !is_char;
    let has_mods = ev.ctrl || shift || ev.alt || ev.meta;

    // A printable char with no *effective* modifiers: emit it directly (with `<`
    // escaped). Shift alone on a char is not an effective modifier here.
    if !has_mods {
        if let Key::Char(c) = ev.key {
            if c == '<' {
                return "<lt>".to_string();
            }
            return c.to_string();
        }
    }

    let mut inner = String::new();
    if ev.ctrl {
        inner.push_str("C-");
    }
    if shift {
        inner.push_str("S-");
    }
    if ev.alt {
        inner.push_str("A-");
    }
    if ev.meta {
        inner.push_str("D-");
    }
    inner.push_str(&base);
    format!("<{inner}>")
}

/// Applies a batch of input events in order, fusing a run of consecutive
/// [`InputEvent::Key`] events into a single `nvim_input` call.
///
/// Fusing matters for determinism: a burst of keystrokes delivered as one
/// `nvim_input` produces a single redraw `flush`, so a caller that sends several
/// keys and then waits for one flush observes the settled frame. Non-key events
/// (mouse, scroll, resize, open/save) are applied individually, in order. Errors
/// are logged, not propagated, matching the fire-and-forget input contract.
pub(crate) async fn apply_batch(nvim: &Nvim, events: &[InputEvent]) {
    let mut pending_keys = String::new();

    async fn flush_keys(nvim: &Nvim, keys: &mut String) {
        if keys.is_empty() {
            return;
        }
        let notation = std::mem::take(keys);
        if let Err(e) = nvim_input(nvim, notation).await {
            tracing::warn!("nvim_input failed: {e}");
        }
    }

    for event in events {
        match event {
            InputEvent::Key(ev) => pending_keys.push_str(&key_to_notation(ev)),
            other => {
                flush_keys(nvim, &mut pending_keys).await;
                if let Err(e) = apply(nvim, other).await {
                    tracing::warn!("input apply failed: {e}");
                }
            }
        }
    }
    flush_keys(nvim, &mut pending_keys).await;
}

/// Applies an input event by issuing the appropriate generic RPC to Neovim.
///
/// All calls go through untyped RPC (`Neovim::call`) so we are insulated from
/// nvim-rs's older typed wrappers. Resize is forwarded via `nvim_ui_try_resize`;
/// the authoritative grid size is whatever the resulting `grid_resize` carries,
/// so we never resize the local grid here.
pub(crate) async fn apply(nvim: &Nvim, event: &InputEvent) -> Result<(), Box<CallError>> {
    match event {
        // Key translation lives in exactly one place — `nvim_input` — shared with
        // the batched live input path (`apply_batch`/`flush_keys`).
        InputEvent::Key(ev) => nvim_input(nvim, key_to_notation(ev)).await,
        InputEvent::Mouse(ev) => apply_mouse(nvim, ev).await,
        InputEvent::Scroll(ev) => apply_scroll(nvim, ev).await,
        InputEvent::Resize { cols, rows } => {
            let _ = nvim
                .call(
                    "nvim_ui_try_resize",
                    vec![Value::from(*cols), Value::from(*rows)],
                )
                .await?;
            Ok(())
        }
        InputEvent::ResizeGrid { grid, cols, rows } => {
            let _ = nvim
                .call(
                    "nvim_ui_try_resize_grid",
                    vec![Value::from(*grid), Value::from(*cols), Value::from(*rows)],
                )
                .await?;
            Ok(())
        }
        InputEvent::FocusWin { win } => {
            // nvim_set_current_win takes a Window handle. nvim-rs accepts the
            // plain i64 over generic RPC (confirmed by the focus_win test); the
            // matching id form is what the redraw stream decodes window handles
            // into (see `crate::redraw::ext_handle`).
            let _ = nvim
                .call("nvim_set_current_win", vec![Value::from(*win)])
                .await?;
            Ok(())
        }
        InputEvent::Open { path } => {
            command(nvim, &format!("edit {}", fnameescape(path))).await
        }
        InputEvent::Save => command(nvim, "write").await,
        InputEvent::SaveAs { path } => {
            command(nvim, &format!("saveas {}", fnameescape(path))).await
        }
        InputEvent::Chdir { path } => command(nvim, &format!("cd {}", fnameescape(path))).await,
        InputEvent::ClosePane => command(nvim, "close").await,
        InputEvent::NewBuffer => command(nvim, "enew").await,
    }
}

/// Escapes a path for use as an Ex-command argument, mirroring Vim's
/// `fnameescape`: backslash-escape characters that are special on the command
/// line (whitespace, `%`, `#`, `|`, `"`, backslash, etc.).
fn fnameescape(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            ' ' | '\t' | '\\' | '%' | '#' | '|' | '"' | '\'' | '*' | '?' | '[' | '{' | '<' | '>'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Sends already-translated key notation to Neovim via `nvim_input`.
///
/// This is the single key-delivery point shared by both the per-event
/// [`apply`] path and the batched [`apply_batch`] live-input pump, so key
/// translation is never duplicated.
async fn nvim_input(nvim: &Nvim, notation: String) -> Result<(), Box<CallError>> {
    let _ = nvim.call("nvim_input", vec![Value::from(notation)]).await?;
    Ok(())
}

async fn command(nvim: &Nvim, cmd: &str) -> Result<(), Box<CallError>> {
    let _ = nvim.call("nvim_command", vec![Value::from(cmd)]).await?;
    Ok(())
}

async fn apply_mouse(nvim: &Nvim, ev: &MouseEvent) -> Result<(), Box<CallError>> {
    let button = match ev.button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    };
    let action = match ev.action {
        MouseAction::Press => "press",
        MouseAction::Release => "release",
        MouseAction::Drag => "drag",
    };
    let modifier = mouse_modifier(ev.ctrl, ev.shift, ev.alt);
    nvim_input_mouse(nvim, button, action, &modifier, ev.grid, ev.row, ev.col).await
}

async fn apply_scroll(nvim: &Nvim, ev: &ScrollEvent) -> Result<(), Box<CallError>> {
    let action = match ev.direction {
        ScrollDirection::Up => "up",
        ScrollDirection::Down => "down",
        ScrollDirection::Left => "left",
        ScrollDirection::Right => "right",
    };
    nvim_input_mouse(nvim, "wheel", action, "", ev.grid, ev.row, ev.col).await
}

async fn nvim_input_mouse(
    nvim: &Nvim,
    button: &str,
    action: &str,
    modifier: &str,
    grid: u64,
    row: u32,
    col: u32,
) -> Result<(), Box<CallError>> {
    // nvim_input_mouse(button, action, modifier, grid, row, col). `grid` targets a
    // specific UI grid: 0 is the global/default grid, while under `ext_multigrid` a
    // window grid (`>= 2`) targets that pane with GRID-LOCAL row/col.
    let _ = nvim
        .call(
            "nvim_input_mouse",
            vec![
                Value::from(button),
                Value::from(action),
                Value::from(modifier),
                Value::from(grid),
                Value::from(u64::from(row)),
                Value::from(u64::from(col)),
            ],
        )
        .await?;
    Ok(())
}

fn mouse_modifier(ctrl: bool, shift: bool, alt: bool) -> String {
    let mut m = String::new();
    if ctrl {
        m.push_str("C-");
    }
    if shift {
        m.push_str("S-");
    }
    if alt {
        m.push_str("A-");
    }
    m
}

fn key_base(key: Key) -> String {
    match key {
        Key::Char(c) => {
            if c == '<' {
                "lt".to_string()
            } else {
                c.to_string()
            }
        }
        Key::Enter => "CR".to_string(),
        Key::Esc => "Esc".to_string(),
        Key::Tab => "Tab".to_string(),
        Key::Backspace => "BS".to_string(),
        Key::Delete => "Del".to_string(),
        Key::Left => "Left".to_string(),
        Key::Right => "Right".to_string(),
        Key::Up => "Up".to_string(),
        Key::Down => "Down".to_string(),
        Key::Home => "Home".to_string(),
        Key::End => "End".to_string(),
        Key::PageUp => "PageUp".to_string(),
        Key::PageDown => "PageDown".to_string(),
        Key::F(n) => format!("F{n}"),
    }
}
