//! Thin entry point for the `wolf-gnome` windowed application.
//!
//! All wiring lives in [`wolf_gnome::app`]; `main` only forwards the process exit
//! code from the GTK/Adwaita run loop. The binary takes NO command-line arguments
//! (files are opened from within Neovim via `:e`).

fn main() -> gtk::glib::ExitCode {
    wolf_gnome::app::run()
}
