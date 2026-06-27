//! `wolf-gnome` — the GTK4/Adwaita front-end for Wolf.
//!
//! Tier-1 scope: only the two pure, widget-free *seams* that can be verified
//! headlessly (no display server, no window):
//!
//! - [`gdk_key`]: translates a GDK keyval + modifier state into a toolkit-neutral
//!   [`wolf_core::KeyEvent`], honoring the shift-glyph contract.
//! - [`paint`]: paints a [`wolf_core::Snapshot`] onto a Cairo context (an offscreen
//!   `ImageSurface` in tests, a `DrawingArea` context in production).
//!
//! The windowed application (`ApplicationWindow`, `DrawingArea` wiring, the run
//! loop, wake marshalling, dialogs, theming, mouse/scroll) is deliberately
//! deferred. Keeping the seams as free functions independent of any widget is what
//! makes them testable without a display.

pub mod input;
pub mod render;

pub use input::gdk_key;
pub use render::{paint, CellMetrics};
