//! The GTK GLUE for crash/stall recovery: the full-bleed opaque overlay carrying
//! the "Restart Neovim" button, and the small transient "Neovim was restarted"
//! banner.
//!
//! Every DECISION this module acts on is made in the pure
//! [`mod@crate::liveness_ui`] seam (which mode to show, what it says, where the
//! button sits); this file only builds widgets and toggles visibility. It mirrors
//! [`crate::picker`] / [`crate::tree_view`]: an `Rc<CrashInner>` on the single
//! GTK main thread, `Cell` interior mutability, and a `Weak` upgrade inside every
//! closure so the widget tree never owns the state that owns it.
//!
//! ## Two overlays, deliberately not one
//!
//! - The **crash/stall overlay** ([`CrashOverlay::widget`]) is FULL-BLEED and
//!   OPAQUE. Its whole point is to HIDE the frozen last frame of a broken editor,
//!   so a user can tell "Neovim died" from "Neovim is idle". It swallows mouse
//!   clicks and never takes keyboard focus.
//! - The **banner** ([`CrashOverlay::banner_widget`]) is a small, transient,
//!   self-dismissing notice shown OVER a WORKING editor. It is INDEPENDENT of
//!   liveness: a successful restart makes the fresh nvim `Alive`, whose wake
//!   hides the crash overlay immediately, so without a separate channel the user
//!   would see nothing at all. Only the restart SUCCESS path flashes it.
//!
//! Mirrors macOS's `NvimStatusOverlayView` / `NvimRestartedBannerView` pair.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::liveness_ui::{
    overlay_message, OverlayMode, BANNER_VISIBLE_SECS, OVERLAY_MESSAGE_HEIGHT, OVERLAY_SPACING,
    RESTARTED_BANNER_MESSAGE, RESTART_BUTTON_HEIGHT, RESTART_BUTTON_LABEL, RESTART_BUTTON_WIDTH,
};

/// Gap (px) between the top of the editor area and the top of the banner.
/// Mirrors macOS's `NvimRestartedBannerView.topMargin`.
const BANNER_TOP_MARGIN: i32 = 12;

/// Padding (px) around the banner's label, giving the `osd` pill its body.
/// Mirrors macOS's `NvimRestartedBannerView.labelPadding`.
const BANNER_LABEL_PADDING: i32 = 8;

/// The crash overlay's shared, interior-mutable state. All access happens on the
/// single GTK main thread, so `Cell` (not a `Mutex`) is sufficient.
struct CrashInner {
    /// The full-bleed opaque overlay child (a vertical `gtk::Box`).
    overlay: gtk::Box,
    /// The overlay's message label, retargeted by [`CrashOverlay::set_mode`].
    message: gtk::Label,
    /// The "Restart Neovim" button.
    button: gtk::Button,
    /// The transient, click-through "Neovim was restarted" banner.
    banner: gtk::Box,
    /// The mode currently applied, so [`CrashOverlay::set_mode`] can report
    /// whether it CHANGED (the shell's focus-return and picker-close actions must
    /// fire on the transition only).
    mode: Cell<OverlayMode>,
    /// Monotonic token identifying the latest [`CrashOverlay::flash_banner`].
    /// A scheduled hide only fires while its token is still current, so rapid
    /// restarts coalesce into ONE continuous banner rather than a premature hide.
    /// Exactly macOS's `flashGeneration` scheme.
    flash_generation: Cell<u64>,
}

/// A cloneable handle to the crash/stall overlay and its banner.
#[derive(Clone)]
pub struct CrashOverlay {
    inner: Rc<CrashInner>,
}

