//! The `wolf-gnome` ACTION NAMESPACE, as pure data.
//!
//! One table ([`ACTION_TABLE`]) is the single source of truth for every
//! `gio::SimpleAction` name, its GTK accelerator, the [`wolf_core::Command`] it
//! corresponds to, and the one-line justification for the chord it claims under
//! `crates/wolf-gnome/DECISIONS.md`'s scheme. A second table
//! ([`MENU_SECTIONS`]) declares the GNOME primary menu in terms of those same
//! action names. Because the menu, the accelerators and the command dispatch all
//! read the same data, they cannot drift apart.
//!
//! This module contains no GTK WIDGET types on purpose: that is what lets the
//! whole namespace be unit-tested headlessly, the same way `layout.rs` /
//! `geometry.rs` keep their decisions pure. (The two GDK value types
//! [`action_for_key`] needs are the sole exception; see "Why chord matching also
//! lives here" below.) The GTK glue layer turns this data into
//! `gio::SimpleAction`s, a `gio::Menu`, and `set_accels_for_action` calls.
//!
//! ## The invariant these tables exist to enforce
//!
//! > **No Neovim key becomes unreachable.**
//!
//! `wolf-gnome`'s own encoder forces `shift: false` on printable keys
//! (`input::gdk_key`) and emits `S-` only for non-`Char` keys
//! (`wolf_core::key_to_notation`), so `Ctrl+Shift+P` encodes to `<C-P>`, which
//! `nvim_replace_termcodes` makes byte-identical to `<C-p>`. Binding
//! `Ctrl+Shift+<letter>` therefore gives up only the terminal-style Ctrl+Shift
//! ALIAS for `<C-letter>` — the key itself is still typed as plain `Ctrl+<letter>`,
//! which Wolf never binds. What is never given up: a bare `Ctrl+<letter>`
//! accelerator, any `Super` chord, and any `Alt+<letter>` chord.
//!
//! [`parse_accel`] + [`forbidden_reason`] enforce that invariant MECHANICALLY over
//! [`ACTION_TABLE`], so a future accelerator cannot quietly shadow a Neovim key.
//! See `crates/wolf-gnome/DECISIONS.md`, "The accelerator namespace", for the
//! reasoning behind the scheme.
//!
//! ## Why the accel grammar is re-implemented here
//!
//! [`parse_accel`] is a deliberate re-implementation of the SUBSET of
//! `gtk_accelerator_parse` this crate allows. `gtk::accelerator_parse` cannot be
//! called from a headless unit test (gtk4-rs asserts an initialized main thread),
//! and a checker that cannot run in the unit-test tier is a checker nobody runs.
//! The accepted grammar is zero or more `<Control>` / `<Shift>` / `<Alt>` tokens
//! followed by a single key NAME (`p`, `o`, `0`, `equal`, `minus`, `grave`,
//! `Left`, `Right`). Anything else — including `<Super>`, `<Primary>`, `<Meta>`,
//! `<Hyper>`, a bare symbol such as `-`, or a malformed token — is rejected with
//! `None`, which is why those tokens never reach [`forbidden_reason`].
//!
//! ## Why chord matching also lives here
//!
//! [`action_for_key`] resolves a live GDK keyval + modifier state to the action a
//! chord activates. It exists because GTK's OWN accelerator matching does not fire
//! under the cage/`wtype` smoke harness: `gdk_key_event_matches` is keymap-aware
//! (it re-translates the keycode through the display's current keymap), and
//! `wtype` synthesizes a fresh one-keycode keymap per keystroke, so a
//! `Ctrl+Shift+<letter>` event fails to match even the trigger built from its OWN
//! keyval and state. `action_for_key` compares the delivered keyval and modifiers
//! DIRECTLY against [`ACTION_TABLE`], which is keymap-independent. See
//! [`crate::shell`] for the capture-phase controller that calls it.
//!
//! That is the one place this module touches the GDK types `gdk::Key` and
//! `gdk::ModifierType`. They are plain value types — no widget, no display, no
//! initialized main thread — so they cost nothing in testability (the crate's
//! `gdk_key` seam and its tests are built on exactly the same two types).

use wolf_core::Command;

/// Whether an [`ActionBinding`]'s accelerator is registered THIS unit, merely
/// reserved for a later one, or absent because the action is menu-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingStatus {
    /// The accelerator is registered with `set_accels_for_action` now.
    Bound,
    /// The chord is spoken for by a later unit; it is NOT registered yet, and the
    /// action does NOT appear in the menu (the menu never shows a no-op).
    Reserved,
    /// No chord at all — the action is reachable only from the primary menu, a
    /// button, or a drained command.
    MenuOnly,
}

/// One row of the action namespace: an action name, its optional accelerator, the
/// [`wolf_core::Command`] it corresponds to (if any), its binding status, and the
/// one-line justification for the chord it claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionBinding {
    /// The `gio` action name, e.g. `"win.go-to-file"` or `"app.quit"`. The `win.`
    /// prefix scopes the action to the window (so the deferred multi-window row
    /// needs no rework); `app.` is used only for application-wide actions.
    pub action: &'static str,
    /// The GTK accelerator string, or `None` for a menu-only action.
    pub accel: Option<&'static str>,
    /// The registry command this action corresponds to, or `None` for a
    /// front-end-only action.
    pub command: Option<Command>,
    /// Whether the accelerator is registered now, reserved, or absent.
    pub status: BindingStatus,
    /// The justification for this row's chord, one line.
    pub note: &'static str,
}

