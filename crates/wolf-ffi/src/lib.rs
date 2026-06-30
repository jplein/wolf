//! `wolf-ffi` — the swift-bridge FFI surface over [`wolf_core`].
//!
//! This crate is the seam between the platform-agnostic Rust core and the macOS
//! (AppKit) front-end. It is built as a `staticlib` (the archive the `.app`
//! links, through the generated swift-bridge glue) and as an `rlib` (so the Rust
//! integration test in `tests/bridge_smoke.rs` can link and drive the same
//! surface headlessly, with no Swift present).
//!
//! ## Surface
//!
//! - [`WolfCoreHandle`] — an opaque, main-thread-only handle wrapping a
//!   `RefCell<Option<WolfCore>>`. [`WolfCore`](wolf_core::WolfCore) is not `Send`,
//!   so the handle and all its methods are used only on the AppKit main thread.
//! - [`FfiFrame`] — an opaque, per-frame snapshot of the active grid, with the
//!   highlight table already resolved in Rust (never re-resolved in Swift). Scalar
//!   accessors plus one flat [`FfiFrame::cells`] handoff (row-major).
//! - [`FfiKeyEvent`] — a transparent struct Swift fills from an `NSEvent`; mapped
//!   here into a [`wolf_core::KeyEvent`] so nvim notation stays the single source
//!   of truth in core.
//! - [`wolf_ffi_start`] / [`WolfCoreHandle::read_frame`] /
//!   [`WolfCoreHandle::send_key`] / [`WolfCoreHandle::resize`] /
//!   [`WolfCoreHandle::shutdown`].
//!
//! ## Wake
//!
//! Core fires a [`WakeFn`](wolf_core::WakeFn) on each `flush`; the front-end must
//! marshal it onto the AppKit main thread (`DispatchQueue.main.async`) and trigger
//! a repaint. Rather than an `extern "Swift"` symbol — which a Rust-only
//! integration test could never resolve at link time — the wake is a globally
//! **registered** `extern "C"` function pointer ([`wolf_ffi_set_wake`]). Swift's
//! `WakeBridge` registers its dispatch-to-main thunk once at startup; the Rust
//! test never registers, so the wake is a harmless no-op there and the staticlib
//! and the rlib both link with no Swift runtime present.

// swift-bridge's `#[swift_bridge::bridge]` expansion emits same-type raw-pointer
// casts in the generated FFI shims (e.g. `*mut FfiFrame -> *mut FfiFrame`); a
// module-level allow does not reach those sibling items, so this one lint is
// allowed crate-wide. The hand-written glue performs no such casts.
#![allow(clippy::unnecessary_cast)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

use wolf_core::{
    rank_matches, CoreConfig, CursorShape, FileSearch, InputEvent, Key, KeyEvent, ResolvedStyle,
    Rgb, WakeFn, WolfCore,
};

// Re-export the bridge-defined transparent structs at the crate root so the Rust
// integration test (and any other rlib consumer) can name
// `wolf_ffi::{FfiColor, FfiKeyEvent}` directly. The opaque `FfiFrame`, `FfiCell`,
// and `WolfCoreHandle`, plus `wolf_ffi_start`, are already crate-root `pub` items
// defined below.
pub use ffi::{FfiColor, FfiKeyEvent};

/// `FfiKeyEvent.kind` discriminants. These mirror the variant order of
/// [`wolf_core::Key`] (Char, Enter, Esc, Tab, Backspace, Delete, Left, Right, Up,
/// Down, Home, End, PageUp, PageDown, F) and MUST match the Swift `Kind` enum in
/// `WolfKeyMap` and `KeyMapTests`. They are the single source of truth consumed by
/// [`key_event_from_ffi`].
mod kind {
    /// A printable character (the glyph rides in `FfiKeyEvent::ch`).
    pub const CHAR: u8 = 0;
    /// The Enter / Return key.
    pub const ENTER: u8 = 1;
    /// The Escape key.
    pub const ESC: u8 = 2;
    /// The Tab key.
    pub const TAB: u8 = 3;
    /// The Backspace key.
    pub const BACKSPACE: u8 = 4;
    /// The forward-delete key.
    pub const DELETE: u8 = 5;
    /// The Left arrow.
    pub const LEFT: u8 = 6;
    /// The Right arrow.
    pub const RIGHT: u8 = 7;
    /// The Up arrow.
    pub const UP: u8 = 8;
    /// The Down arrow.
    pub const DOWN: u8 = 9;
    /// The Home key.
    pub const HOME: u8 = 10;
    /// The End key.
    pub const END: u8 = 11;
    /// The Page Up key.
    pub const PAGE_UP: u8 = 12;
    /// The Page Down key.
    pub const PAGE_DOWN: u8 = 13;
    /// A function key (number in `FfiKeyEvent::fnum`).
    pub const FUNCTION: u8 = 14;
}