/// Builds the crash/stall overlay and its banner. Both start HIDDEN; the shell
/// reveals them via [`CrashOverlay::set_mode`] / [`CrashOverlay::flash_banner`].
///
/// Both widgets are meant to be added with `gtk::Overlay::add_overlay` — the
/// overlay child AFTER the empty state and the picker (a crashed core must cover
/// both: the picker's Open and the empty state's Open Folder would each send into
/// a dead nvim), and the banner LAST so it composites over everything.
///
/// ## The overlay's internal layout is DERIVABLE, and that is the point
///
/// ```text
/// [ gtk::Box spacer, vexpand ]
/// [ gtk::Label   — OVERLAY_MESSAGE_HEIGHT tall, OVERLAY_SPACING bottom margin ]
/// [ gtk::Button  — RESTART_BUTTON_WIDTH x RESTART_BUTTON_HEIGHT ]
/// [ gtk::Box spacer, vexpand ]
/// ```
///
/// Two `vexpand` spacers with natural size 0 split `GtkBox`'s leftover equally,
/// so the fixed block is exactly vertically centred and
/// [`crate::restart_button_rect`] describes the button's real position to within
/// GTK's ±1 px odd-leftover rounding.
///
/// **Do NOT substitute an `adw::StatusPage` here** (as
/// [`crate::shell::build_empty_state`] uses), and do NOT give the `gtk::Box` a
/// non-zero spacing. A status page's internal padding is theme-owned and not
/// derivable from anything this crate declares, and `gtk::Box` spacing applies
/// between EVERY pair of children (including the two spacers), so either change
/// silently invalidates `restart_button_rect` — and with it the pixel that
/// `crash_overlay_and_restart_button_recover_under_cage` in
/// `crates/wolf-gnome/tests/smoke_headless.rs` clicks. The gap between the
/// message and the button lives in the LABEL's `margin_bottom`, deliberately.
#[must_use]
pub fn build() -> CrashOverlay {
    // Spacing 0 — see the doc comment above. The gap is the label's margin.
    let overlay = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // The same opacity mechanism `shell::build_empty_state` uses: this is what
    // makes the overlay hide the frozen last frame instead of tinting it.
    overlay.add_css_class("background");
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    overlay.set_halign(gtk::Align::Fill);
    overlay.set_valign(gtk::Align::Fill);

    let top_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_spacer.set_vexpand(true);

    // No size-changing class or icon: a larger font would move the button off `restart_button_rect`.
    // The hidden initial text is a real message so a stray reveal never shows a blank panel.
    let message = gtk::Label::new(overlay_message(OverlayMode::Stalled));
    message.set_halign(gtk::Align::Center);
    message.set_vexpand(false);
    message.set_size_request(-1, OVERLAY_MESSAGE_HEIGHT);
    message.set_margin_bottom(OVERLAY_SPACING);

    let button = gtk::Button::builder()
        .label(RESTART_BUTTON_LABEL)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(false)
        .build();
    button.set_size_request(RESTART_BUTTON_WIDTH, RESTART_BUTTON_HEIGHT);
    button.add_css_class("suggested-action");
    button.add_css_class("pill");

    let bottom_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom_spacer.set_vexpand(true);

    overlay.append(&top_spacer);
    overlay.append(&message);
    overlay.append(&button);
    overlay.append(&bottom_spacer);
    overlay.set_visible(false);

    let banner = build_banner();

    CrashOverlay {
        inner: Rc::new(CrashInner {
            overlay,
            message,
            button,
            banner,
            mode: Cell::new(OverlayMode::Hidden),
            flash_generation: Cell::new(0),
        }),
    }
}

/// Builds the transient "Neovim was restarted" banner: a top-centred `osd` pill
/// that starts hidden and NEVER intercepts a click.
///
/// `set_can_target(false)` is the GTK equivalent of macOS's `hitTest -> nil`. The
/// banner has no controls of its own and sits over a WORKING editor for several
/// seconds, so a click landing in its rect belongs to the pane beneath it.
fn build_banner() -> gtk::Box {
    let label = gtk::Label::new(Some(RESTARTED_BANNER_MESSAGE));
    label.set_margin_top(BANNER_LABEL_PADDING);
    label.set_margin_bottom(BANNER_LABEL_PADDING);
    label.set_margin_start(BANNER_LABEL_PADDING);
    label.set_margin_end(BANNER_LABEL_PADDING);

    let banner = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    banner.add_css_class("osd");
    banner.set_halign(gtk::Align::Center);
    banner.set_valign(gtk::Align::Start);
    banner.set_margin_top(BANNER_TOP_MARGIN);
    banner.append(&label);
    banner.set_can_target(false);
    banner.set_visible(false);
    banner
}