/// The COMPLETE Linux accelerator scheme of `crates/wolf-gnome/DECISIONS.md`,
/// "The accelerator namespace": one row per action, covering later units'
/// reserved chords as well as this unit's bound ones.
///
/// Reserved rows are present so the namespace is auditable in one place and so
/// the `no_accel_makes_a_neovim_key_unreachable` unit test checks them too — a
/// chord that would shadow a Neovim key must fail the tests when it is RESERVED,
/// not later when it is finally bound.
pub const ACTION_TABLE: &[ActionBinding] = &[
    // --- File-ish actions (macOS File menu) ---------------------------------
    ActionBinding {
        action: "win.go-to-file",
        accel: Some("<Control><Shift>o"),
        command: Some(Command::GoToFile),
        status: BindingStatus::Bound,
        note: "macOS Cmd+O; gives up only the Ctrl+Shift alias for <C-o>, which is still typed as \
               plain Ctrl+O",
    },
    ActionBinding {
        action: "win.open-folder",
        accel: None,
        command: Some(Command::OpenFolder),
        status: BindingStatus::MenuOnly,
        note: "macOS Shift+Cmd+O; reached from the primary menu and the empty state's button, so \
               no chord is spent on it",
    },
    ActionBinding {
        action: "win.open-file",
        accel: None,
        command: Some(Command::OpenFile),
        status: BindingStatus::MenuOnly,
        note: "no macOS accelerator either (NSOpenPanel is menu-only there); reached from the \
               primary menu",
    },
    ActionBinding {
        action: "win.new-buffer",
        accel: Some("<Control><Shift>n"),
        command: Some(Command::NewBuffer),
        status: BindingStatus::Bound,
        note: "macOS Cmd+N; gives up only the Ctrl+Shift alias for <C-n>, which is still typed as \
               plain Ctrl+N",
    },
    ActionBinding {
        action: "app.new-window",
        accel: None,
        command: Some(Command::NewWindow),
        status: BindingStatus::MenuOnly,
        note: "macOS Shift+Cmd+N; deferred because wolf-gnome is single-window, so it is neither \
               bound nor listed in the menu yet",
    },
    ActionBinding {
        action: "win.new-tab",
        accel: Some("<Control><Shift>t"),
        command: Some(Command::NewTab),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+T; reserved for GTK native tabs, and gives up only the Ctrl+Shift alias \
               for <C-t>",
    },
    ActionBinding {
        action: "win.close-pane",
        accel: Some("<Control><Shift>w"),
        command: Some(Command::ClosePane),
        status: BindingStatus::Bound,
        note: "macOS Cmd+W; gives up only the Ctrl+Shift alias for <C-w>, the window-command \
               prefix, which is still typed as plain Ctrl+W",
    },
    ActionBinding {
        action: "win.close-window",
        accel: None,
        command: Some(Command::CloseWindow),
        status: BindingStatus::MenuOnly,
        note: "macOS Shift+Cmd+W; the GNOME shell already closes a window, so this is menu-only",
    },
    ActionBinding {
        action: "win.minimize-window",
        accel: None,
        command: Some(Command::MinimizeWindow),
        status: BindingStatus::MenuOnly,
        note: "macOS Cmd+M; the GNOME shell owns minimize, so the command is dispatched but no \
               chord is bound",
    },
    ActionBinding {
        action: "app.quit",
        accel: Some("<Control><Shift>q"),
        command: Some(Command::QuitApplication),
        status: BindingStatus::Bound,
        note: "macOS Cmd+Q; gives up only the Ctrl+Shift alias for <C-q> (= CTRL-V in \
               insert/cmdline), which is still typed as plain Ctrl+Q",
    },
    // --- The command-palette overlay -----------------------------------------
    ActionBinding {
        action: "win.command-palette",
        accel: Some("<Control><Shift>p"),
        command: Some(Command::CommandPalette),
        status: BindingStatus::Bound,
        note: "macOS Cmd+P; the strongest Linux idiom for this chord, and gives up only the \
               Ctrl+Shift alias for <C-p>",
    },
    // --- The buffer-picker overlay -------------------------------------------
    ActionBinding {
        action: "win.buffer-picker",
        accel: Some("<Control><Shift>e"),
        command: Some(Command::BufferPicker),
        status: BindingStatus::Bound,
        note: "macOS Cmd+E; gives up only the Ctrl+Shift alias for <C-e> (scroll down one line), \
               which is still typed as plain Ctrl+E",
    },
    // --- The project-search overlay ------------------------------------------
    ActionBinding {
        action: "win.search-in-project",
        accel: Some("<Control><Shift>f"),
        command: Some(Command::SearchInProject),
        status: BindingStatus::Bound,
        note: "macOS Shift+Cmd+F; gives up only the Ctrl+Shift alias for <C-f> (page forward), \
               which is still typed as plain Ctrl+F",
    },
    // --- The system clipboard -------------------------------------------------
    ActionBinding {
        action: "win.copy",
        accel: Some("<Control><Shift>c"),
        command: Some(Command::Copy),
        status: BindingStatus::Bound,
        note: "macOS Cmd+C; the GNOME Terminal spelling, and gives up only the Ctrl+Shift alias \
               for <C-c>, the INTERRUPT, which is still typed as plain Ctrl+C",
    },
    ActionBinding {
        action: "win.paste",
        accel: Some("<Control><Shift>v"),
        command: Some(Command::Paste),
        status: BindingStatus::Bound,
        note: "macOS Cmd+V; the GNOME Terminal spelling, and gives up only the Ctrl+Shift alias \
               for <C-v>, visual BLOCK, which is still typed as plain Ctrl+V",
    },
    // --- The file-tree sidebar ------------------------------------------------
    ActionBinding {
        action: "win.toggle-sidebar",
        accel: Some("<Control><Shift>b"),
        command: Some(Command::ToggleSidebar),
        status: BindingStatus::Bound,
        note: "macOS Ctrl+Cmd+S; B is the cross-platform sidebar letter and gives up only the \
               Ctrl+Shift alias for <C-b>",
    },
    // --- Reserved for later units (declared here so the scheme is auditable) --
    ActionBinding {
        action: "win.sidebar-show-project",
        accel: Some("<Control><Shift>s"),
        command: Some(Command::SidebarShowProject),
        status: BindingStatus::Reserved,
        note: "macOS Shift+Cmd+E; gives up only the Ctrl+Shift alias for <C-s> (LSP signature \
               help in i/v/s)",
    },
    ActionBinding {
        action: "win.increase-font-size",
        accel: Some("<Control>equal"),
        command: Some(Command::IncreaseFontSize),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+=; <C-=> has no built-in Neovim meaning, so the idiomatic bare-Ctrl \
               chord is safe here",
    },
    ActionBinding {
        action: "win.decrease-font-size",
        accel: Some("<Control>minus"),
        command: Some(Command::DecreaseFontSize),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+-; <C--> has no built-in Neovim meaning, so the idiomatic bare-Ctrl \
               chord is safe here",
    },
    ActionBinding {
        action: "win.reset-font-size",
        accel: Some("<Control>0"),
        command: Some(Command::ResetFontSize),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+0; assumed free because <C-0> has no index.txt entry, which is an \
               assumption and not a verified fact",
    },
    ActionBinding {
        action: "win.terminal",
        accel: Some("<Control>grave"),
        command: Some(Command::Terminal),
        status: BindingStatus::Reserved,
        note: "macOS Ctrl+`; assumed free because <C-`> has no index.txt entry, which is an \
               assumption and not a verified fact",
    },
    ActionBinding {
        action: "win.history-back",
        accel: Some("<Alt>Left"),
        command: None,
        status: BindingStatus::Bound,
        note: "macOS Cmd+[; front-end-only navigation with no Command variant, and capturing \
               <A-Left> PREVENTS the destructive <Esc>+<Left> split",
    },
    ActionBinding {
        action: "win.history-forward",
        accel: Some("<Alt>Right"),
        command: None,
        status: BindingStatus::Bound,
        note: "macOS Cmd+]; front-end-only navigation with no Command variant, and capturing \
               <A-Right> PREVENTS the destructive <Esc>+<Right> split",
    },
    ActionBinding {
        action: "win.about",
        accel: None,
        command: None,
        status: BindingStatus::MenuOnly,
        note: "no macOS counterpart in the registry; About is the one surface on this front-end \
               that carries the bundled fonts' licence notices, and it is menu-only so it spends \
               no chord",
    },
];

/// One item of the GNOME primary menu: a label and the action it activates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItemSpec {
    /// The menu label. Where the item's action maps to a [`wolf_core::Command`],
    /// this MUST equal that command's `CommandSpec::title` so a registry rename
    /// cannot silently desync the GTK menu.
    pub label: &'static str,
    /// The `gio` action name this item activates; must be a row of
    /// [`ACTION_TABLE`].
    pub action: &'static str,
}

