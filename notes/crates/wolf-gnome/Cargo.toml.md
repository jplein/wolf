# `crates/wolf-gnome/Cargo.toml`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## gtk-rs release set

The gtk4-rs crates (gtk4, gdk4, gsk4) and the gtk-rs core crates (cairo-rs, pango, pangocairo, glib) release together but carry different minor numbers: gdk4 0.11 depends on the core crates at 0.22. The invariant is one coherent release set, not one identical minor, and mismatched `-sys` minors fail to link.

`gdk`'s `v4_8` feature exposes `gdk::ScrollUnit`, which `mouse::scroll_threshold` names in its public signature. gtk4's `v4_12` already turns it on transitively, but a type this crate exposes should not depend on another crate's feature graph. `Cargo.lock` does not record features, so the pin moves no lock file and needs no `CARGO_BAZEL_REPIN`.

## webkit6 features

webkit6 0.6.1 requires gtk4 ^0.11, gdk4 ^0.11, glib ^0.22 and gio ^0.22, which the workspace lock already satisfies, so it joins the graph without moving an existing version. It is unconditional, like gtk, adw and cairo: gtk4-sys already sits unconditionally in `Cargo.Bazel.lock` with a green macOS build, so a cfg gate would buy nothing and split the dependency graph.

`v2_52` matches the flake's pinned webkitgtk (2.52.4, ABI 6.0), the ABI the devShell's `webkitgtk-6.0.pc` describes, so it must not rise past what the flake pins. `gtk_v4_18` stays off because it would drag in gtk4's `v4_18` behind this crate's deliberate `v4_12` pin.

webkit6 names `gtk::Accessible` unconditionally in `WebView` and `WebViewBase`, and gtk4 gates that type behind `v4_10` from 0.11.4 onward, so the `^0.11` requirement admits a gtk4 where the binding depends on the feature. The `v4_12` on gtk satisfies it either way. Trimming gtk4's feature below `v4_10` breaks the build with an error that points at neither the feature nor webkit, measured against gtk4 0.11.4:

```text
error[E0425]: cannot find type `Accessible` in crate `gtk`
  --> webkit6-0.6.1/src/auto/web_view.rs:27:129
  --> webkit6-0.6.1/src/auto/web_view_base.rs:10:128
```