impl CrashOverlay {
    /// The full-bleed opaque overlay child, for `gtk::Overlay::add_overlay`.
    ///
    /// It deliberately does NOT call `set_can_target(false)`: swallowing pointer
    /// events is half its job. A visible, allocated overlay child takes the
    /// pointer instead of the pane `DrawingArea`s beneath it (the same mechanism
    /// `pane_view`'s module doc records for the picker and the empty state), so a
    /// stray click on a crashed editor cannot reach a dead nvim.
    #[must_use]
    pub fn widget(&self) -> gtk::Widget {
        self.inner.overlay.clone().upcast::<gtk::Widget>()
    }

    /// The transient, click-through banner child, for `gtk::Overlay::add_overlay`.
    /// Add it LAST so it composites over every other overlay child.
    #[must_use]
    pub fn banner_widget(&self) -> gtk::Widget {
        self.inner.banner.clone().upcast::<gtk::Widget>()
    }

    /// Applies `mode` (message + visibility). Returns `true` iff the mode actually
    /// CHANGED.
    ///
    /// The return value is load-bearing, not a convenience: the shell's
    /// picker-close and focus-return actions must fire on the TRANSITION only.
    /// Firing them on every `Alive` wake would steal focus from the picker
    /// continuously.
    pub fn set_mode(&self, mode: OverlayMode) -> bool {
        if self.inner.mode.get() == mode {
            return false;
        }
        self.inner.mode.set(mode);
        match overlay_message(mode) {
            Some(message) => {
                self.inner.message.set_label(message);
                self.inner.overlay.set_visible(true);
            }
            None => self.inner.overlay.set_visible(false),
        }
        true
    }

    /// Installs the "Restart Neovim" button's click handler.
    pub fn connect_restart<F: Fn() + 'static>(&self, on_restart: F) {
        self.inner
            .button
            .connect_clicked(move |_button| on_restart());
    }

    /// Reveals the banner and schedules its auto-dismiss
    /// [`BANNER_VISIBLE_SECS`] later.
    ///
    /// Bumps a monotonic generation and captures it; the delayed hide only runs
    /// while that token is still current, so a newer flash supersedes an older
    /// pending hide and rapid restarts coalesce into one continuous banner.
    pub fn flash_banner(&self) {
        let generation = self.inner.flash_generation.get().wrapping_add(1);
        self.inner.flash_generation.set(generation);
        self.inner.banner.set_visible(true);

        let weak = Rc::downgrade(&self.inner);
        glib::timeout_add_local_once(Duration::from_secs_f64(BANNER_VISIBLE_SECS), move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if inner.flash_generation.get() == generation {
                inner.banner.set_visible(false);
            }
        });
    }

    /// Whether the toplevel's focus widget is the overlay itself or anything under
    /// it (i.e. the restart button).
    ///
    /// The shell uses this on a visible -> hidden transition to park focus back on
    /// the persistent cmdline strip. Nothing here ever calls `grab_focus()` on the
    /// button: a focusable widget inside a container that is later HIDDEN is
    /// precisely the wedge `click_then_close_keeps_keyboard_input_alive_under_cage`
    /// guards against — GTK does not hand focus back when the focus widget goes
    /// away, and the toplevel is left with none.
    ///
    /// `false` when the overlay is not rooted (no window yet), which is the case
    /// for every call before `present()`.
    #[must_use]
    pub fn focus_is_inside(&self) -> bool {
        let Some(root) = self.inner.overlay.root() else {
            return false;
        };
        let Some(focus) = root.focus() else {
            return false;
        };
        focus.is_ancestor(&self.inner.overlay)
            || &focus == self.inner.overlay.upcast_ref::<gtk::Widget>()
    }
}