/// One separated section of the GNOME primary menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuSection {
    /// The items of this section, in display order.
    pub items: &'static [MenuItemSpec],
}

/// The GNOME primary menu, as sections of items.
///
/// Reserved actions never appear here: the menu must not show an item that does
/// nothing.
pub const MENU_SECTIONS: &[MenuSection] = &[
    MenuSection {
        items: &[
            MenuItemSpec {
                label: "New",
                action: "win.new-buffer",
            },
            MenuItemSpec {
                label: "Open Folder…",
                action: "win.open-folder",
            },
            MenuItemSpec {
                label: "Open File…",
                action: "win.open-file",
            },
            MenuItemSpec {
                label: "Go to File…",
                action: "win.go-to-file",
            },
            MenuItemSpec {
                label: "Command Palette…",
                action: "win.command-palette",
            },
            MenuItemSpec {
                label: "Buffer Picker…",
                action: "win.buffer-picker",
            },
            MenuItemSpec {
                label: "Search in project",
                action: "win.search-in-project",
            },
        ],
    },
    MenuSection {
        items: &[
            MenuItemSpec {
                label: "Back",
                action: "win.history-back",
            },
            MenuItemSpec {
                label: "Forward",
                action: "win.history-forward",
            },
            MenuItemSpec {
                label: "Toggle sidebar",
                action: "win.toggle-sidebar",
            },
        ],
    },
    MenuSection {
        items: &[
            MenuItemSpec {
                label: "Copy",
                action: "win.copy",
            },
            MenuItemSpec {
                label: "Paste",
                action: "win.paste",
            },
        ],
    },
    MenuSection {
        items: &[
            MenuItemSpec {
                label: "Close",
                action: "win.close-pane",
            },
            MenuItemSpec {
                label: "Close Window",
                action: "win.close-window",
            },
        ],
    },
    MenuSection {
        items: &[
            MenuItemSpec {
                label: "About Wolf",
                action: "win.about",
            },
            MenuItemSpec {
                label: "Quit Wolf",
                action: "app.quit",
            },
        ],
    },
];

/// A parsed accelerator: which of the three accepted modifiers are held, plus the
/// key NAME.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedAccel {
    /// Whether `<Control>` is part of the chord.
    pub ctrl: bool,
    /// Whether `<Shift>` is part of the chord.
    pub shift: bool,
    /// Whether `<Alt>` is part of the chord.
    pub alt: bool,
    /// The key NAME (never a symbol): `p`, `0`, `equal`, `minus`, `grave`,
    /// `Left`, `Right`, …
    pub key: &'static str,
}

/// Looks up the [`ACTION_TABLE`] row for `action`, or `None` if the action is not
/// declared.
#[must_use]
pub fn binding_for_action(action: &str) -> Option<&'static ActionBinding> {
    ACTION_TABLE.iter().find(|binding| binding.action == action)
}

/// Returns the action name that dispatches `cmd`, or `None` if no action does.
#[must_use]
pub fn action_for_command(cmd: Command) -> Option<&'static str> {
    ACTION_TABLE
        .iter()
        .find(|binding| binding.command == Some(cmd))
        .map(|binding| binding.action)
}

/// Returns the [`wolf_core::Command`] `action` corresponds to, or `None` for a
/// front-end-only or undeclared action.
#[must_use]
pub fn command_for_action(action: &str) -> Option<Command> {
    binding_for_action(action).and_then(|binding| binding.command)
}

/// Iterates the rows whose accelerator is registered THIS unit
/// ([`BindingStatus::Bound`]) — exactly the rows the GTK glue passes to
/// `set_accels_for_action`.
#[must_use = "iterators are lazy: nothing is registered until this one is consumed"]
pub fn bound_bindings() -> impl Iterator<Item = &'static ActionBinding> {
    ACTION_TABLE
        .iter()
        .filter(|binding| binding.status == BindingStatus::Bound)
}

/// Resolves a live GDK keyval + modifier state to the action its chord activates,
/// or `None` when the chord is not one Wolf binds.
///
/// Only [`BindingStatus::Bound`] rows are considered: a Reserved row's action is
/// not installed this unit, so matching it would swallow the key and do nothing.
///
/// The modifier comparison is EXACT on the three modifiers Wolf's accel grammar
/// knows (Ctrl / Shift / Alt) — an accelerator that does not name Shift must not
/// fire when Shift is held — and every other modifier bit (Super, Lock, the mouse
/// buttons) is ignored, since none of them can appear in a parsed accelerator.
/// The key comparison is case-insensitive on letters, because whether a shifted
/// letter arrives as `o` or `O` depends on the keymap (see the module docs).
#[must_use]
pub fn action_for_key(keyval: gdk::Key, state: gdk::ModifierType) -> Option<&'static str> {
    let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
    let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
    let alt = state.contains(gdk::ModifierType::ALT_MASK);

    bound_bindings()
        .filter_map(|binding| Some((binding, parse_accel(binding.accel?)?)))
        .find(|(_, accel)| {
            accel.ctrl == ctrl
                && accel.shift == shift
                && accel.alt == alt
                && key_name_matches(accel.key, keyval)
        })
        .map(|(binding, _)| binding.action)
}

/// Whether the GTK key NAME `name` denotes `keyval`.
///
/// Covers exactly the names [`ACTION_TABLE`] uses: the named arrow keys, the three
/// punctuation names GTK spells out, and any single ASCII alphanumeric (compared
/// case-insensitively). Anything else is `false` rather than a guess — a chord
/// that silently matched the wrong key would be worse than one that never fires.
fn key_name_matches(name: &str, keyval: gdk::Key) -> bool {
    let symbol = match name {
        "Left" => return keyval == gdk::Key::Left,
        "Right" => return keyval == gdk::Key::Right,
        "equal" => '=',
        "minus" => '-',
        "grave" => '`',
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => c,
                _ => return false,
            }
        }
    };
    keyval
        .to_unicode()
        .is_some_and(|c| c.eq_ignore_ascii_case(&symbol))
}

/// Parses `spec` under the accepted SUBSET of GTK's accelerator grammar (see the
/// module docs), returning `None` for anything outside it.
///
/// Rejected with `None`: an unknown modifier token (`<Super>`, `<Primary>`,
/// `<Meta>`, `<Hyper>`, …), a malformed or unterminated token, a missing key, and
/// more than one key.
#[must_use]
pub fn parse_accel(spec: &'static str) -> Option<ParsedAccel> {
    let mut rest = spec;
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;

    // Leading `<Modifier>` tokens, each accepted at most once.
    while let Some(after_open) = rest.strip_prefix('<') {
        let (token, tail) = after_open.split_once('>')?;
        let seen = match token {
            "Control" => &mut ctrl,
            "Shift" => &mut shift,
            "Alt" => &mut alt,
            // `<Super>` / `<Primary>` / `<Meta>` / `<Hyper>` and anything malformed
            // are deliberately outside the accepted subset.
            _ => return None,
        };
        if *seen {
            return None;
        }
        *seen = true;
        rest = tail;
    }

    // Exactly one key NAME: ASCII letters and digits only. That rejects a bare
    // symbol (`-` instead of `minus`), an empty key, and a second `<…>` token
    // appearing after the key.
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }

    Some(ParsedAccel {
        ctrl,
        shift,
        alt,
        key: rest,
    })
}

