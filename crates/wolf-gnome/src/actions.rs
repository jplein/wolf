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
//! ## The ctrl-folding class
//!
//! A chord reaches Neovim as BYTES, and `nvim_input` folds `Ctrl+<symbol>` to
//! `symbol & 0x1F` for every symbol in `0x40..=0x7F`. So a `Ctrl+<punctuation>`
//! chord does not merely claim a key: it consumes a CONTROL CODE. `<Control>grave`
//! consumes NUL (`0x60 & 0x1F == 0x00`, the backtick spelling of `<C-@>`), and
//! `<Control>bracketleft` would consume `0x1B` — ESCAPE.
//!
//! [`ctrl_fold`] computes that byte and [`ActionBinding::ctrl_fold`] declares it
//! per row, so [`forbidden_reason`] refuses three things: a `<Control>` chord over
//! a key NAME it cannot resolve, any fold onto [`CTRL_FOLD_FORBIDDEN`], and a
//! declaration that disagrees with the fold in either direction. Binding a chord
//! that eats a control code is therefore a deliberate act with the price written on
//! the row, rather than a silent one.
//!
//! Single-LETTER keys declare nothing, and the reason is the modifier MASK rather
//! than the character class: the first rule already refuses a bare `Ctrl+<letter>`,
//! so every surviving letter row carries Shift, and [`action_for_key`] matches masks
//! EXACTLY. `DECISIONS.md`'s accelerator section works through what that does and
//! does not cost Neovim — read it there rather than re-deriving it from which
//! characters fold.
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
    /// prefix scopes the action to ONE window and `app.` to the whole process, and
    /// the split decides who installs the row: `app.` rows are installed exactly
    /// once by [`crate::coordinator::Coordinator::install_app_actions`], while
    /// every `win.` row is installed per window by
    /// `shell::AppShell::install_actions`. `gio::ActionMap::add_action` REPLACES an
    /// existing entry, so an `app.` row installed per window would leave the last
    /// window's closure owning it for the whole application.
    pub action: &'static str,
    /// The GTK accelerator string, or `None` for a menu-only action.
    pub accel: Option<&'static str>,
    /// The control byte this row's chord CONSUMES by Ctrl-folding, declared per
    /// row so that binding one is a deliberate, reviewable act.
    ///
    /// `None` means "this chord folds to nothing", and [`forbidden_reason`]
    /// refuses the row when the declaration and [`ctrl_fold`]'s answer disagree in
    /// either direction. See "The ctrl-folding class" in the module docs.
    pub ctrl_fold: Option<u8>,
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
        ctrl_fold: None,
        command: Some(Command::GoToFile),
        status: BindingStatus::Bound,
        note: "macOS Cmd+O; gives up only the Ctrl+Shift alias for <C-o>, which is still typed as \
               plain Ctrl+O",
    },
    ActionBinding {
        action: "win.open-folder",
        accel: None,
        ctrl_fold: None,
        command: Some(Command::OpenFolder),
        status: BindingStatus::MenuOnly,
        note: "macOS Shift+Cmd+O; reached from the primary menu and the empty state's button, so \
               no chord is spent on it",
    },
    ActionBinding {
        action: "win.open-file",
        accel: None,
        ctrl_fold: None,
        command: Some(Command::OpenFile),
        status: BindingStatus::MenuOnly,
        note: "no macOS accelerator either (NSOpenPanel is menu-only there); reached from the \
               primary menu",
    },
    ActionBinding {
        action: "win.new-buffer",
        accel: Some("<Control><Shift>n"),
        ctrl_fold: None,
        command: Some(Command::NewBuffer),
        status: BindingStatus::Bound,
        note: "macOS Cmd+N; gives up only the Ctrl+Shift alias for <C-n>, which is still typed as \
               plain Ctrl+N",
    },
    ActionBinding {
        action: "app.new-window",
        accel: None,
        ctrl_fold: None,
        command: Some(Command::NewWindow),
        status: BindingStatus::MenuOnly,
        note:
            "macOS Shift+Cmd+N; menu-only because Ctrl+Shift+N is spent on win.new-buffer and no \
               Ctrl+Shift+<letter> is free, so the menu, the palette and :WolfNewWindow are its \
               routes",
    },
    ActionBinding {
        action: "win.new-tab",
        accel: Some("<Control><Shift>t"),
        ctrl_fold: None,
        command: Some(Command::NewTab),
        status: BindingStatus::Bound,
        note: "macOS Cmd+T; opens a native tab over this window's core, and gives up only the \
               Ctrl+Shift alias for <C-t>",
    },
    ActionBinding {
        action: "win.close-pane",
        accel: Some("<Control><Shift>w"),
        ctrl_fold: None,
        command: Some(Command::ClosePane),
        status: BindingStatus::Bound,
        note: "macOS Cmd+W; gives up only the Ctrl+Shift alias for <C-w>, the window-command \
               prefix, which is still typed as plain Ctrl+W",
    },
    ActionBinding {
        action: "win.close-window",
        accel: None,
        ctrl_fold: None,
        command: Some(Command::CloseWindow),
        status: BindingStatus::MenuOnly,
        note: "macOS Shift+Cmd+W; the GNOME shell already closes a window, so this is menu-only",
    },
    ActionBinding {
        action: "win.minimize-window",
        accel: None,
        ctrl_fold: None,
        command: Some(Command::MinimizeWindow),
        status: BindingStatus::MenuOnly,
        note: "macOS Cmd+M; the GNOME shell owns minimize, so the command is dispatched but no \
               chord is bound",
    },
    ActionBinding {
        action: "app.quit",
        accel: Some("<Control><Shift>q"),
        ctrl_fold: None,
        command: Some(Command::QuitApplication),
        status: BindingStatus::Bound,
        note: "macOS Cmd+Q; gives up only the Ctrl+Shift alias for <C-q> (= CTRL-V in \
               insert/cmdline), which is still typed as plain Ctrl+Q",
    },
    // --- The command-palette overlay -----------------------------------------
    ActionBinding {
        action: "win.command-palette",
        accel: Some("<Control><Shift>p"),
        ctrl_fold: None,
        command: Some(Command::CommandPalette),
        status: BindingStatus::Bound,
        note: "macOS Cmd+P; the strongest Linux idiom for this chord, and gives up only the \
               Ctrl+Shift alias for <C-p>",
    },
    // --- The buffer-picker overlay -------------------------------------------
    ActionBinding {
        action: "win.buffer-picker",
        accel: Some("<Control><Shift>e"),
        ctrl_fold: None,
        command: Some(Command::BufferPicker),
        status: BindingStatus::Bound,
        note: "macOS Cmd+E; gives up only the Ctrl+Shift alias for <C-e> (scroll down one line), \
               which is still typed as plain Ctrl+E",
    },
    // --- The project-search overlay ------------------------------------------
    ActionBinding {
        action: "win.search-in-project",
        accel: Some("<Control><Shift>f"),
        ctrl_fold: None,
        command: Some(Command::SearchInProject),
        status: BindingStatus::Bound,
        note: "macOS Shift+Cmd+F; gives up only the Ctrl+Shift alias for <C-f> (page forward), \
               which is still typed as plain Ctrl+F",
    },
    // --- The system clipboard -------------------------------------------------
    ActionBinding {
        action: "win.copy",
        accel: Some("<Control><Shift>c"),
        ctrl_fold: None,
        command: Some(Command::Copy),
        status: BindingStatus::Bound,
        note: "macOS Cmd+C; the GNOME Terminal spelling, and gives up only the Ctrl+Shift alias \
               for <C-c>, the INTERRUPT, which is still typed as plain Ctrl+C",
    },
    ActionBinding {
        action: "win.paste",
        accel: Some("<Control><Shift>v"),
        ctrl_fold: None,
        command: Some(Command::Paste),
        status: BindingStatus::Bound,
        note: "macOS Cmd+V; the GNOME Terminal spelling, and gives up only the Ctrl+Shift alias \
               for <C-v>, visual BLOCK, which is still typed as plain Ctrl+V",
    },
    // --- The file-tree sidebar ------------------------------------------------
    ActionBinding {
        action: "win.toggle-sidebar",
        accel: Some("<Control><Shift>b"),
        ctrl_fold: None,
        command: Some(Command::ToggleSidebar),
        status: BindingStatus::Bound,
        note: "macOS Ctrl+Cmd+S; B is the cross-platform sidebar letter and gives up only the \
               Ctrl+Shift alias for <C-b>",
    },
    // --- Reserved for later units (declared here so the scheme is auditable) --
    ActionBinding {
        action: "win.sidebar-show-project",
        accel: Some("<Control><Shift>s"),
        ctrl_fold: None,
        command: Some(Command::SidebarShowProject),
        status: BindingStatus::Reserved,
        note: "macOS Shift+Cmd+E; gives up only the Ctrl+Shift alias for <C-s> (LSP signature \
               help in i/v/s)",
    },
    ActionBinding {
        action: "win.increase-font-size",
        accel: Some("<Control>equal"),
        ctrl_fold: None,
        command: Some(Command::IncreaseFontSize),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+=; <C-=> has no built-in Neovim meaning, so the idiomatic bare-Ctrl \
               chord is safe here",
    },
    ActionBinding {
        action: "win.decrease-font-size",
        accel: Some("<Control>minus"),
        ctrl_fold: None,
        command: Some(Command::DecreaseFontSize),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+-; <C--> has no built-in Neovim meaning, so the idiomatic bare-Ctrl \
               chord is safe here",
    },
    ActionBinding {
        action: "win.reset-font-size",
        accel: Some("<Control>0"),
        ctrl_fold: None,
        command: Some(Command::ResetFontSize),
        status: BindingStatus::Reserved,
        note: "macOS Cmd+0; assumed free because <C-0> has no index.txt entry, which is an \
               assumption and not a verified fact",
    },
    ActionBinding {
        action: "win.terminal",
        accel: Some("<Control>grave"),
        ctrl_fold: Some(0x00),
        command: Some(Command::Terminal),
        status: BindingStatus::Bound,
        note: "macOS Ctrl+`; MEASURED free (nothing maps <C-`> in n/i/v/t/c and helpgrep finds \
               nothing), and the price is the BACKTICK spelling of <C-@>, since Ctrl+backtick \
               folds to NUL — Ctrl+Shift+2 (keyval `at`) survives unbound and still types it",
    },
    ActionBinding {
        action: "win.history-back",
        accel: Some("<Alt>Left"),
        ctrl_fold: None,
        command: None,
        status: BindingStatus::Bound,
        note: "macOS Cmd+[; front-end-only navigation with no Command variant, and capturing \
               <A-Left> PREVENTS the destructive <Esc>+<Left> split",
    },
    ActionBinding {
        action: "win.history-forward",
        accel: Some("<Alt>Right"),
        ctrl_fold: None,
        command: None,
        status: BindingStatus::Bound,
        note: "macOS Cmd+]; front-end-only navigation with no Command variant, and capturing \
               <A-Right> PREVENTS the destructive <Esc>+<Right> split",
    },
    ActionBinding {
        action: "win.about",
        accel: None,
        ctrl_fold: None,
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
                label: "New Window",
                action: "app.new-window",
            },
            MenuItemSpec {
                label: "New Tab",
                action: "win.new-tab",
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
            MenuItemSpec {
                label: "Terminal",
                action: "win.terminal",
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
/// Only [`BindingStatus::Bound`] rows are considered. Matching a Reserved row would
/// swallow the key for an action that does nothing — but that consequence follows from
/// the row's CLASSIFICATION rather than from `Reserved` itself:
/// `shell::AppShell::install_actions` never reads `status` and installs any row whose
/// command classifies to a real `UiAction`. It holds because every Reserved row's
/// command classifies `Deferred`; one that classified otherwise would have a working
/// action sitting behind a chord nothing registers.
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

/// The ASCII symbol a GTK key NAME produces, or `None` for a name outside the set
/// this module resolves (a named non-printable key such as `Left`, or anything
/// unknown).
///
/// This is the ONE map both [`key_name_matches`] and [`ctrl_fold`] read, and that
/// sharing is the point rather than tidiness: a name that folds is by construction
/// a name the matcher can match, so a chord can never be waved through by the
/// invariant checker on a name the live matcher would then fail to recognise (or
/// the reverse). Widening it therefore widens both at once.
///
/// The set is deliberately larger than the names [`ACTION_TABLE`] uses today. Every
/// name here parses under [`parse_accel`]'s grammar, so every one of them can reach
/// [`forbidden_reason`], and the punctuation names beyond `equal` / `minus` /
/// `grave` are exactly the ones whose symbols lie in the Ctrl-folding range
/// `0x40..=0x7F` — `bracketleft` folds to `0x1B`, ESCAPE. Leaving them out would
/// make [`ctrl_fold`] answer [`CtrlFold::UnknownKey`] for a chord that really does
/// fold, which is refused rather than silently accepted, but the refusal message
/// would then name the wrong problem.
fn key_symbol(name: &str) -> Option<char> {
    Some(match name {
        "equal" => '=',
        "minus" => '-',
        "grave" => '`',
        "bracketleft" => '[',
        "bracketright" => ']',
        "backslash" => '\\',
        "asciicircum" => '^',
        "underscore" => '_',
        "at" => '@',
        "question" => '?',
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => c,
                _ => return None,
            }
        }
    })
}