/// Cell style flag bits packed into [`FfiCell::flags`].
mod flag {
    /// Bold text.
    pub const BOLD: u8 = 1 << 0;
    /// Italic text.
    pub const ITALIC: u8 = 1 << 1;
    /// Underlined text.
    pub const UNDERLINE: u8 = 1 << 2;
    /// Curly underline.
    pub const UNDERCURL: u8 = 1 << 3;
    /// Strikethrough text.
    pub const STRIKETHROUGH: u8 = 1 << 4;
}

/// The cursor-shape discriminants packed into [`FfiFrame`]'s `cursor_shape`,
/// mirroring [`wolf_core::CursorShape`].
mod cursor_shape {
    /// A full-cell block cursor.
    pub const BLOCK: u8 = 0;
    /// A horizontal bar cursor.
    pub const HORIZONTAL: u8 = 1;
    /// A vertical bar cursor.
    pub const VERTICAL: u8 = 2;
}

// swift-bridge's generated module expands into code that does not satisfy this
// crate's stricter lints; scope a narrow allow to the bridge module only. The
// hand-written glue below the module is held to the normal bar.
#[allow(clippy::all, missing_docs, unused)]
#[swift_bridge::bridge]
mod ffi {
    // An RGB color plus a `set` flag, mirroring `Option<wolf_core::Rgb>` across
    // the bridge (`set = false` means "unset — use a fallback").
    #[swift_bridge(swift_repr = "struct")]
    struct FfiColor {
        // Packed `0x00RRGGBB`.
        rgb: u32,
        // Whether this color is set (vs. a fallback should apply).
        set: bool,
    }

    // A structured key event from Swift, mapped in Rust into a
    // `wolf_core::KeyEvent`. The `kind` field discriminates the logical key; for
    // `kind == Char`, `ch` is the (already-shifted) Unicode scalar.
    #[swift_bridge(swift_repr = "struct")]
    struct FfiKeyEvent {
        kind: u8,
        ch: u32,
        fnum: u8,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    }

    extern "Rust" {
        // Starts the core against the pinned Neovim (None on error). `init_lua`
        // and `cwd` are filesystem PATHS.
        fn wolf_ffi_start(
            init_lua: String,
            cwd: String,
            cols: u16,
            rows: u16,
        ) -> Option<WolfCoreHandle>;
    }

    extern "Rust" {
        type WolfCoreHandle;

        fn read_frame(&self) -> FfiFrame;
        fn send_key(&self, ev: FfiKeyEvent);
        fn resize(&self, cols: u16, rows: u16);
        fn shutdown(&self);
        // Starts a streaming file-search session over the core's cwd, fed by the
        // registered file-search wake. Infallible: returns an empty-backed handle
        // when the core is gone (mirroring `read_frame`'s empty-frame fallback).
        fn start_file_search(&self) -> FfiFileSearch;
        // Starts a SEPARATE long-lived streaming file-search session for the
        // file-tree sidebar, fed by the registered TREE wake. Same infallible
        // empty-backed fallback as `start_file_search`.
        fn start_tree_search(&self) -> FfiFileSearch;
        // The absolute path of the buffer currently being edited (canonicalized
        // by core), or the empty string for an unnamed buffer / shut-down core.
        fn current_file(&self) -> String;
        // Opens `path` (an absolute filesystem path) in Neovim via
        // `InputEvent::Open`; the core's input pump records the MRU.
        fn open(&self, path: String);
    }

    extern "Rust" {
        type FfiFileSearch;

        // The top `max` ranked entries for `query` against the latest streamed
        // search snapshot. An empty `query` lists all entries MRU-first. `FfiFileEntry`
        // is opaque so a `Vec<FfiFileEntry>` is supported (same as `FfiCell`).
        fn ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry>;
        // The IMMEDIATE children of `dir` ("" = the cwd root) projected from the
        // latest streamed search snapshot: directories-first then files (each
        // case-insensitive by name). `FfiTreeNode` is opaque so a
        // `Vec<FfiTreeNode>` is supported.
        fn children_of(&self, dir: String) -> Vec<FfiTreeNode>;
        // Maps an absolute path to its tree-display rel-path (EXACT entry-path
        // match), or the empty string when no streamed entry matches.
        fn rel_path_for(&self, abs: String) -> String;
    }

    extern "Rust" {
        type FfiFileEntry;

        // The display string (path relative to the search root) shown + ranked.
        fn display(&self) -> String;
        // The absolute filesystem path, handed back to `WolfCoreHandle::open`.
        fn path(&self) -> String;
    }