/// Returns why `p` would violate the "no Neovim key becomes unreachable"
/// invariant, or `None` if the chord is safe to bind.
///
/// Rejects a bare `Ctrl+<single letter>` (that WOULD make `<C-letter>`
/// unreachable) and `Alt` combined with a single-letter key (GTK's mnemonic
/// namespace; forwarded, an unmapped Alt chord splits into `<Esc>` + the base
/// key). `Alt` with a NAMED key (`Left` / `Right`) is allowed — capturing it is
/// what PREVENTS that destructive split. `Super` / `Primary` never reach here:
/// [`parse_accel`] rejects those tokens outright.
#[must_use]
pub fn forbidden_reason(p: &ParsedAccel) -> Option<&'static str> {
    let single_letter = p.key.len() == 1 && p.key.chars().all(|c| c.is_ascii_alphabetic());
    if !single_letter {
        return None;
    }
    if p.ctrl && !p.shift && !p.alt {
        return Some(
            "a bare Ctrl+<letter> accelerator would make <C-letter> UNREACHABLE: Wolf matches its \
             chords in the capture phase, before the window key controller that forwards keys to \
             Neovim ever sees the event",
        );
    }
    if p.alt {
        return Some(
            "Alt+<letter> is GTK's mnemonic namespace, and an unmapped Alt chord forwarded to \
             Neovim splits into <Esc> + the base key (insert-mode <A-p> pastes the unnamed \
             register)",
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use gdk::glib::translate::from_glib;

    /// The only key NAMES allowed to carry uppercase letters: GTK's named
    /// non-printable keys. Everything else must be a lowercase/digit key name
    /// (`p`, `o`, `0`, `equal`, `minus`, `grave`), never a symbol.
    const NAMED_KEYS: [&str; 2] = ["Left", "Right"];

    /// The actions whose accelerators are actually registered — the
    /// [`BindingStatus::Bound`] rows of [`ACTION_TABLE`] — sorted.
    const EXPECTED_BOUND_ACTIONS: [&str; 12] = [
        "app.quit",
        "win.buffer-picker",
        "win.close-pane",
        "win.command-palette",
        "win.copy",
        "win.go-to-file",
        "win.history-back",
        "win.history-forward",
        "win.new-buffer",
        "win.paste",
        "win.search-in-project",
        "win.toggle-sidebar",
    ];

    /// Whether a chord spelling is ACCEPTABLE for Wolf to bind: it must parse
    /// under the accepted subset AND carry no forbidden reason.
    ///
    /// Both halves count as a rejection on purpose. `<Super>n` and `<Primary>p`
    /// are outside `parse_accel`'s grammar (so they are rejected there), while
    /// `<Control>p` and `<Alt>p` parse fine and are rejected by
    /// `forbidden_reason`. Which mechanism bites is an implementation detail; that
    /// the chord is refused is the contract.
    fn accel_is_accepted(spec: &'static str) -> bool {
        let Some(parsed) = parse_accel(spec) else {
            return false;
        };
        forbidden_reason(&parsed).is_none()
    }

    /// Whether `action` matches `^(app|win)\.[a-z0-9]+(-[a-z0-9]+)*$`, spelled out
    /// by hand because this crate has no regex dependency.
    fn action_name_is_valid(action: &str) -> bool {
        let Some((namespace, rest)) = action.split_once('.') else {
            return false;
        };
        if namespace != "app" && namespace != "win" {
            return false;
        }
        if rest.is_empty() {
            return false;
        }
        for segment in rest.split('-') {
            if !is_lowercase_name(segment) {
                return false;
            }
        }
        true
    }

    /// Whether `name` is a non-empty run of lowercase ASCII letters and digits —
    /// the shape both a GTK key name (`p`, `o`, `0`, `equal`) and an action-name
    /// segment (`go`, `to`, `file`) must have.
    fn is_lowercase_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        for c in name.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
                return false;
            }
        }
        true
    }

    /// Whether `key` is a single punctuation character — the `<Control>-` spelling
    /// GTK does not understand, as opposed to `<Control>minus`.
    fn is_single_symbol(key: &str) -> bool {
        let mut chars = key.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        chars.next().is_none() && first.is_ascii_punctuation()
    }

    /// Builds the `gdk::Key` a real keypress of `c` delivers.
    ///
    /// gdk4-rs has no `Key::from_unicode`, so the keyval is constructed exactly as
    /// GDK would from a Unicode scalar — the same helper `tests/gdk_key.rs` and
    /// `tests/roundtrip_nvim.rs` use. Building the keyval from the GLYPH rather
    /// than from a `gdk::Key` constant is what keeps these cases keyboard-layout
    /// independent, AND what lets a case spell the SHIFTED form (`'O'`) that a real
    /// keymap actually delivers for Ctrl+Shift+O.
    fn key_from_char(c: char) -> gdk::Key {
        unsafe { from_glib(gdk::unicode_to_keyval(c as u32)) }
    }

    /// Every declared accelerator parses under the accepted subset.
    ///
    /// The non-vacuity assertion matters: with an empty table every "for each row"
    /// check below would pass while proving nothing.
    #[test]
    fn every_accel_parses() {
        let with_accel = ACTION_TABLE.iter().filter(|b| b.accel.is_some()).count();
        assert!(
            with_accel > 0,
            "ACTION_TABLE declares no accelerators at all, so every per-accel test in this module \
             would pass vacuously. Populate it from the scheme in \
             `crates/wolf-gnome/DECISIONS.md`, \"The accelerator namespace\" (plan section 2.4)."
        );

        for binding in ACTION_TABLE {
            let Some(accel) = binding.accel else {
                continue;
            };
            assert!(
                parse_accel(accel).is_some(),
                "accel {accel:?} for action {:?} does not parse under the accepted subset of the \
                 GTK accelerator grammar. Allowed: zero or more <Control>/<Shift>/<Alt> tokens \
                 followed by ONE key NAME. `<Super>`/`<Primary>` are deliberately outside the \
                 grammar (see the module docs).",
                binding.action
            );
        }
    }

    /// Key parts are GTK key NAMES, never the symbol the key produces.
    ///
    /// This is the `<Control>-` vs `<Control>minus` trap: `gtk_accelerator_parse`
    /// wants `minus` / `equal` / `grave`, and a symbol spelling silently fails to
    /// match anything at runtime — a bug no smoke test can see, because the chord
    /// simply never fires.
    #[test]
    fn accels_use_key_names_not_symbols() {
        for binding in ACTION_TABLE {
            let Some(accel) = binding.accel else {
                continue;
            };
            let parsed = parse_accel(accel).unwrap_or_else(|| {
                panic!(
                    "accel {accel:?} for action {:?} does not parse; see `every_accel_parses`",
                    binding.action
                )
            });
            let key = parsed.key;

            assert!(
                !key.is_empty(),
                "accel {accel:?} for action {:?} parsed to an EMPTY key name",
                binding.action
            );
            assert!(
                !is_single_symbol(key),
                "accel {accel:?} for action {:?} uses the SYMBOL {key:?} as its key. GTK wants the \
                 key NAME (`minus`, `equal`, `grave`, …); a symbol spelling parses to nothing and \
                 the chord silently never fires.",
                binding.action
            );
            assert!(
                NAMED_KEYS.contains(&key) || is_lowercase_name(key),
                "accel {accel:?} for action {:?} has key {key:?}, which is neither an all-lowercase \
                 key name nor one of the known named keys {NAMED_KEYS:?}. `gtk_accelerator_parse` \
                 lowercases key names, so an uppercase spelling does NOT imply Shift — Shift must \
                 be spelled `<Shift>`.",
                binding.action
            );
        }
    }

    /// No declared accelerator makes a Neovim key unreachable — and the checker
    /// that says so actually bites.
    ///
    /// The negative cases are the point of the test. A `forbidden_reason` that
    /// never returns `Some` would satisfy the table half of this test forever
    /// while permitting exactly the bug the invariant exists to prevent.
    #[test]
    fn no_accel_makes_a_neovim_key_unreachable() {
        for binding in ACTION_TABLE {
            let Some(accel) = binding.accel else {
                continue;
            };
            let parsed = parse_accel(accel).unwrap_or_else(|| {
                panic!(
                    "accel {accel:?} for action {:?} does not parse; see `every_accel_parses`",
                    binding.action
                )
            });
            let reason = forbidden_reason(&parsed);
            assert!(
                reason.is_none(),
                "accel {accel:?} for action {:?} is FORBIDDEN: {}. A chord that shadows a Neovim \
                 key is a bug, not a tradeoff — see the invariant in the module docs and in \
                 `crates/wolf-gnome/DECISIONS.md`, \"The accelerator namespace\".",
                binding.action,
                reason.unwrap_or("<no reason given>")
            );
        }

        // --- The checker must REJECT these ---------------------------------
        assert!(
            !accel_is_accepted("<Control>p"),
            "a bare Ctrl+<letter> accelerator MUST be refused: binding it makes `<C-p>` \
             unreachable, because GTK captures application accelerators before the window key \
             controller ever sees them."
        );
        assert!(
            !accel_is_accepted("<Super>n"),
            "a Super chord MUST be refused: `input::gdk_key` maps SUPER_MASK to `meta` and \
             `key_to_notation` emits `<D-…>`, which unmapped nvim either strips (executing the \
             base key) or inserts as literal text."
        );
        assert!(
            !accel_is_accepted("<Primary>p"),
            "`<Primary>` MUST be refused: in GTK4 it is a plain synonym for `<Control>` (it does \
             NOT mean Command), so it reads as if it were safe while binding exactly the chord \
             `<Control>p` is refused for."
        );
        assert!(
            !accel_is_accepted("<Alt>p"),
            "an Alt+<letter> chord MUST be refused: it is GTK's mnemonic namespace, and an \
             unmapped Alt chord forwarded to nvim splits into `<Esc>` + the base key (destructive: \
             insert-mode `<A-p>` pastes the unnamed register)."
        );

        // --- ...but it must ACCEPT this ------------------------------------
        assert!(
            accel_is_accepted("<Alt>Left"),
            "`<Alt>Left` MUST be accepted: `<A-Left>` has no default nvim mapping, and CAPTURING \
             the chord is precisely what prevents the destructive `<Esc>`+`<Left>` split. A \
             checker that rejects everything is as useless as one that rejects nothing."
        );
    }

    /// No accelerator string is declared twice: two actions sharing a chord means
    /// one of them silently never fires.
    #[test]
    fn accels_are_unique() {
        let mut seen: HashSet<&'static str> = HashSet::new();
        for binding in ACTION_TABLE {
            let Some(accel) = binding.accel else {
                continue;
            };
            assert!(
                seen.insert(accel),
                "accel {accel:?} is declared twice in ACTION_TABLE (second occurrence on action \
                 {:?}). GTK resolves a duplicate chord to ONE action, so the other silently never \
                 fires.",
                binding.action
            );
        }
    }

    /// Every action name is a well-formed `gio` action name in the `app.` or
    /// `win.` namespace: `^(app|win)\.[a-z0-9]+(-[a-z0-9]+)*$`.
    ///
    /// `gio` rejects a malformed name at `add_action` time with a runtime warning,
    /// not a compile error, so this is the only cheap place to catch a typo.
    #[test]
    fn action_names_are_valid() {
        for binding in ACTION_TABLE {
            assert!(
                action_name_is_valid(binding.action),
                "action name {:?} is not a valid gio action name. It must match \
                 `^(app|win)\\.[a-z0-9]+(-[a-z0-9]+)*$`: an `app.` or `win.` prefix, then \
                 lowercase kebab-case.",
                binding.action
            );
        }
    }

    /// The accelerators actually REGISTERED are exactly
    /// [`EXPECTED_BOUND_ACTIONS`], and each of them really has a chord to register.
    #[test]
    fn bound_set_is_exactly_this_unit() {
        let mut bound: Vec<&'static str> = bound_bindings().map(|b| b.action).collect();
        bound.sort_unstable();

        let mut expected: Vec<&'static str> = EXPECTED_BOUND_ACTIONS.to_vec();
        expected.sort_unstable();

        assert_eq!(
            bound, expected,
            "the set of BOUND accelerators changed. wolf-gnome registers exactly twelve \
             (Ctrl+Shift+O go-to-file, Ctrl+Shift+N new-buffer, Ctrl+Shift+P command-palette, \
             Ctrl+Shift+E buffer-picker, Ctrl+Shift+F search-in-project, Ctrl+Shift+W close-pane, \
             Ctrl+Shift+Q quit, Alt+Left history-back, Alt+Right history-forward, Ctrl+Shift+B \
             toggle-sidebar, Ctrl+Shift+C copy, Ctrl+Shift+V paste); every other \
             row of the table is Reserved for a later unit or MenuOnly. Binding a reserved chord \
             early means shipping a menu/accelerator that does nothing — and DEMOTING one back to \
             Reserved silently unbinds a chord the menu still advertises. The two ALT rows are the \
             namespace's deliberate exception and demoting either is worse than unbinding an alias: \
             an uncaptured <Alt>Left reaches Neovim as <A-Left>, which unmapped splits into <Esc> \
             plus <Left>. Demoting win.copy or win.paste back to Reserved unbinds the chord AND \
             leaves `command_action::ui_action_for` dispatching a command nothing can reach from \
             the keyboard, which is why the two tests move together."
        );

        for binding in bound_bindings() {
            assert!(
                binding.accel.is_some(),
                "action {:?} is marked Bound but declares no accelerator; \
                 `set_accels_for_action` would have nothing to register.",
                binding.action
            );
        }
    }

    /// The LIVE chord matcher: a delivered GDK keyval + modifier state resolves to
    /// exactly the action that chord activates, and to nothing otherwise.
    ///
    /// This is the real matching path — `shell::AppShell::install_chord_controller`
    /// calls [`action_for_key`] from a capture-phase `EventControllerKey`, because
    /// GTK's own keymap-aware matching does not fire under `wtype` (see the module
    /// docs) — so every row below is a user-visible behaviour:
    ///
    /// * **The twelve Bound chords must fire, in BOTH letter cases where they have
    ///   one.** A real keymap delivers the SHIFTED glyph for Ctrl+Shift+O — keyval
    ///   `O`, not `o` (`input::gdk_key`'s shift contract, and what
    ///   `tests/gdk_key.rs` pins) — so a case-SENSITIVE comparison would mean the
    ///   chord never fires on real hardware while a lowercase-only test still
    ///   passed. The two ARROW chords (`<Alt>Left` / `<Alt>Right`) have no letter
    ///   case at all: they resolve through [`key_name_matches`]' named-key branch,
    ///   which compares `gdk::Key` constants directly, so they carry one row each.
    /// * **A bare `Ctrl+<letter>` must NOT fire.** This is the mechanical
    ///   enforcement of the invariant in the module docs: the matcher runs in the
    ///   CAPTURE phase, ahead of the key controller that forwards to Neovim and
    ///   returning `Propagation::Stop`, so a match here is a Neovim key made
    ///   UNREACHABLE. `<C-o>`, `<C-n>`, `<C-p>`, `<C-e>` (scroll down one line),
    ///   `<C-f>` (page forward), `<C-b>` (page backward), `<C-w>` (the
    ///   window-command prefix), `<C-q>`, `<C-c>` and `<C-v>` must all still reach
    ///   Neovim.
    ///
    ///   `<C-c>` and `<C-v>` are named separately because they are the two rows
    ///   where "it is only an alias" is not a good enough answer, and where the
    ///   argument has to come from the ENCODER rather than from the general rule.
    ///   `<C-c>` is Neovim's INTERRUPT — the key that aborts a running mapping, a
    ///   hung `:global` or a pending operator — and `<C-v>` is visual-BLOCK in
    ///   normal mode and the literal-next-character prefix in insert and cmdline
    ///   mode. Losing either would be losing a capability, not an alias. Neither
    ///   is lost: `input::gdk_key` reads the modifier out of `state` and never out
    ///   of the keyval, so plain Ctrl+C arrives as keyval `c` whose
    ///   `to_unicode()` is `'c'` (never the control character 0x03, which the
    ///   `is_control` rejection would drop), [`action_for_key`] compares all three
    ///   modifiers EXACTLY so `<Control><Shift>c` cannot match a state without
    ///   Shift, and the bubble-phase controller then encodes `Key::Char('c')` with
    ///   `ctrl: true` as `<C-c>`. The two rows below are what keep that true.
    /// * **A Reserved row must NOT fire.** Its action is not installed this unit,
    ///   so a match would swallow the key and do nothing at all — worse than not
    ///   binding it. `Ctrl+minus` (`win.decrease-font-size`) is the row carrying
    ///   that bucket.
    /// * **The modifier match is EXACT, not "at least".** Adding Alt to a bound
    ///   Ctrl+Shift chord must miss, and adding Ctrl to a bound ALT chord must miss
    ///   too — `Ctrl+Alt+Left` is not `<Alt>Left`, so it stays Neovim's. A modifier
    ///   bit outside Wolf's accel grammar (Super) is ignored, which the last two
    ///   rows pin from both sides.
    #[test]
    fn action_for_key_table() {
        let none = gdk::ModifierType::empty();
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        let shift = gdk::ModifierType::SHIFT_MASK;
        let alt = gdk::ModifierType::ALT_MASK;
        let sup = gdk::ModifierType::SUPER_MASK;

        // (description, delivered keyval, modifier state, expected action)
        let cases: Vec<(&str, gdk::Key, gdk::ModifierType, Option<&'static str>)> = vec![
            // --- The twelve BOUND chords, in both letter cases where they have one
            (
                "Ctrl+Shift+o -> win.go-to-file",
                key_from_char('o'),
                ctrl | shift,
                Some("win.go-to-file"),
            ),
            (
                "Ctrl+Shift+O (the SHIFTED glyph a real keymap delivers) -> win.go-to-file",
                key_from_char('O'),
                ctrl | shift,
                Some("win.go-to-file"),
            ),
            (
                "Ctrl+Shift+n -> win.new-buffer",
                key_from_char('n'),
                ctrl | shift,
                Some("win.new-buffer"),
            ),
            (
                "Ctrl+Shift+N (shifted glyph) -> win.new-buffer",
                key_from_char('N'),
                ctrl | shift,
                Some("win.new-buffer"),
            ),
            (
                "Ctrl+Shift+w -> win.close-pane",
                key_from_char('w'),
                ctrl | shift,
                Some("win.close-pane"),
            ),
            (
                "Ctrl+Shift+W (shifted glyph) -> win.close-pane",
                key_from_char('W'),
                ctrl | shift,
                Some("win.close-pane"),
            ),
            (
                "Ctrl+Shift+q -> app.quit",
                key_from_char('q'),
                ctrl | shift,
                Some("app.quit"),
            ),
            (
                "Ctrl+Shift+Q (shifted glyph) -> app.quit",
                key_from_char('Q'),
                ctrl | shift,
                Some("app.quit"),
            ),
            (
                "Ctrl+Shift+p -> win.command-palette",
                key_from_char('p'),
                ctrl | shift,
                Some("win.command-palette"),
            ),
            (
                "Ctrl+Shift+P (shifted glyph) -> win.command-palette",
                key_from_char('P'),
                ctrl | shift,
                Some("win.command-palette"),
            ),
            (
                "Ctrl+Shift+e -> win.buffer-picker",
                key_from_char('e'),
                ctrl | shift,
                Some("win.buffer-picker"),
            ),
            (
                "Ctrl+Shift+E (the SHIFTED glyph a real keymap delivers) -> win.buffer-picker",
                key_from_char('E'),
                ctrl | shift,
                Some("win.buffer-picker"),
            ),
            (
                "Ctrl+Shift+f -> win.search-in-project",
                key_from_char('f'),
                ctrl | shift,
                Some("win.search-in-project"),
            ),
            (
                "Ctrl+Shift+F (the SHIFTED glyph a real keymap delivers) -> win.search-in-project",
                key_from_char('F'),
                ctrl | shift,
                Some("win.search-in-project"),
            ),
            (
                "Ctrl+Shift+b -> win.toggle-sidebar",
                key_from_char('b'),
                ctrl | shift,
                Some("win.toggle-sidebar"),
            ),
            (
                "Ctrl+Shift+B (the SHIFTED glyph a real keymap delivers) -> win.toggle-sidebar",
                key_from_char('B'),
                ctrl | shift,
                Some("win.toggle-sidebar"),
            ),
            (
                "Ctrl+Shift+c -> win.copy",
                key_from_char('c'),
                ctrl | shift,
                Some("win.copy"),
            ),
            (
                "Ctrl+Shift+C (the SHIFTED glyph a real keymap delivers) -> win.copy",
                key_from_char('C'),
                ctrl | shift,
                Some("win.copy"),
            ),
            (
                "Ctrl+Shift+v -> win.paste",
                key_from_char('v'),
                ctrl | shift,
                Some("win.paste"),
            ),
            (
                "Ctrl+Shift+V (the SHIFTED glyph a real keymap delivers) -> win.paste",
                key_from_char('V'),
                ctrl | shift,
                Some("win.paste"),
            ),
            // --- The two ARROW chords, which have no letter case --------------
            //
            // These are the accelerator namespace's deliberate exception: `<Alt>` is
            // refused with a single-LETTER key (`forbidden_reason`) and permitted
            // with a NAMED one, because capturing `<A-Left>` is precisely what stops
            // it reaching Neovim and splitting into `<Esc>` + `<Left>`.
            (
                "Alt+Left -> win.history-back",
                gdk::Key::Left,
                alt,
                Some("win.history-back"),
            ),
            (
                "Alt+Right -> win.history-forward",
                gdk::Key::Right,
                alt,
                Some("win.history-forward"),
            ),
            // --- Bare Ctrl+<letter> stays Neovim's ---------------------------
            (
                "Ctrl+o (no Shift) -> None, so <C-o> still reaches Neovim",
                key_from_char('o'),
                ctrl,
                None,
            ),
            (
                "Ctrl+n (no Shift) -> None, so <C-n> still reaches Neovim",
                key_from_char('n'),
                ctrl,
                None,
            ),
            (
                "Ctrl+w (no Shift) -> None, so <C-w> (the window prefix) still reaches Neovim",
                key_from_char('w'),
                ctrl,
                None,
            ),
            (
                "Ctrl+q (no Shift) -> None, so <C-q> still reaches Neovim",
                key_from_char('q'),
                ctrl,
                None,
            ),
            (
                "Ctrl+p (no Shift) -> None, so <C-p> still reaches Neovim",
                key_from_char('p'),
                ctrl,
                None,
            ),
            (
                "Ctrl+e (no Shift) -> None, so <C-e> (scroll down one line) still reaches Neovim",
                key_from_char('e'),
                ctrl,
                None,
            ),
            (
                "Ctrl+f (no Shift) -> None, so <C-f> (page forward) still reaches Neovim",
                key_from_char('f'),
                ctrl,
                None,
            ),
            (
                "Ctrl+b (no Shift) -> None, so <C-b> (page backward) still reaches Neovim",
                key_from_char('b'),
                ctrl,
                None,
            ),
            (
                "Ctrl+c (no Shift) -> None, so <C-c> (INTERRUPT: aborts a running mapping, a hung \
                 :global or a pending operator) still reaches Neovim",
                key_from_char('c'),
                ctrl,
                None,
            ),
            (
                "Ctrl+v (no Shift) -> None, so <C-v> (visual BLOCK in normal mode, literal-next \
                 in insert and cmdline) still reaches Neovim",
                key_from_char('v'),
                ctrl,
                None,
            ),
            // --- Reserved rows must never match ------------------------------
            (
                "Ctrl+minus -> None: win.decrease-font-size is Reserved, not installed",
                key_from_char('-'),
                ctrl,
                None,
            ),
            // --- The arrow chords match EXACTLY too ---------------------------
            (
                "Ctrl+Alt+Left -> None: the accel names Alt alone, and the match is exact",
                gdk::Key::Left,
                ctrl | alt,
                None,
            ),
            (
                "Alt+Up -> None: a named key with Alt that no row binds",
                gdk::Key::Up,
                alt,
                None,
            ),
            // --- Modifier matching is EXACT on Ctrl / Shift / Alt -------------
            (
                "Ctrl+Shift+Alt+o -> None: the accel does not name Alt, and the match is exact",
                key_from_char('o'),
                ctrl | shift | alt,
                None,
            ),
            (
                "plain o (no modifiers) -> None",
                key_from_char('o'),
                none,
                None,
            ),
            (
                "Shift+o -> None: typing an uppercase O must not open the picker",
                key_from_char('O'),
                shift,
                None,
            ),
            // --- Super is outside the accel grammar, so its bit is IGNORED ----
            (
                "Super+Shift+o -> None: Ctrl is missing, and Super can never supply it",
                key_from_char('o'),
                sup | shift,
                None,
            ),
            (
                "Ctrl+Shift+Super+o -> win.go-to-file: a modifier bit the grammar does not know \
                 (Super, Lock, a held mouse button) is ignored rather than blocking the chord",
                key_from_char('o'),
                ctrl | shift | sup,
                Some("win.go-to-file"),
            ),
        ];

        for (desc, keyval, state, expected) in cases {
            assert_eq!(
                action_for_key(keyval, state),
                expected,
                "action_for_key case: {desc}"
            );
        }
    }

    /// [`key_name_matches`] over every key-NAME shape [`ACTION_TABLE`] uses.
    ///
    /// The NAMED-key branch (`Left` / `Right`) is LIVE: `<Alt>Left` and
    /// `<Alt>Right` are Bound, so [`action_for_key`] reaches it on every press of
    /// either arrow chord and `action_for_key_table` covers it end to end. What
    /// remains reserved-only is the PUNCTUATION-name branch (`equal` / `minus` /
    /// `grave`), which no Bound row uses — the three font-size rows and the
    /// terminal row are Reserved — so those cases are exercised here and nowhere
    /// else. Pinning them keeps a later promotion a one-line status change instead
    /// of a debugging session over a chord that silently never fires.
    ///
    /// The negative rows are the point: a name outside the known set must be
    /// `false` rather than a guess, and a punctuation name compared against a
    /// non-printable keyval (whose `to_unicode()` is `None`) must be `false` rather
    /// than a panic. `Up` is the sharpest of them — it is a real `gdk::Key`
    /// constant that [`key_name_matches`] deliberately does not know, so it must
    /// answer `false` rather than fall through to the alphanumeric branch.
    #[test]
    fn key_name_matches_table() {
        // (GTK key NAME, delivered keyval, whether the name denotes that keyval)
        let cases: Vec<(&str, gdk::Key, bool)> = vec![
            // Named non-printable keys (`<Alt>Left` / `<Alt>Right`).
            ("Left", gdk::Key::Left, true),
            ("Right", gdk::Key::Right, true),
            ("Left", gdk::Key::Right, false),
            ("Right", gdk::Key::Left, false),
            ("Left", key_from_char('l'), false),
            // Punctuation NAMES, never the symbol: `<Control>minus`, not
            // `<Control>-` (the trap `accels_use_key_names_not_symbols` guards).
            ("minus", key_from_char('-'), true),
            ("equal", key_from_char('='), true),
            ("grave", key_from_char('`'), true),
            ("minus", key_from_char('='), false),
            ("equal", key_from_char('-'), false),
            ("grave", key_from_char('~'), false),
            // A punctuation name against a non-printable keyval: `to_unicode()` is
            // None there, so this must be false rather than panicking.
            ("minus", gdk::Key::Left, false),
            // Digits (`<Control>0`).
            ("0", key_from_char('0'), true),
            ("0", key_from_char('9'), false),
            // Single letters, compared case-INSENSITIVELY (the shift-glyph
            // contract: Ctrl+Shift+O delivers the keyval `O`).
            ("o", key_from_char('o'), true),
            ("o", key_from_char('O'), true),
            ("O", key_from_char('o'), true),
            ("o", key_from_char('p'), false),
            // Anything outside the known set is false, never a guess.
            ("", key_from_char('a'), false),
            ("Up", gdk::Key::Up, false),
            ("space", key_from_char(' '), false),
        ];

        for (name, keyval, expected) in cases {
            assert_eq!(
                key_name_matches(name, keyval),
                expected,
                "key_name_matches({name:?}, {keyval:?}) — an unknown or mis-cased key NAME must \
                 resolve deterministically: a chord that matched the WRONG key would be worse \
                 than one that never fires."
            );
        }
    }

    /// Command <-> action is a bijection over the rows that carry a command.
    #[test]
    fn command_action_round_trip() {
        for binding in ACTION_TABLE {
            let Some(command) = binding.command else {
                continue;
            };
            assert_eq!(
                command_for_action(binding.action),
                Some(command),
                "command_for_action({:?}) must resolve to {command:?}",
                binding.action
            );
            assert_eq!(
                action_for_command(command),
                Some(binding.action),
                "action_for_command({command:?}) must resolve to {:?}. A mismatch means two rows \
                 claim the same command, so the drained command dispatches to whichever row \
                 happens to be first.",
                binding.action
            );
        }
    }

    /// Every menu item references an action that is actually declared.
    ///
    /// A `gio::Menu` item naming an unknown action renders INSENSITIVE at popup
    /// time — a silent, visual-only failure no other test would catch.
    #[test]
    fn menu_items_reference_declared_actions() {
        assert!(
            !MENU_SECTIONS.is_empty(),
            "MENU_SECTIONS is empty, so every menu test in this module would pass vacuously. \
             Populate it from plan section 3.1."
        );

        for (index, section) in MENU_SECTIONS.iter().enumerate() {
            assert!(
                !section.items.is_empty(),
                "MENU_SECTIONS[{index}] has no items; an empty section renders as a stray \
                 separator."
            );
            for item in section.items {
                assert!(
                    binding_for_action(item.action).is_some(),
                    "menu item {:?} references action {:?}, which is not declared in \
                     ACTION_TABLE. gio renders such an item insensitive rather than failing, so \
                     the menu would ship with a dead entry.",
                    item.label,
                    item.action
                );
            }
        }
    }

    /// The menu never lists a RESERVED action: reserved chords are not registered
    /// this unit and their actions are not installed, so the item would do
    /// nothing.
    #[test]
    fn menu_never_lists_a_reserved_action() {
        for section in MENU_SECTIONS {
            for item in section.items {
                let binding = binding_for_action(item.action).unwrap_or_else(|| {
                    panic!(
                        "menu item {:?} references undeclared action {:?}; see \
                         `menu_items_reference_declared_actions`",
                        item.label, item.action
                    )
                });
                assert_ne!(
                    binding.status,
                    BindingStatus::Reserved,
                    "menu item {:?} points at RESERVED action {:?}. A reserved action is not \
                     installed this unit, so the item would appear in the primary menu and do \
                     nothing when clicked.",
                    item.label,
                    item.action
                );
            }
        }
    }

    /// Where a menu item's action maps to a registry command, its label is that
    /// command's `CommandSpec::title`.
    ///
    /// The registry is the single source of truth for user-visible command names;
    /// hand-typing them into the GTK menu is how the two silently desync.
    #[test]
    fn menu_labels_match_the_registry() {
        for section in MENU_SECTIONS {
            for item in section.items {
                let Some(command) = command_for_action(item.action) else {
                    continue;
                };
                let title = command.spec().title;
                assert_eq!(
                    item.label, title,
                    "menu item for action {:?} is labelled {:?} but the registry titles \
                     {command:?} as {title:?}. Take the label from `Command::spec().title` so a \
                     registry rename cannot leave the GTK menu behind.",
                    item.action, item.label
                );
            }
        }
    }

    /// The About row spends no chord, and therefore cannot shadow a Neovim key.
    ///
    /// Every accelerator invariant in this module opens with
    /// `let Some(accel) = binding.accel else { continue; }`, so a row with no
    /// accelerator is SKIPPED by all four rather than checked by them. That is
    /// correct, and it is also why the property has to be stated somewhere: with
    /// `accel: None` there is nothing for `no_accel_makes_a_neovim_key_unreachable`
    /// to evaluate, so the four together say nothing whatsoever about this row.
    ///
    /// The row is menu-only by design. About carries the bundled fonts' licence
    /// notices, which is a surface a user opens once, and `Ctrl+Shift+<letter>` is
    /// the whole of this front-end's available namespace — spending one of its
    /// letters here would give up a Neovim alias for nothing.
    #[test]
    fn the_about_row_carries_no_chord() {
        const ABOUT_ACTION: &str = "win.about";

        let binding = binding_for_action(ABOUT_ACTION).unwrap_or_else(|| {
            panic!(
                "no ACTION_TABLE row declares {ABOUT_ACTION:?}. The About dialog is installed \
                 through this table — `shell::install_actions` skips an action it cannot resolve \
                 and prints a reason — so without the row the menu item renders insensitive and \
                 the licence notices are unreachable from the running application."
            )
        });

        assert_eq!(
            binding.accel, None,
            "the {ABOUT_ACTION:?} row declares an accelerator ({:?}). Two things follow and both \
             are unwanted: it spends one of the finitely many `Ctrl+Shift+<letter>` chords on a \
             dialog opened once, and it starts being evaluated by \
             `no_accel_makes_a_neovim_key_unreachable`, which is the test that decides whether a \
             Neovim key just became unreachable.",
            binding.accel
        );
        assert_eq!(
            binding.status,
            BindingStatus::MenuOnly,
            "the {ABOUT_ACTION:?} row is {:?}, not MenuOnly. `Bound` would put it in the set \
             `bound_set_is_exactly_this_unit` enumerates while it has no chord to register; \
             `Reserved` would make `menu_never_lists_a_reserved_action` reject the menu item, and \
             a reserved action is not installed at all — the menu entry would then be present and \
             do nothing when clicked.",
            binding.status
        );
        assert_eq!(
            binding.command, None,
            "the {ABOUT_ACTION:?} row claims registry command {:?}. There is no About command in \
             `wolf_core::COMMAND_SPECS`: this action is front-end-only and resolves through \
             `command_action::ui_action_for_front_end_action`. A command here would also subject \
             the menu label to `menu_labels_match_the_registry`, which would then demand a title \
             the registry does not have.",
            binding.command
        );

        // Menu-only means exactly that: the primary menu is the ONLY route to it,
        // so a row absent from MENU_SECTIONS is an action nothing can ever activate.
        let listed = MENU_SECTIONS
            .iter()
            .flat_map(|section| section.items)
            .any(|item| item.action == ABOUT_ACTION);
        assert!(
            listed,
            "the {ABOUT_ACTION:?} row is MenuOnly but no MENU_SECTIONS item references it, so \
             nothing reaches it: it has no accelerator, no registry command to be drained, and now \
             no menu entry either. `menu_items_reference_declared_actions` checks the other \
             direction and cannot see this."
        );
    }
}