/// Whether the GTK key NAME `name` denotes `keyval`.
///
/// Covers the named arrow keys directly, and every other name through
/// [`key_symbol`] (compared case-insensitively, because whether a shifted letter
/// arrives as `o` or `O` depends on the keymap). Anything else is `false` rather
/// than a guess — a chord that silently matched the wrong key would be worse than
/// one that never fires.
fn key_name_matches(name: &str, keyval: gdk::Key) -> bool {
    if name == "Left" {
        return keyval == gdk::Key::Left;
    }
    if name == "Right" {
        return keyval == gdk::Key::Right;
    }
    let Some(symbol) = key_symbol(name) else {
        return false;
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

/// What Ctrl-folding does to one parsed accelerator.
///
/// Neovim receives a chord as BYTES: `nvim_input` folds `Ctrl+<symbol>` to
/// `symbol & 0x1F` for every symbol in `0x40..=0x7F`, which is why `Ctrl+backtick`
/// arrives as NUL (`0x60 & 0x1F == 0x00`, i.e. `<C-@>`) rather than as a key of its
/// own. A chord that folds therefore consumes a control code, and the accelerator
/// namespace has to account for which one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlFold {
    /// The chord holds no `<Control>`, so nothing folds.
    NotCtrl,
    /// `<Control>` is held, the key NAME resolves, and its symbol lies outside the
    /// folding range — `<Control>0`, `<Control>equal` and `<Control>minus` are all
    /// this: Neovim sees an ordinary key, not a control byte.
    NoFold,
    /// `<Control>` is held and the chord folds to this control byte.
    Byte(u8),
    /// `<Control>` is held and [`key_symbol`] resolves the key NAME to nothing, so
    /// what the chord folds to is UNKNOWN.
    ///
    /// This variant, rather than [`CtrlFold::NoFold`], is what stops
    /// [`forbidden_reason`] being vacuous for exactly the rows the fold rule exists
    /// to catch: an unrecognised name is the shape a NEW punctuation chord arrives
    /// in, and answering "it does not fold" for it would wave through the next
    /// `<Control>bracketleft` as silently as the old checker did.
    UnknownKey,
}

/// The control bytes no accelerator may consume, WHATEVER it declares.
///
/// `0x1B` is ESCAPE and `0x7F` is DEL. Both are reachable from Neovim in every
/// mode and neither has a second spelling this front end leaves free, so a chord
/// that folds onto one is refused outright rather than being declarable. That is
/// the arm which makes `<Control>bracketleft` (`[` is `0x5B`; `0x5B & 0x1F` is
/// `0x1B`) loud instead of legal.
pub const CTRL_FOLD_FORBIDDEN: [u8; 2] = [0x1B, 0x7F];

/// What `p` folds to under Neovim's Ctrl-folding rule. See [`CtrlFold`].
///
/// `?` is the one symbol whose fold is not `symbol & 0x1F`: `0x3F` is below the
/// folding range, yet `Ctrl+?` is delivered as DEL (`0x7F`) by terminal convention,
/// so it is answered as such rather than as [`CtrlFold::NoFold`]. That convention is
/// NOT measured here, unlike `<Control>grave`, which was measured three ways against
/// the pinned Neovim. The direction of any error is safe: `0x7F` is in
/// [`CTRL_FOLD_FORBIDDEN`], so this arm only ever REFUSES a chord, and no row binds
/// `<Control>question`. Measure it before letting anything depend on it positively.
#[must_use]
pub fn ctrl_fold(p: &ParsedAccel) -> CtrlFold {
    if !p.ctrl {
        return CtrlFold::NotCtrl;
    }
    let Some(symbol) = key_symbol(p.key) else {
        return CtrlFold::UnknownKey;
    };
    if symbol == '?' {
        return CtrlFold::Byte(0x7F);
    }
    // `key_symbol` only ever answers ASCII, and a non-ASCII symbol would not fold
    // either way.
    let Ok(byte) = u8::try_from(symbol) else {
        return CtrlFold::NoFold;
    };
    if (0x40..=0x7F).contains(&byte) {
        CtrlFold::Byte(byte & 0x1F)
    } else {
        CtrlFold::NoFold
    }
}

/// Returns why `p` would violate the "no Neovim key becomes unreachable"
/// invariant, or `None` if the chord is safe to bind. `declared` is the row's
/// [`ActionBinding::ctrl_fold`].
///
/// Rejects a bare `Ctrl+<single letter>` (that WOULD make `<C-letter>`
/// unreachable) and `Alt` combined with a single-letter key (GTK's mnemonic
/// namespace; forwarded, an unmapped Alt chord splits into `<Esc>` + the base
/// key). `Alt` with a NAMED key (`Left` / `Right`) is allowed — capturing it is
/// what PREVENTS that destructive split. `Super` / `Primary` never reach here:
/// [`parse_accel`] rejects those tokens outright.
///
/// It then applies the ctrl-folding rules in order: an unresolvable key NAME under
/// `<Control>` is refused; a fold onto [`CTRL_FOLD_FORBIDDEN`] is refused whatever
/// the row declares; and a fold that disagrees with `declared` is refused, with
/// distinct messages for a MISSING declaration and a WRONG one.
///
/// **One checker, not two.** The fold rules take `declared` as a parameter instead
/// of living in a second `pub fn` precisely so a caller cannot run half of the
/// invariant: there is no way to ask "is this chord allowed?" and get an answer
/// that has skipped the folding class.
#[must_use]
pub fn forbidden_reason(p: &ParsedAccel, declared: Option<u8>) -> Option<&'static str> {
    let single_letter = p.key.len() == 1 && p.key.chars().all(|c| c.is_ascii_alphabetic());
    if single_letter {
        if p.ctrl && !p.shift && !p.alt {
            return Some(
                "a bare Ctrl+<letter> accelerator would make <C-letter> UNREACHABLE: Wolf matches \
                 its chords in the capture phase, before the window key controller that forwards \
                 keys to Neovim ever sees the event",
            );
        }
        if p.alt {
            return Some(
                "Alt+<letter> is GTK's mnemonic namespace, and an unmapped Alt chord forwarded to \
                 Neovim splits into <Esc> + the base key (insert-mode <A-p> pastes the unnamed \
                 register)",
            );
        }
    }

    let fold = ctrl_fold(p);
    if fold == CtrlFold::UnknownKey {
        return Some(
            "this chord holds <Control> over a key NAME `key_symbol` does not resolve, so what \
             control byte it consumes is UNKNOWN. Add the name to `key_symbol` — which widens the \
             live matcher with it — and then declare the fold on the row",
        );
    }
    if matches!(fold, CtrlFold::Byte(byte) if CTRL_FOLD_FORBIDDEN.contains(&byte)) {
        return Some(
            "this chord folds onto ESCAPE (0x1B) or DEL (0x7F), which no accelerator may consume \
             whatever it declares: both are reachable from Neovim in every mode and neither has a \
             second spelling this front end leaves free",
        );
    }

    // The single-LETTER class declares no fold, and the reason is the modifier MASK
    // rather than the character class: rule 1 above refuses a bare Ctrl+<letter>, so
    // every surviving letter row carries Shift, and `action_for_key` compares its
    // three modifiers EXACTLY — a Shift-carrying accelerator never intercepts the
    // unshifted event, so consuming 0x09 or 0x0D through Ctrl+Shift+I / Ctrl+Shift+M
    // costs Neovim nothing. DECISIONS.md's accelerator section carries that bargain.
    // The gate cannot hide the dangerous class either: `a`..`z` and `A`..`Z` fold
    // into 0x01..=0x1A, so no ASCII letter reaches CTRL_FOLD_FORBIDDEN.
    if single_letter {
        return None;
    }

    let actual = match fold {
        CtrlFold::Byte(byte) => Some(byte),
        _ => None,
    };
    if actual != declared {
        return Some(if declared.is_none() {
            "this chord folds to a control byte but the row declares no `ctrl_fold`. Consuming a \
             control code must be a deliberate, reviewable act: declare the byte on the row, and \
             say in its note what Neovim spelling is being given up"
        } else {
            "the row's declared `ctrl_fold` is not what this chord actually folds to. The \
             declaration is what a reviewer reads instead of re-deriving `keyval & 0x1F`, so a \
             stale one is worse than none"
        });
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
    const EXPECTED_BOUND_ACTIONS: [&str; 14] = [
        "app.quit",
        "win.buffer-picker",
        "win.close-pane",
        "win.command-palette",
        "win.copy",
        "win.go-to-file",
        "win.history-back",
        "win.history-forward",
        "win.new-buffer",
        "win.new-tab",
        "win.paste",
        "win.search-in-project",
        "win.terminal",
        "win.toggle-sidebar",
    ];

    /// Whether a chord spelling is ACCEPTABLE for Wolf to bind, declaring no
    /// ctrl-fold: it must parse under the accepted subset AND carry no forbidden
    /// reason.
    ///
    /// Both halves count as a rejection on purpose. `<Super>n` and `<Primary>p`
    /// are outside `parse_accel`'s grammar (so they are rejected there), while
    /// `<Control>p` and `<Alt>p` parse fine and are rejected by
    /// `forbidden_reason`. Which mechanism bites is an implementation detail; that
    /// the chord is refused is the contract.
    fn accel_is_accepted(spec: &'static str) -> bool {
        accel_is_accepted_with(spec, None)
    }

    /// [`accel_is_accepted`] with an explicit [`ActionBinding::ctrl_fold`]
    /// declaration, which is what the folding rules read.
    ///
    /// The declaration is a PARAMETER of the invariant rather than a separate
    /// check, so a chord's acceptability cannot be asked without saying what byte
    /// its row claims to consume — see `forbidden_reason`'s "one checker, not two".
    fn accel_is_accepted_with(spec: &'static str, declared: Option<u8>) -> bool {
        let Some(parsed) = parse_accel(spec) else {
            return false;
        };
        forbidden_reason(&parsed, declared).is_none()
    }

    /// Parses `spec` or panics, so a table row reads as the accelerator STRING a
    /// reader recognises rather than as a hand-built [`ParsedAccel`].
    ///
    /// A panic here is a broken FIXTURE, not a broken invariant: every spec passed
    /// to it is inside `parse_accel`'s grammar by construction, and one that is not
    /// would otherwise make its row assert about nothing.
    ///
    /// Named `parsed_accel` rather than `parsed` because several tests below bind a
    /// LOCAL called `parsed`, which would shadow the function inside their scope.
    fn parsed_accel(spec: &'static str) -> ParsedAccel {
        parse_accel(spec).unwrap_or_else(|| {
            panic!(
                "test fixture: accel {spec:?} does not parse under the accepted subset of the GTK \
                 accelerator grammar, so the row using it asserts about nothing. Allowed: zero or \
                 more <Control>/<Shift>/<Alt> tokens followed by ONE key NAME; see \
                 `every_accel_parses`."
            )
        })
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

    /// [`ctrl_fold`] over every shape the accel grammar admits: a chord holding no
    /// `<Control>`, a `<Control>` chord whose key does not fold, one whose key
    /// does, and one whose key NAME resolves to nothing at all.
    ///
    /// This is the arithmetic the folding rule rests on, and it is a claim about
    /// NEOVIM rather than about GTK: `nvim_input` folds `Ctrl+<symbol>` to
    /// `symbol & 0x1F` over `0x40..=0x7F`, so a `Ctrl+<punctuation>` chord consumes
    /// a control code rather than merely claiming a key. MEASURED for
    /// `<Control>grave` three ways against the pinned Neovim: insert mode raises
    /// `E29: No inserted text yet` and drops out of insert (`i_CTRL-@`), `<C-@>`
    /// through the identical sequence is byte-identical, and terminal mode forwards
    /// it with `cat -v` echoing `X^@Y`.
    ///
    /// FALSIFYING EDITS:
    ///
    /// - Widen the fold MASK (`& 0x1F` -> `& 0x7F`): the `grave` row answers
    ///   `Byte(0x60)` where `Byte(0x00)` was wanted. NOT isolated, for the same
    ///   reason the two bullets around it are not: `<Control>grave` is the live
    ///   `win.terminal` row, which declares `Some(0x00)`, so the loop in
    ///   `no_accel_makes_a_neovim_key_unreachable` reddens beside this test on the
    ///   WRONG-declaration rule (measured). Predicted to take
    ///   `a_shifted_ctrl_letter_needs_no_fold_declaration` with it as well, since
    ///   the same arithmetic makes `<Control><Shift>o` answer `Byte(0x6F)` rather
    ///   than `Byte(0x0F)`.
    /// - Widen the folding RANGE (`0x40..=0x7F` -> `0x20..=0x7F`): the three
    ///   `NoFold` rows start folding — `0` to `Byte(0x10)`, `equal` to `Byte(0x1D)`,
    ///   `minus` to `Byte(0x0D)`. NOT isolated: those three are the live Reserved
    ///   font-size rows, so `no_accel_makes_a_neovim_key_unreachable` reddens beside
    ///   this test on the missing-declaration rule. Nothing else asserts that those
    ///   three chords stay legal while declaring no `ctrl_fold`.
    /// - Answer [`CtrlFold::NoFold`] instead of [`CtrlFold::UnknownKey`] for an
    ///   unresolvable name: only the `<Control>Left` row moves. That isolation is
    ///   the point — no [`ACTION_TABLE`] row holds `<Control>` over a name
    ///   [`key_symbol`] cannot resolve, so the loop in
    ///   `no_accel_makes_a_neovim_key_unreachable` passes whatever that arm does,
    ///   and the arm is the one thing standing between the checker and the next
    ///   `<Control>bracketleft`.
    /// - Delete the `?` special case: the `question` row answers `NoFold`, because
    ///   `0x3F` sits BELOW the folding range while `Ctrl+?` is still delivered as
    ///   DEL.
    /// - Delete the `!p.ctrl` early return: the two `NotCtrl` rows answer
    ///   `UnknownKey` and `Byte(0x10)`. NOT isolated — `<Alt>Left` is a live Bound
    ///   row, so the invariant test reddens with them.
    #[test]
    fn ctrl_fold_table() {
        // (accelerator spelling, what Ctrl-folding makes of it)
        let cases: Vec<(&'static str, CtrlFold)> = vec![
            // --- No <Control>: nothing folds, whatever the key is --------------
            //
            // `<Alt>Left` is a live Bound row whose key NAME `key_symbol` does not
            // resolve, so it also pins the ORDER of the two checks: the <Control>
            // test has to run first, or this row would answer UnknownKey.
            ("<Alt>Left", CtrlFold::NotCtrl),
            ("<Shift>p", CtrlFold::NotCtrl),
            // --- <Control> over a symbol OUTSIDE 0x40..=0x7F ------------------
            //
            // These three are the live Reserved font-size rows. Neovim sees an
            // ordinary key, not a control byte, which is what makes them legal
            // while declaring `ctrl_fold: None`.
            ("<Control>0", CtrlFold::NoFold),
            ("<Control>equal", CtrlFold::NoFold),
            ("<Control>minus", CtrlFold::NoFold),
            // --- <Control> over a symbol that DOES fold -----------------------
            //
            // `grave` is the live `win.terminal` row: 0x60 & 0x1F == 0x00, the
            // BACKTICK spelling of <C-@>.
            ("<Control>grave", CtrlFold::Byte(0x00)),
            // `at` is Ctrl+Shift+2 — the OTHER spelling of that same byte, and the
            // one this front end deliberately leaves unbound so <C-@> stays typable.
            ("<Control>at", CtrlFold::Byte(0x00)),
            // `[` is 0x5B, so this folds onto ESCAPE. It is the chord
            // CTRL_FOLD_FORBIDDEN exists for and the one a vacuous checker waves
            // through; `no_accel_makes_a_neovim_key_unreachable` refuses it.
            ("<Control>bracketleft", CtrlFold::Byte(0x1B)),
            // A LETTER folds by the same arithmetic (0x70 & 0x1F == 0x10). The
            // declaration rule skips that class deliberately, for a reason the
            // invariant test pins; the fold itself is not special.
            ("<Control>p", CtrlFold::Byte(0x10)),
            // `?` is 0x3F, BELOW the folding range, yet Ctrl+? is delivered as DEL.
            // Nothing else in this crate pins that exception.
            ("<Control>question", CtrlFold::Byte(0x7F)),
            // --- <Control> over a NAME `key_symbol` does not resolve -----------
            ("<Control>Left", CtrlFold::UnknownKey),
        ];

        for (spec, expected) in cases {
            assert_eq!(
                ctrl_fold(&parsed_accel(spec)),
                expected,
                "ctrl_fold({spec:?}). This byte is what binding the chord takes AWAY from Neovim, \
                 so the three wrong answers fail differently: a `Byte` where `NoFold` was wanted \
                 claims a control code no row declared, a `NoFold` where a `Byte` was wanted hands \
                 `forbidden_reason` a folding chord it will wave through, and an `UnknownKey` \
                 where a byte was wanted means `key_symbol` lost a name the LIVE matcher still \
                 resolves through it."
            );
        }
    }

    /// No declared accelerator makes a Neovim key unreachable — and the checker
    /// that says so actually bites.
    ///
    /// The negative cases are the point of the test. A `forbidden_reason` that
    /// never returns `Some` would satisfy the table half of this test forever
    /// while permitting exactly the bug the invariant exists to prevent.
    ///
    /// The loop passes each row's own [`ActionBinding::ctrl_fold`] declaration, so
    /// deleting `ctrl_fold: Some(0x00)` from the `win.terminal` row fails it by
    /// name, on the missing-declaration reason. Every other row declares `None` and
    /// folds to nothing, which is why the folding rules need the explicit rows
    /// below: without them the loop is satisfied by all three arms doing nothing.
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
            let reason = forbidden_reason(&parsed, binding.ctrl_fold);
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

        // --- The ctrl-folding rules, which the loop above cannot exercise ---
        //
        // Every live row either holds no <Control> or folds to exactly what it
        // declares, so the loop passes whatever these arms do. The rows below are
        // what make them bite.
        assert!(
            !accel_is_accepted_with("<Control>grave", None),
            "`<Control>grave` declaring NO ctrl_fold MUST be refused. `nvim_input` folds \
             Ctrl+backtick to NUL (0x60 & 0x1F == 0x00), so binding the chord consumes the \
             BACKTICK spelling of `<C-@>` — a control code, not just a key. Consuming one has to \
             be a deliberate, reviewable act with the price written on the row, which is what the \
             declaration is for."
        );
        assert!(
            !accel_is_accepted_with("<Control>grave", Some(0x1F)),
            "a row whose declared ctrl_fold is not what its chord actually folds to MUST be \
             refused. The declaration is what a reviewer reads INSTEAD of re-deriving \
             `keyval & 0x1F`, so a stale one is worse than none: it documents a price the chord is \
             not paying, and the byte it names is then given up by nothing."
        );

        // `<Control>bracketleft` declares the byte it really consumes, so the
        // MISSING-declaration rule cannot fire for it and only the
        // CTRL_FOLD_FORBIDDEN arm can refuse it. Declaring `None` here instead would
        // be refused too — but by the wrong rule, and deleting the forbidden arm
        // would then leave this test GREEN while a chord folding to Escape got
        // through. That is the vacuity this pairing exists to close.
        let bracketleft_refusal =
            forbidden_reason(&parsed_accel("<Control>bracketleft"), Some(0x1B));
        assert!(
            bracketleft_refusal.is_some(),
            "`<Control>bracketleft` MUST be refused even when it declares the byte it really \
             consumes: `[` is 0x5B, and 0x5B & 0x1F is 0x1B — ESCAPE. Escape is reachable from \
             Neovim in EVERY mode and this front end leaves no second spelling of it free, so the \
             chord is refused outright rather than being declarable like `<Control>grave`."
        );
        assert_ne!(
            bracketleft_refusal,
            forbidden_reason(&parsed_accel("<Control>grave"), None),
            "the `<Control>bracketleft` refusal above and the MISSING-DECLARATION refusal are the \
             same reason, so the assertion above cannot say which rule ran. The bracketleft row \
             declares exactly what it folds to, so the declaration rule is unreachable for it and \
             the CTRL_FOLD_FORBIDDEN arm is the only thing that can refuse it — one shared string \
             would hide a deleted arm behind a rule that is still passing."
        );
        assert!(
            !accel_is_accepted_with("<Control>question", Some(0x7F)),
            "`<Control>question` MUST be refused even declaring 0x7F: `Ctrl+?` is delivered as \
             DEL, the SECOND entry of CTRL_FOLD_FORBIDDEN. Without this row that entry carries no \
             assertion at all — dropping 0x7F from the array would redden nothing, leaving a \
             one-element list wearing a two-element type."
        );
        assert!(
            accel_is_accepted_with("<Control>grave", Some(0x00)),
            "`<Control>grave` declaring Some(0x00) MUST be accepted. The chord is MEASURED free \
             (nothing maps `<C-`>` in n/i/v/t/c and `helpgrep CTRL-`` finds no entry), the \
             declaration matches the fold, and the price — the backtick spelling of `<C-@>`, with \
             Ctrl+Shift+2 surviving unbound to type it — is stated on the row. A checker that \
             refuses a correctly declared chord makes the declaration pointless and leaves the \
             terminal drawer with no chord at all."
        );
    }

    /// The `!single_letter` GATE on the declaration rule: a bound
    /// `Ctrl+Shift+<letter>` chord folds like any other, and declaring no
    /// [`ActionBinding::ctrl_fold`] for it is nevertheless correct.
    ///
    /// This is its own `#[test]` rather than a tail of
    /// `no_accel_makes_a_neovim_key_unreachable` because that test's
    /// [`ACTION_TABLE`] loop panics FIRST under the only falsifier that reaches
    /// either — on `win.go-to-file` / `<Control><Shift>o`, measured. A tail there
    /// can never produce a red of its own, which is documentation wearing a test's
    /// clothes; separated, it fails by name and its message is the one a reader
    /// sees.
    ///
    /// **The gate is safe because of the MODIFIER MASK, not the character class.**
    /// `forbidden_reason`'s first rule already refuses a BARE `Ctrl+<letter>`, so
    /// every surviving letter row carries Shift — and [`action_for_key`] compares
    /// the three modifiers EXACTLY, so a Shift-carrying accelerator never
    /// intercepts the unshifted `Ctrl+<letter>` event at all. Neovim still receives
    /// `<C-I>` (0x09, Tab) from Ctrl+I and `<C-M>` (0x0D, Enter) from Ctrl+M, which
    /// is why consuming those bytes on paper costs nothing. It is also exactly why
    /// `<Control>grave`, which carries NO Shift, genuinely does consume `` <C-`> ``
    /// while Ctrl+Shift+2 (keyval `at`, the same 0x00) survives unbound.
    ///
    /// FALSIFYING EDIT: delete the `!single_letter` gate from `forbidden_reason`'s
    /// declaration rule. Predicted red: `<Control><Shift>o` declaring `None` is
    /// refused, so the second assertion below fails and names the gate. NOT
    /// isolated — `no_accel_makes_a_neovim_key_unreachable` reddens beside it,
    /// firing on the first of the twelve letter rows, `win.go-to-file` (measured).
    /// That pairing is the disclosure, not the coverage: this test's red stands on
    /// its own either way, which is what a separate `#[test]` buys.
    #[test]
    fn a_shifted_ctrl_letter_needs_no_fold_declaration() {
        assert_eq!(
            ctrl_fold(&parsed_accel("<Control><Shift>o")),
            CtrlFold::Byte(0x0F),
            "Ctrl+Shift+O folds to 0x0F by the same arithmetic as every other Ctrl chord \
             (0x6F & 0x1F). If this answered NoFold, the gate asserted below would look like \
             tidiness rather than a decision, and the reason it is SAFE would be the wrong one — \
             the letter class is exempt because of the modifier mask, not because letters do not \
             fold."
        );
        assert!(
            accel_is_accepted_with("<Control><Shift>o", None),
            "a Ctrl+Shift+<letter> row declaring NO ctrl_fold MUST be accepted, even though the \
             chord folds to 0x0F. This is the live `win.go-to-file` row and eleven others like \
             it: without the `!single_letter` gate on the declaration rule, every one of them \
             would have to declare a byte it does not actually take from Neovim, because \
             `action_for_key` matches modifiers exactly and the unshifted Ctrl+<letter> event is \
             never intercepted."
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
    ///
    /// **This test is NOT isolated**, disclosed here the way `command_action`'s
    /// `dispatched_commands_are_the_expected_set` discloses its own. Demoting a
    /// Bound row to `Reserved` reddens up to two other tests beside it, and
    /// `win.terminal` is a row that moves all three:
    ///
    /// - `action_for_key_table` — for EVERY Bound row, because that table asserts
    ///   each of the fourteen chords resolves to its action and [`action_for_key`]
    ///   scans `Bound` rows only, so a demoted row's chord answers `None`. The
    ///   `Ctrl+grave -> win.terminal` row is what makes that true of this one
    ///   (measured).
    /// - `menu_never_lists_a_reserved_action` — for a Bound row that also carries a
    ///   menu item, because the item stays in [`MENU_SECTIONS`] through the
    ///   demotion. Its red opens by naming the item:
    ///   `menu item "Terminal" points at RESERVED action "win.terminal"` (measured).
    ///
    /// The pairing is signal rather than noise — a demotion that reddened only this
    /// test would mean the chord was already unmatched or the menu had already lost
    /// its item.
    ///
    /// `win.new-tab` is the OTHER row that moves all three, and it is this unit's.
    /// Reverting its status to `Reserved` reddens `action_for_key_table` on BOTH its
    /// `Ctrl+Shift+T` rows — [`action_for_key`] scans `Bound` rows only, so each
    /// letter case answers `None` — and `menu_never_lists_a_reserved_action` on the
    /// "New Tab" item, which stays in [`MENU_SECTIONS`] through the demotion.
    /// PREDICTED, not yet measured: the pass that runs the revert is the one that
    /// confirms the triple moves together, and should replace this word with
    /// `measured` and the failure text it saw.
    #[test]
    fn bound_set_is_exactly_this_unit() {
        let mut bound: Vec<&'static str> = bound_bindings().map(|b| b.action).collect();
        bound.sort_unstable();

        let mut expected: Vec<&'static str> = EXPECTED_BOUND_ACTIONS.to_vec();
        expected.sort_unstable();

        assert_eq!(
            bound, expected,
            "the set of BOUND accelerators changed. wolf-gnome registers exactly fourteen \
             (Ctrl+Shift+O go-to-file, Ctrl+Shift+N new-buffer, Ctrl+Shift+T new-tab, \
             Ctrl+Shift+P command-palette, Ctrl+Shift+E buffer-picker, \
             Ctrl+Shift+F search-in-project, Ctrl+Shift+W close-pane, \
             Ctrl+Shift+Q quit, Alt+Left history-back, Alt+Right history-forward, Ctrl+Shift+B \
             toggle-sidebar, Ctrl+Shift+C copy, Ctrl+Shift+V paste, Ctrl+grave terminal); every \
             other \
             row of the table is Reserved for a later unit or MenuOnly. A one-element diff naming \
             \"win.terminal\" means the terminal-drawer chord was demoted back to Reserved, in \
             which case `action_for_key` stops scanning the row and Ctrl+backtick falls through to \
             Neovim as <C-@> — the drawer is then reachable only from the primary menu, and \
             `command_action::ui_action_for` is left dispatching a command no chord reaches. \
             A one-element diff naming \"win.new-tab\" \
             means the native-tabs chord was demoted back to Reserved, in which case \
             `action_for_key` stops scanning the row and Ctrl+Shift+T falls through to Neovim as \
             <C-t> (the tag-stack jump in normal mode, one shiftwidth of indent in insert mode) — \
             a new tab is then reachable only from the primary menu, the palette row and \
             :WolfNewTab, and the menu item advertises a chord that cannot fire. That demotion is \
             NOT isolated: `action_for_key_table`'s two Ctrl+Shift+T rows report None and \
             `menu_never_lists_a_reserved_action` rejects the \"New Tab\" item, and those three \
             moving TOGETHER is what says the row's STATUS changed rather than this list. \
             Binding a reserved chord \
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
    /// * **The fourteen Bound chords must fire, in BOTH letter cases where they
    ///   have one.** A real keymap delivers the SHIFTED glyph for Ctrl+Shift+O —
    ///   keyval `O`, not `o` (`input::gdk_key`'s shift contract, and what
    ///   `tests/gdk_key.rs` pins) — so a case-SENSITIVE comparison would mean the
    ///   chord never fires on real hardware while a lowercase-only test still
    ///   passed. The two ARROW chords (`<Alt>Left` / `<Alt>Right`) have no letter
    ///   case at all: they resolve through [`key_name_matches`]' named-key branch,
    ///   which compares `gdk::Key` constants directly, so they carry one row each.
    /// * **`Ctrl+grave` is the one PUNCTUATION chord, and it must fire without
    ///   Shift and only without Shift.** `win.terminal` is the only Bound row whose
    ///   key is a punctuation NAME, so it is the only one that reaches
    ///   [`key_symbol`]'s punctuation arm through the live matcher — every other
    ///   punctuation row is Reserved, and a Reserved row is never scanned. Its two
    ///   negative rows are not decoration: the accel names no Shift and the match is
    ///   EXACT, so a delivered `grave` with Shift held must miss, and a real keymap
    ///   delivers keyval `asciitilde` for that press anyway (`<C-~>` folds to 0x1E,
    ///   not 0x00), which is the spelling the third row keeps as Neovim's.
    /// * **A bare `Ctrl+<letter>` must NOT fire.** This is the mechanical
    ///   enforcement of the invariant in the module docs: the matcher runs in the
    ///   CAPTURE phase, ahead of the key controller that forwards to Neovim and
    ///   returning `Propagation::Stop`, so a match here is a Neovim key made
    ///   UNREACHABLE. `<C-o>`, `<C-n>`, `<C-p>`, `<C-e>` (scroll down one line),
    ///   `<C-f>` (page forward), `<C-b>` (page backward), `<C-w>` (the
    ///   window-command prefix), `<C-t>` (jump to the older tag-stack entry in
    ///   normal mode, insert one shiftwidth of indent in insert mode), `<C-q>`,
    ///   `<C-c>` and `<C-v>` must all still reach Neovim.
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
    ///
    /// FALSIFYING EDIT for the terminal chord: change [`key_symbol`]'s `"grave"`
    /// arm. Predicted red: `Ctrl+grave` answers `None` where `Some("win.terminal")`
    /// was wanted, while `bound_set_is_exactly_this_unit` and
    /// `menu_never_lists_a_reserved_action` stay GREEN. That pairing is what tells
    /// this failure apart from the other way the chord dies — demoting the row back
    /// to `Reserved`, where those two are precisely the tests that go and this row
    /// follows them. `ctrl_fold_table`, `key_name_matches_table` and
    /// `no_accel_makes_a_neovim_key_unreachable` redden alongside it, because
    /// [`key_symbol`] is the ONE map all four read.
    #[test]
    fn action_for_key_table() {
        let none = gdk::ModifierType::empty();
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        let shift = gdk::ModifierType::SHIFT_MASK;
        let alt = gdk::ModifierType::ALT_MASK;
        let sup = gdk::ModifierType::SUPER_MASK;

        // (description, delivered keyval, modifier state, expected action)
        let cases: Vec<(&str, gdk::Key, gdk::ModifierType, Option<&'static str>)> = vec![
            // --- The fourteen BOUND chords, in both letter cases where they have one
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
                "Ctrl+Shift+t -> win.new-tab",
                key_from_char('t'),
                ctrl | shift,
                Some("win.new-tab"),
            ),
            (
                "Ctrl+Shift+T (the SHIFTED glyph a real keymap delivers) -> win.new-tab",
                key_from_char('T'),
                ctrl | shift,
                Some("win.new-tab"),
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
            // --- The one PUNCTUATION chord, which has no letter case either ----
            //
            // `<Control>grave` is the only Bound row whose key is a punctuation
            // NAME, so it is the only chord reaching `key_symbol`'s punctuation arm
            // through the live matcher. Binding it consumes the BACKTICK spelling of
            // <C-@> (0x60 & 0x1F == 0x00), which is the price stated on the row.
            (
                "Ctrl+grave -> win.terminal",
                key_from_char('`'),
                ctrl,
                Some("win.terminal"),
            ),
            (
                "Ctrl+Shift+grave -> None: the accel names no Shift, and the match is exact",
                key_from_char('`'),
                ctrl | shift,
                None,
            ),
            (
                "Ctrl+asciitilde -> None: what a real keymap DELIVERS for Ctrl+Shift+backtick is \
                 keyval `asciitilde`, so <C-~> (0x7E & 0x1F == 0x1E) must still reach Neovim",
                key_from_char('~'),
                ctrl | shift,
                None,
            ),
            (
                "plain grave (no modifiers) -> None: typing a backtick must not open the drawer",
                key_from_char('`'),
                none,
                None,
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
                "Ctrl+t (no Shift) -> None, so <C-t> (jump to the older tag-stack entry in normal \
                 mode, insert one shiftwidth of indent in insert mode) still reaches Neovim",
                key_from_char('t'),
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
    /// either arrow chord and `action_for_key_table` covers it end to end. So is
    /// part of the PUNCTUATION-name branch: `<Control>grave` is Bound for
    /// `win.terminal`, and `action_for_key_table` covers the `grave` rows the same
    /// way. What remains reserved-only is `equal` and `minus`, whose two font-size
    /// rows are Reserved, so those cases are exercised here and nowhere else.
    /// Pinning them keeps a later promotion a one-line status change instead of a
    /// debugging session over a chord that silently never fires.
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

    /// The menu never lists a RESERVED action: a reserved chord is neither
    /// registered nor scanned by [`action_for_key`], so the item would sit in the
    /// primary menu under a chord that cannot fire.
    ///
    /// This is a POLICY over [`MENU_SECTIONS`], not a consequence of
    /// [`BindingStatus`]: `shell::AppShell::install_actions` keys installation on
    /// the row's `UiAction` classification and never reads `status`, so a Reserved
    /// row is skipped there only because every Reserved row's command happens to
    /// classify as `Deferred`. The assertion message spells out both failures.
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
                    "menu item {:?} points at RESERVED action {:?}. `Reserved` declares a chord \
                     for a LATER unit: `bound_bindings` does not register it and `action_for_key` \
                     does not scan it, so the item's chord is dead however the item behaves. \
                     Installation does NOT follow from `status` — `install_actions` adds a \
                     `SimpleAction` for every row whose command classifies to a real `UiAction`, \
                     whatever the status — so the two failures this rule forbids DIFFER: a \
                     Reserved row classifying to `Deferred`, as each Reserved row's command does, \
                     is never installed and gio renders the item INSENSITIVE at popup time, while \
                     one classifying to a real `UiAction` gets a WORKING item under a chord that \
                     never fires. Keeping Reserved rows out of MENU_SECTIONS is a policy over \
                     this table, not a consequence of `status`.",
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
             `Reserved` would make `menu_never_lists_a_reserved_action` reject the menu item, which \
             is a POLICY over MENU_SECTIONS rather than a statement about installation. This row \
             would still be installed and the item would still WORK: `install_actions` keys on the \
             `UiAction` classification and never reads `status`, and About carries no registry \
             command, so it resolves through `ui_action_for_front_end_action` whatever its status \
             says. What `Reserved` would falsely declare is a chord held for a later unit, which \
             this row does not have.",
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

    /// The rows installed ONCE PER PROCESS — the `app.` namespace — sorted.
    ///
    /// A literal set rather than a count, so a red names the row that moved.
    const EXPECTED_APP_SCOPED_ACTIONS: [&str; 2] = ["app.new-window", "app.quit"];

    /// Exactly two rows are `app.`-scoped, and both are actions that need no
    /// PARTICULAR window to mean something.
    ///
    /// The namespace is not a naming convention: it decides who installs the row.
    /// [`crate::coordinator::Coordinator::install_app_actions`] installs every
    /// `app.` row exactly once, against the application, and
    /// `shell::AppShell::install_actions` installs every `win.` row once per
    /// window and skips the `app.` half entirely — so an `app.` row has exactly
    /// one closure for the whole process, and that closure holds the COORDINATOR
    /// rather than any window.
    ///
    /// An `app.` row whose behaviour needs a particular window therefore has none
    /// to act on. It would either act on an arbitrary window (whichever the single
    /// closure happened to capture) or, once that window closed, on none at all —
    /// silently, since `gio` activates the action either way. New Window opens a
    /// window that does not exist yet and Quit closes every window there is, so
    /// neither has that problem; every other row in the table is one window's.
    ///
    /// `gio::ActionMap::add_action` REPLACES an existing entry, which is what makes
    /// the split load-bearing rather than tidy: installing `app.quit` per window
    /// would leave the LAST window's closure owning the whole application's Quit,
    /// and closing that window would kill Ctrl+Shift+Q for every window still open.
    ///
    /// `command_action::tests::app_scoped_commands_classify_to_an_action_the_coordinator_performs`
    /// is the other half of the rule and the two are read together: this test pins
    /// WHICH rows are `app.`-scoped, that one pins what those rows may mean.
    ///
    /// FALSIFYING EDIT: rename `win.close-window` to `app.close-window`.
    /// PREDICTED: the set gains a third element, `"app.close-window"`, in sorted
    /// position before `"app.new-window"`. PREDICTED NOT ISOLATED, and by more
    /// than `action_names_are_valid` (which accepts either namespace and stays
    /// green): a `MENU_SECTIONS` item still names `"win.close-window"`, so
    /// `menu_items_reference_declared_actions` reddens and
    /// `menu_never_lists_a_reserved_action` panics on its lookup, and
    /// `command_action`'s classification test reddens because `CloseWindow` is not
    /// an action the coordinator performs. The isolating variant is
    /// `win.minimize-window`, which no menu item references and no chord binds:
    /// renaming it reddens this test and the classification test, and nothing else.
    #[test]
    fn app_scoped_rows_are_performable_without_a_window() {
        // The namespace is read the way both installers read it — `split_once('.')`
        // on the action name — rather than by `starts_with("app.")`, so a row named
        // `"apple.thing"` cannot be counted as app-scoped here while `gio` scopes it
        // somewhere else entirely.
        let mut app_scoped: Vec<&'static str> = ACTION_TABLE
            .iter()
            .filter(|binding| matches!(binding.action.split_once('.'), Some(("app", _))))
            .map(|binding| binding.action)
            .collect();
        app_scoped.sort_unstable();

        let mut expected: Vec<&'static str> = EXPECTED_APP_SCOPED_ACTIONS.to_vec();
        expected.sort_unstable();

        assert_eq!(
            app_scoped, expected,
            "the set of `app.`-scoped actions changed. These rows are installed ONCE per process \
             by `Coordinator::install_app_actions`, against a closure holding the coordinator and \
             no window at all, and no window's shell installs any of them. An EXTRA row means an \
             action whose behaviour needs a particular window now has none: it would act on \
             whichever window the single closure captured, or — once that window closed — on \
             nothing, with `gio` activating it just the same either way. A MISSING row means an \
             application-wide action became one window's: `add_action` REPLACES an existing entry, \
             so installing it per window leaves the last window's closure owning it for the whole \
             process, and closing that window silently unbinds it everywhere. Only two behaviours \
             qualify: New Window acts on a window that does not exist yet, and Quit acts on every \
             window there is."
        );
    }

    /// The New Window row spends no chord, and the primary menu is what makes it
    /// reachable.
    ///
    /// The sibling of [`the_about_row_carries_no_chord`], and it exists for the
    /// same structural reason: every accelerator invariant in this module opens
    /// with `let Some(accel) = binding.accel else { continue; }`, so a row with no
    /// accelerator is SKIPPED by all four rather than checked by them, and the
    /// property has to be stated somewhere.
    ///
    /// **Why this row spends no chord is a collision, not a preference.** macOS
    /// gives New Window Shift+Cmd+N, and the Linux transliteration of that is
    /// Ctrl+Shift+N — which `win.new-buffer` already holds (macOS Cmd+N), and
    /// `Ctrl+Shift+<letter>` is the whole of this front-end's available namespace.
    /// Giving this row that chord would not merely duplicate a binding: GTK
    /// resolves a duplicate to ONE action, so New would silently stop firing.
    ///
    /// The row is nevertheless DISPATCHED, so it is reachable from three routes
    /// with no accelerator among them: the primary menu item asserted below, the
    /// command palette (which offers a live row for every dispatched command), and
    /// `:WolfNewWindow`.
    ///
    /// FALSIFYING EDIT 1: delete the `"New Window"` item from `MENU_SECTIONS[0]`.
    /// PREDICTED: the `listed` assertion fails — the row then has no chord and no
    /// menu entry, leaving only the palette and the Ex command, which is a
    /// user-visible regression no other test sees.
    /// PREDICTED ISOLATED: yes — `menu_items_reference_declared_actions` and
    /// `menu_labels_match_the_registry` both walk MENU_SECTIONS and cannot see an
    /// item that is not there.
    ///
    /// FALSIFYING EDIT 2: give the row `accel: Some("<Control><Shift>n")`.
    /// PREDICTED: the `accel` assertion below fails, AND `accels_are_unique`
    /// reddens on the second occurrence of that spelling, naming
    /// `"app.new-window"`. PREDICTED NOT ISOLATED — and the collision is precisely
    /// why the row spends no chord, so the pair moving together is the evidence
    /// rather than noise.
    #[test]
    fn the_new_window_row_is_menu_only_and_listed() {
        const NEW_WINDOW_ACTION: &str = "app.new-window";

        let binding = binding_for_action(NEW_WINDOW_ACTION).unwrap_or_else(|| {
            panic!(
                "no ACTION_TABLE row declares {NEW_WINDOW_ACTION:?}. The row is how \
                 `Coordinator::install_app_actions` learns to install New Window at all, so \
                 without it the primary menu item renders insensitive and the coordinator's \
                 `open_new_window` is reachable only from the palette and `:WolfNewWindow`."
            )
        });

        assert_eq!(
            binding.accel, None,
            "the {NEW_WINDOW_ACTION:?} row declares an accelerator ({:?}). The Linux \
             transliteration of macOS's Shift+Cmd+N is Ctrl+Shift+N, which `win.new-buffer` \
             already holds, and `Ctrl+Shift+<letter>` is the whole of this front-end's available \
             namespace — GTK resolves a duplicate chord to ONE action, so spelling it here would \
             silently stop New from firing rather than give New Window a chord. It would also \
             start this row being evaluated by `no_accel_makes_a_neovim_key_unreachable`, the \
             test that decides whether a Neovim key just became unreachable.",
            binding.accel
        );
        assert_eq!(
            binding.ctrl_fold, None,
            "the {NEW_WINDOW_ACTION:?} row declares ctrl_fold {:?} while carrying no accelerator. \
             Every folding rule is reached only through `parse_accel(binding.accel?)`, so the \
             declaration is READ BY NOTHING: it states a control byte this row gives up while the \
             row gives up no key at all, and a reviewer takes the declaration for the price.",
            binding.ctrl_fold
        );
        assert_eq!(
            binding.status,
            BindingStatus::MenuOnly,
            "the {NEW_WINDOW_ACTION:?} row is {:?}, not MenuOnly. `Bound` would put it in the set \
             `bound_set_is_exactly_this_unit` enumerates while it has no chord to register; \
             `Reserved` would make `menu_never_lists_a_reserved_action` reject the menu item that \
             is this row's ONLY route, and would falsely declare a chord held for a later unit. \
             Installation follows from neither: both installers key on the `UiAction` \
             classification and never read `status`.",
            binding.status
        );
        assert_eq!(
            binding.command,
            Some(Command::NewWindow),
            "the {NEW_WINDOW_ACTION:?} row carries command {:?}, not Command::NewWindow. \
             `Coordinator::install_app_actions` resolves an `app.` row through its command and \
             nothing else, so a None here means the row is printed and skipped (the menu item \
             renders insensitive) and any other command means the New Window menu item runs \
             something else entirely.",
            binding.command
        );

        // MenuOnly means exactly that: with no chord, an absent menu item leaves
        // only the palette row and `:WolfNewWindow`.
        let listed = MENU_SECTIONS
            .iter()
            .flat_map(|section| section.items)
            .any(|item| item.action == NEW_WINDOW_ACTION);
        assert!(
            listed,
            "the {NEW_WINDOW_ACTION:?} row is MenuOnly but no MENU_SECTIONS item references it. \
             It has no accelerator by design, so the primary menu is the only pointing-device \
             route to a second window — what remains is the command palette and `:WolfNewWindow`, \
             neither of which a user discovers by looking. \
             `menu_items_reference_declared_actions` checks the other direction and cannot see \
             this."
        );
    }
}