    extern "Rust" {
        type FfiTreeNode;

        // The immediate component name (the last segment of `rel_path`).
        fn name(&self) -> String;
        // The full relative, `/`-separated path from the cwd root to this node.
        fn rel_path(&self) -> String;
        // The absolute filesystem path for a File node; the empty string for a
        // Dir node.
        fn abs_path(&self) -> String;
        // Whether this node is a directory (vs. a leaf file).
        fn is_dir(&self) -> bool;
    }

    extern "Rust" {
        // The ancestor directory rel-paths of `rel_path`, from the cwd root down
        // to the node's parent (`"src/main.rs"` -> `["src"]`, `"README.md"` ->
        // `[]`). A free fn (no session needed) delegating to `wolf_core::tree`.
        fn wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String>;
    }

    extern "Rust" {
        type FfiFrame;

        fn cols(&self) -> u16;
        fn rows(&self) -> u16;
        fn cursor_row(&self) -> u16;
        fn cursor_col(&self) -> u16;
        fn cursor_shape(&self) -> u8;
        fn cursor_cell_percentage(&self) -> u32;
        fn cursor_visible(&self) -> bool;
        fn default_fg(&self) -> FfiColor;
        fn default_bg(&self) -> FfiColor;
        // Reverse-video cursor colors (so the cursor is reliably visible). Two
        // scalar accessors rather than a `Vec<FfiColor>`: swift-bridge does not
        // emit Vec support for transparent structs.
        fn cursor_fill(&self) -> FfiColor;
        fn cursor_glyph(&self) -> FfiColor;
        // The flat, row-major grid of resolved cells (`cols * rows` entries).
        // `FfiCell` is opaque (Vec support is generated for opaque types).
        fn cells(&self) -> Vec<FfiCell>;
    }

    extern "Rust" {
        type FfiCell;

        // The displayed text (one grapheme cluster), empty for a wide-char
        // trailing placeholder or a never-painted cell.
        fn text(&self) -> String;
        // Resolved foreground color.
        fn fg(&self) -> FfiColor;
        // Resolved background color.
        fn bg(&self) -> FfiColor;
        // Packed style flags (bold/italic/underline/undercurl/strikethrough).
        fn flags(&self) -> u8;
    }
}

// ---------------------------------------------------------------------------
// Wake registration (Swift-free; see the crate docs).
// ---------------------------------------------------------------------------

/// The C ABI of the front-end wake callback.
type WakeCallback = extern "C" fn();

/// The globally-registered wake callback, or null when none is registered (the
/// Rust integration test path). Stored as a raw pointer in an [`AtomicPtr`].
static WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end wake callback.
///
/// Swift's `WakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … setNeedsDisplay … }`. Declared `extern "C"` with
/// a hand-written C header (`generated/wolf_ffi_wake.h`) rather than through the
/// swift-bridge module so there is no `extern "Swift"` symbol for a Rust-only
/// build to leave unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn()` that is safe to call from
/// any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on flush: it loads the registered callback
/// and calls it, or does nothing if none is registered. Captures no state, so it
/// is `Send + Sync` as `WakeFn` requires.
fn registered_wake() -> WakeFn {
    Arc::new(|| {
        let ptr = WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn()` by
            // `wolf_ffi_set_wake`; a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f();
        }
    })
}

/// The globally-registered file-search wake callback, or null when none is
/// registered (the Rust integration-test path). Stored as a raw pointer in an
/// [`AtomicPtr`], exactly like [`WAKE_FN`] but for the picker's traversal-batch
/// wakes.
static FILESEARCH_WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end's file-search wake callback.
///
/// Swift's `PickerWakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … picker.refresh() … }`. A second registered
/// `extern "C"` callback alongside [`wolf_ffi_set_wake`], declared in the
/// hand-written `generated/wolf_ffi_wake.h`, so a Rust-only build leaves no
/// `extern "Swift"` symbol unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn()` that is safe to call from
/// any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_filesearch_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    FILESEARCH_WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on each file-search traversal batch: it
/// loads the registered file-search callback and calls it, or does nothing if
/// none is registered. Captures no state, so it is `Send + Sync` as `WakeFn`
/// requires.
fn registered_filesearch_wake() -> WakeFn {
    Arc::new(|| {
        let ptr = FILESEARCH_WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn()` by
            // `wolf_ffi_set_filesearch_wake`; a non-null value is always such a
            // pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f();
        }
    })
}

/// The globally-registered file-TREE wake callback, or null when none is
/// registered (the Rust integration-test path). Stored as a raw pointer in an
/// [`AtomicPtr`], a third sibling of [`WAKE_FN`] / [`FILESEARCH_WAKE_FN`] feeding
/// the sidebar's own long-lived [`FileSearch`] traversal-batch wakes.
static TREE_WAKE_FN: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Registers (or, with `None`, clears) the front-end's file-TREE wake callback.
///
/// Swift's `TreeWakeBridge` calls this once at startup with a thunk that does
/// `DispatchQueue.main.async { … tree.refresh() … }`. A third registered
/// `extern "C"` callback alongside [`wolf_ffi_set_wake`] and
/// [`wolf_ffi_set_filesearch_wake`], declared in the hand-written
/// `generated/wolf_ffi_wake.h`, so a Rust-only build leaves no `extern "Swift"`
/// symbol unresolved.
///
/// # Safety
///
/// `wake`, if `Some`, must be a valid `extern "C" fn()` that is safe to call from
/// any thread (core fires the wake from its tokio worker thread).
#[no_mangle]
pub extern "C" fn wolf_ffi_set_tree_wake(wake: Option<WakeCallback>) {
    let ptr = wake.map_or(std::ptr::null_mut(), |f| f as *mut ());
    TREE_WAKE_FN.store(ptr, Ordering::Release);
}

/// Builds the [`WakeFn`] core invokes on each file-tree traversal batch: it loads
/// the registered tree callback and calls it, or does nothing if none is
/// registered. Captures no state, so it is `Send + Sync` as `WakeFn` requires.
fn registered_tree_wake() -> WakeFn {
    Arc::new(|| {
        let ptr = TREE_WAKE_FN.load(Ordering::Acquire);
        if !ptr.is_null() {
            // SAFETY: `ptr` was set from a valid `extern "C" fn()` by
            // `wolf_ffi_set_tree_wake`; a non-null value is always such a pointer.
            let f: WakeCallback = unsafe { std::mem::transmute::<*mut (), WakeCallback>(ptr) };
            f();
        }
    })
}

/// Returns the ancestor directory rel-paths of `rel_path`, from the cwd root down
/// to the node's parent (`"src/main.rs"` -> `["src"]`, `"README.md"` -> `[]`).
///
/// A free FFI fn (no live session required) delegating to
/// [`wolf_core::tree::ancestors_to_reveal`]; the sidebar calls it to learn which
/// ancestor dirs to expand before revealing the current file.
#[must_use]
pub fn wolf_ffi_ancestors_to_reveal(rel_path: String) -> Vec<String> {
    wolf_core::tree::ancestors_to_reveal(&rel_path)
}

// ---------------------------------------------------------------------------
// Opaque handles.
// ---------------------------------------------------------------------------

/// An opaque, main-thread-only handle to a live [`WolfCore`].
///
/// [`WolfCore`] is not `Send`; this handle and every method on it are used only on
/// the AppKit main thread. The interior `RefCell<Option<…>>` lets
/// [`shutdown`](WolfCoreHandle::shutdown) take the core out by value (it consumes
/// `self`) while the bridge only ever hands Swift a `&self`.
pub struct WolfCoreHandle {
    core: RefCell<Option<WolfCore>>,
}

impl WolfCoreHandle {
    /// Reads the current render-state snapshot into an owned [`FfiFrame`].
    #[must_use]
    pub fn read_frame(&self) -> FfiFrame {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return FfiFrame::empty();
        };
        let snapshot = core.read_snapshot();
        FfiFrame::from_snapshot(&snapshot)
    }

    /// Maps `ev` into a [`wolf_core::KeyEvent`] and forwards it to Neovim.
    pub fn send_key(&self, ev: FfiKeyEvent) {
        if let Some(key) = key_event_from_ffi(&ev) {
            if let Some(core) = self.core.borrow().as_ref() {
                core.send_input(InputEvent::Key(key));
            }
        }
    }

    /// Requests a grid resize.
    pub fn resize(&self, cols: u16, rows: u16) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.resize(cols, rows);
        }
    }

    /// Cleanly shuts down the captive Neovim and tears down the runtime.
    ///
    /// Takes the core out of the handle (so a second call is a no-op) and consumes
    /// it via [`WolfCore::shutdown`]; any shutdown error is logged, not returned
    /// (the bridge signature is infallible).
    pub fn shutdown(&self) {
        if let Some(core) = self.core.borrow_mut().take() {
            if let Err(e) = core.shutdown() {
                tracing::warn!("wolf-ffi: core shutdown failed: {e}");
            }
        }
    }

    /// Starts a streaming file-search session over the core's cwd, fed by the
    /// registered file-search wake.
    ///
    /// Infallible across the bridge: when the core has already been shut down it
    /// returns an empty-backed [`FfiFileSearch`] (whose [`FfiFileSearch::ranked`]
    /// yields no entries), mirroring [`read_frame`](WolfCoreHandle::read_frame)'s
    /// empty-frame fallback.
    #[must_use]
    pub fn start_file_search(&self) -> FfiFileSearch {
        let search = self
            .core
            .borrow()
            .as_ref()
            .map(|core| core.start_file_search(registered_filesearch_wake()));
        FfiFileSearch { search }
    }

    /// Starts a SEPARATE long-lived streaming file-search session for the
    /// file-tree sidebar, fed by the registered tree wake.
    ///
    /// Infallible across the bridge, exactly like
    /// [`start_file_search`](WolfCoreHandle::start_file_search): when the core has
    /// already been shut down it returns an empty-backed [`FfiFileSearch`] (whose
    /// projection methods yield nothing). The sidebar owns this session for its
    /// whole lifetime, independent of the transient picker session.
    #[must_use]
    pub fn start_tree_search(&self) -> FfiFileSearch {
        let search = self
            .core
            .borrow()
            .as_ref()
            .map(|core| core.start_file_search(registered_tree_wake()));
        FfiFileSearch { search }
    }

    /// The absolute path of the buffer currently being edited (canonicalized by
    /// core via the `wolf_current_file` notification), or the empty string for an
    /// unnamed buffer or a shut-down core.
    ///
    /// Read on each editor wake by the sidebar to highlight + reveal the current
    /// file.
    #[must_use]
    pub fn current_file(&self) -> String {
        let borrow = self.core.borrow();
        let Some(core) = borrow.as_ref() else {
            return String::new();
        };
        let snapshot = core.read_snapshot();
        snapshot
            .current_file
            .as_ref()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
    }

    /// Opens `path` (an absolute filesystem path) in Neovim via
    /// [`InputEvent::Open`]; the core's input pump records the MRU. A no-op once
    /// the core has been shut down.
    pub fn open(&self, path: String) {
        if let Some(core) = self.core.borrow().as_ref() {
            core.send_input(InputEvent::Open {
                path: PathBuf::from(path),
            });
        }
    }
}

/// Starts the core. `init_lua` and `cwd` are filesystem paths (mirroring
/// [`CoreConfig`]); returns `None` (after logging) on any startup error.
#[must_use]
pub fn wolf_ffi_start(
    init_lua: String,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Option<WolfCoreHandle> {
    let cfg = CoreConfig {
        init_lua: PathBuf::from(init_lua),
        cols,
        rows,
        cwd: PathBuf::from(cwd),
    };
    match WolfCore::start(cfg, registered_wake()) {
        Ok(core) => Some(WolfCoreHandle {
            core: RefCell::new(Some(core)),
        }),
        Err(e) => {
            tracing::error!("wolf-ffi: WolfCore::start failed: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// FfiCell: one resolved grid cell (opaque to Swift).
// ---------------------------------------------------------------------------

/// One resolved grid cell: its text, foreground/background, and style flags.
///
/// Opaque across the bridge (so a `Vec<FfiCell>` is supported — swift-bridge does
/// not emit Vec support for transparent structs). Colors are held as
/// `Option<Rgb>` (keeping the cell `Clone`); Swift reads them via [`FfiCell::fg`]
/// / [`FfiCell::bg`], which build an [`FfiColor`] on demand. The `text` field is
/// `pub` so the Rust integration test can read `cell.text` directly.
#[derive(Clone)]
pub struct FfiCell {
    /// The displayed text (one grapheme cluster), empty for a wide-char trailing
    /// placeholder or a never-painted cell.
    pub text: String,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    flags: u8,
}

impl FfiCell {
    /// The cell's text (one grapheme cluster), empty for a placeholder cell.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    /// The resolved foreground color.
    #[must_use]
    pub fn fg(&self) -> FfiColor {
        FfiColor::from_opt(self.fg)
    }

    /// The resolved background color.
    #[must_use]
    pub fn bg(&self) -> FfiColor {
        FfiColor::from_opt(self.bg)
    }

    /// The packed style flags (bold/italic/underline/undercurl/strikethrough).
    #[must_use]
    pub fn flags(&self) -> u8 {
        self.flags
    }
}

// ---------------------------------------------------------------------------
// FfiFileSearch / FfiFileEntry: the file-picker search surface (opaque to Swift).
// ---------------------------------------------------------------------------

/// A live file-search session handle (opaque to Swift).
///
/// Wraps an `Option<`[`FileSearch`]`>` so [`start_file_search`] stays infallible
/// across the bridge: `None` means the core was already gone at start, and
/// [`ranked`](FfiFileSearch::ranked) then yields no entries. Dropping this handle
/// drops the inner [`FileSearch`], cancelling its background traversal.
pub struct FfiFileSearch {
    search: Option<FileSearch>,
}

impl FfiFileSearch {
    /// The top `max` ranked entries for `query` against the latest streamed
    /// search snapshot.
    ///
    /// An empty (or all-whitespace) `query` lists all known entries MRU-first; a
    /// non-empty `query` is tokenized on whitespace into terms and keeps only
    /// entries where EVERY term is a case-insensitive subsequence (order-
    /// independent), best-first (see [`rank_matches`]). The snapshot read guard is
    /// held only for the duration of the rank + clone and dropped before returning.
    #[must_use]
    pub fn ranked(&self, query: String, max: usize) -> Vec<FfiFileEntry> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let view = search.read();
        let entries: Vec<FfiFileEntry> = rank_matches(&query, &view.entries)
            .into_iter()
            .take(max)
            .map(|m| {
                let entry = &view.entries[m.index];
                FfiFileEntry {
                    path: entry.path.clone(),
                    display: entry.display.clone(),
                }
            })
            .collect();
        entries
    }

    /// The IMMEDIATE children of `dir` (`""` = the cwd root) projected from the
    /// latest streamed search snapshot.
    ///
    /// Delegates to [`wolf_core::tree::children_of`]: directories-first then
    /// files, each case-insensitive by name; dotfiles are listed and gitignored
    /// files are excluded (both already handled by the streaming traversal). The
    /// snapshot read guard is held only for the projection + map and dropped
    /// before returning. Yields an empty `Vec` when the session is absent.
    #[must_use]
    pub fn children_of(&self, dir: String) -> Vec<FfiTreeNode> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let view = search.read();
        wolf_core::tree::children_of(&view.entries, &dir)
            .into_iter()
            .map(FfiTreeNode::from_node)
            .collect()
    }

    /// Maps an absolute path to its tree-display rel-path via an EXACT entry-path
    /// match (see [`wolf_core::tree::rel_path_for`]), or the empty string when no
    /// streamed entry matches (or the session is absent).
    #[must_use]
    pub fn rel_path_for(&self, abs: String) -> String {
        let Some(search) = &self.search else {
            return String::new();
        };
        let view = search.read();
        wolf_core::tree::rel_path_for(&view.entries, std::path::Path::new(&abs))
            .unwrap_or_default()
    }
}

/// One projected tree node (opaque to Swift, so a `Vec<FfiTreeNode>` is supported
/// — swift-bridge emits Vec support only for opaque types).
///
/// Flattens [`wolf_core::tree::TreeNode`]'s two variants into a tagged struct:
/// `is_dir` discriminates, and `abs_path` is the empty string for a `Dir`.
pub struct FfiTreeNode {
    name: String,
    rel_path: String,
    abs_path: Option<PathBuf>,
    is_dir: bool,
}

impl FfiTreeNode {
    /// The immediate component name (the last segment of `rel_path`).
    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The full relative, `/`-separated path from the cwd root to this node.
    #[must_use]
    pub fn rel_path(&self) -> String {
        self.rel_path.clone()
    }

    /// The absolute filesystem path for a File node; the empty string for a Dir
    /// node.
    #[must_use]
    pub fn abs_path(&self) -> String {
        self.abs_path
            .as_ref()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
    }

    /// Whether this node is a directory (vs. a leaf file).
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Flattens a [`wolf_core::tree::TreeNode`] into the bridge-friendly tagged
    /// struct.
    fn from_node(node: wolf_core::tree::TreeNode) -> Self {
        match node {
            wolf_core::tree::TreeNode::Dir { name, rel_path } => Self {
                name,
                rel_path,
                abs_path: None,
                is_dir: true,
            },
            wolf_core::tree::TreeNode::File {
                name,
                rel_path,
                abs_path,
            } => Self {
                name,
                rel_path,
                abs_path: Some(abs_path),
                is_dir: false,
            },
        }
    }
}

/// One ranked file-search result (opaque to Swift, so a `Vec<FfiFileEntry>` is
/// supported — swift-bridge emits Vec support only for opaque types).
pub struct FfiFileEntry {
    path: PathBuf,
    display: String,
}

impl FfiFileEntry {
    /// The display string (the path relative to the search root) shown + ranked.
    #[must_use]
    pub fn display(&self) -> String {
        self.display.clone()
    }

    /// The absolute filesystem path, handed back to [`WolfCoreHandle::open`].
    #[must_use]
    pub fn path(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------------------
// FfiFrame: an owned, resolved snapshot of the active grid.
// ---------------------------------------------------------------------------

/// An opaque, owned, paint-ready frame: the active grid with every cell already
/// resolved against the highlight table in Rust (never re-resolved in Swift).
pub struct FfiFrame {
    cols: u16,
    rows: u16,
    cursor_row: u16,
    cursor_col: u16,
    cursor_shape: u8,
    cursor_cell_percentage: u32,
    cursor_visible: bool,
    default_fg: FfiColor,
    default_bg: FfiColor,
    cursor_fill: FfiColor,
    cursor_glyph: FfiColor,
    cells: Vec<FfiCell>,
}

impl FfiFrame {
    /// Grid width in columns.
    #[must_use]
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Grid height in rows.
    #[must_use]
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Cursor row (grid coordinate).
    #[must_use]
    pub fn cursor_row(&self) -> u16 {
        self.cursor_row
    }

    /// Cursor column (grid coordinate).
    #[must_use]
    pub fn cursor_col(&self) -> u16 {
        self.cursor_col
    }

    /// Cursor shape (0 = Block, 1 = Horizontal, 2 = Vertical).
    #[must_use]
    pub fn cursor_shape(&self) -> u8 {
        self.cursor_shape
    }

    /// Bar-cursor cell coverage percentage (1–100), or 0 if unspecified.
    #[must_use]
    pub fn cursor_cell_percentage(&self) -> u32 {
        self.cursor_cell_percentage
    }

    /// Whether the cursor lives on the active grid (so it should be drawn).
    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// The editor default foreground color.
    #[must_use]
    pub fn default_fg(&self) -> FfiColor {
        self.default_fg.dup()
    }

    /// The editor default background color.
    #[must_use]
    pub fn default_bg(&self) -> FfiColor {
        self.default_bg.dup()
    }

    /// The cursor fill color (reverse-video; ported from wolf-gnome's `cursor.rs`).
    #[must_use]
    pub fn cursor_fill(&self) -> FfiColor {
        self.cursor_fill.dup()
    }

    /// The cursor glyph color (the readable color over the fill).
    #[must_use]
    pub fn cursor_glyph(&self) -> FfiColor {
        self.cursor_glyph.dup()
    }

    /// The flat, row-major grid of resolved cells (`cols * rows` entries).
    #[must_use]
    pub fn cells(&self) -> Vec<FfiCell> {
        self.cells.clone()
    }

    /// An empty `0x0` frame (used when the core has already been shut down).
    fn empty() -> Self {
        Self {
            cols: 0,
            rows: 0,
            cursor_row: 0,
            cursor_col: 0,
            cursor_shape: cursor_shape::BLOCK,
            cursor_cell_percentage: 0,
            cursor_visible: false,
            default_fg: FfiColor::unset(),
            default_bg: FfiColor::unset(),
            cursor_fill: FfiColor::unset(),
            cursor_glyph: FfiColor::unset(),
            cells: Vec::new(),
        }
    }

    /// Builds a frame from a render-state snapshot, resolving the active grid's
    /// cells against the highlight table in Rust.
    ///
    /// Public so the headless `tests/frame_pack.rs` integration test can drive the
    /// snapshot-level packing directly (no live `WolfCore`); `read_frame` keeps
    /// delegating to it.
    #[must_use]
    pub fn from_snapshot(snapshot: &wolf_core::Snapshot) -> Self {
        let active_id = snapshot.active_grid_id();
        let grid = snapshot.active_grid();
        let highlights = &snapshot.highlights;

        let mut cells = Vec::with_capacity(grid.cols * grid.rows);
        for row in 0..grid.rows {
            for col in 0..grid.cols {
                let (text, style) = match grid.cell(row, col) {
                    Some(cell) => (cell.text.clone(), highlights.resolve(cell.hl_id)),
                    None => (String::new(), ResolvedStyle::default()),
                };
                cells.push(FfiCell {
                    text,
                    fg: style.fg,
                    bg: style.bg,
                    flags: pack_flags(&style),
                });
            }
        }

        let (shape, cell_percentage) = snapshot
            .mode_infos
            .get(snapshot.cursor.mode_idx)
            .map_or((CursorShape::default(), None), |m| {
                (m.shape, m.cell_percentage)
            });

        let (fill, glyph) = cursor_colors(snapshot);

        Self {
            cols: clamp_u16(grid.cols),
            rows: clamp_u16(grid.rows),
            cursor_row: clamp_u16(snapshot.cursor.row),
            cursor_col: clamp_u16(snapshot.cursor.col),
            cursor_shape: pack_cursor_shape(shape),
            cursor_cell_percentage: cell_percentage.unwrap_or(0),
            cursor_visible: snapshot.cursor.grid == active_id,
            default_fg: FfiColor::from_opt(highlights.defaults.fg),
            default_bg: FfiColor::from_opt(highlights.defaults.bg),
            cursor_fill: FfiColor::from_rgb(fill),
            cursor_glyph: FfiColor::from_rgb(glyph),
            cells,
        }
    }
}

// ---------------------------------------------------------------------------
// Color / style / key helpers.
// ---------------------------------------------------------------------------

/// Fallback foreground (white) when neither a highlight nor the defaults set one,
/// mirroring wolf-gnome's `render::FALLBACK_FG`.
const FALLBACK_FG: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
/// Fallback background (black), mirroring wolf-gnome's `render::FALLBACK_BG`.
const FALLBACK_BG: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// Bit shift for the red channel when packing an [`Rgb`] into `0x00RRGGBB`.
const RED_SHIFT: u32 = 16;
/// Bit shift for the green channel.
const GREEN_SHIFT: u32 = 8;

impl FfiColor {
    /// An unset color (`set = false`).
    fn unset() -> Self {
        Self { rgb: 0, set: false }
    }

    /// Duplicates this color (swift-bridge transparent structs are not `Copy`).
    fn dup(&self) -> Self {
        Self {
            rgb: self.rgb,
            set: self.set,
        }
    }

    /// Packs an [`Rgb`] into a set [`FfiColor`].
    fn from_rgb(c: Rgb) -> Self {
        Self {
            rgb: (u32::from(c.r) << RED_SHIFT) | (u32::from(c.g) << GREEN_SHIFT) | u32::from(c.b),
            set: true,
        }
    }

    /// Packs an `Option<Rgb>` into an [`FfiColor`] (`None` → unset).
    fn from_opt(c: Option<Rgb>) -> Self {
        c.map_or_else(Self::unset, Self::from_rgb)
    }
}

/// Packs a [`ResolvedStyle`]'s boolean attributes into the [`FfiCell::flags`]
/// bitset.
fn pack_flags(style: &ResolvedStyle) -> u8 {
    let mut f = 0u8;
    if style.bold {
        f |= flag::BOLD;
    }
    if style.italic {
        f |= flag::ITALIC;
    }
    if style.underline {
        f |= flag::UNDERLINE;
    }
    if style.undercurl {
        f |= flag::UNDERCURL;
    }
    if style.strikethrough {
        f |= flag::STRIKETHROUGH;
    }
    f
}

/// Packs a [`CursorShape`] into its `u8` discriminant.
fn pack_cursor_shape(shape: CursorShape) -> u8 {
    match shape {
        CursorShape::Block => cursor_shape::BLOCK,
        CursorShape::Horizontal => cursor_shape::HORIZONTAL,
        CursorShape::Vertical => cursor_shape::VERTICAL,
    }
}

/// Clamps a `usize` grid dimension/coordinate into the `u16` the bridge uses.
fn clamp_u16(v: usize) -> u16 {
    u16::try_from(v).unwrap_or(u16::MAX)
}

/// Computes the cursor's `(fill, glyph)` colors (reverse video, honoring a visible
/// mode cursor highlight), ported from wolf-gnome's `cursor::cursor_colors`.
fn cursor_colors(snap: &wolf_core::Snapshot) -> (Rgb, Rgb) {
    let defaults = &snap.highlights.defaults;

    let (cell_fg, cell_bg) = match snap.active_grid().cell(snap.cursor.row, snap.cursor.col) {
        Some(cell) => {
            let style = snap.highlights.resolve(cell.hl_id);
            (
                style.fg.unwrap_or(FALLBACK_FG),
                style.bg.unwrap_or(FALLBACK_BG),
            )
        }
        None => (
            defaults.fg.unwrap_or(FALLBACK_FG),
            defaults.bg.unwrap_or(FALLBACK_BG),
        ),
    };

    if let Some(attr_id) = snap
        .mode_infos
        .get(snap.cursor.mode_idx)
        .and_then(|mode| mode.attr_id)
    {
        let style = snap.highlights.resolve(attr_id);
        if let (Some(f), Some(b)) = (style.fg, style.bg) {
            if b != cell_bg && b != f {
                return (b, f);
            }
        }
    }

    (cell_fg, cell_bg)
}

/// Maps an [`FfiKeyEvent`] into a [`wolf_core::KeyEvent`], or `None` if the event
/// carries no logical key (e.g. an unrecognized `kind`).
///
/// `kind` discriminants are the single source of truth shared with the Swift
/// `WolfKeyMap`; see the [`kind`] module.
fn key_event_from_ffi(ev: &FfiKeyEvent) -> Option<KeyEvent> {
    let key = match ev.kind {
        kind::CHAR => Key::Char(char::from_u32(ev.ch)?),
        kind::ENTER => Key::Enter,
        kind::ESC => Key::Esc,
        kind::TAB => Key::Tab,
        kind::BACKSPACE => Key::Backspace,
        kind::DELETE => Key::Delete,
        kind::LEFT => Key::Left,
        kind::RIGHT => Key::Right,
        kind::UP => Key::Up,
        kind::DOWN => Key::Down,
        kind::HOME => Key::Home,
        kind::END => Key::End,
        kind::PAGE_UP => Key::PageUp,
        kind::PAGE_DOWN => Key::PageDown,
        kind::FUNCTION => Key::F(ev.fnum),
        _ => return None,
    };
    Some(KeyEvent {
        key,
        ctrl: ev.ctrl,
        shift: ev.shift,
        alt: ev.alt,
        meta: ev.meta,
    })
}
