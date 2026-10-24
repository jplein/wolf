# `crates/wolf-gnome/src/editor_inset.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Fractional strip height

Row H is the only row whose rect is not a whole number of pixels, and the only one that reaches this module's own `f64 -> i32` conversion. Rounding instead of flooring would carry 607.5 to 608, and 608 / 16 is exactly 38: a whole extra row of buffer text from half a pixel of strip.

The strip height production passes is integral by construction: `statusbar_layout`'s `height` is two `ceil`ed line boxes plus the vertical padding twice and the line gap once, all whole numbers. So the row pins the seam's contract, that the conversion floors whatever it is handed, rather than a state production reaches. The seam takes `f64`, and a scaled or dpi-converted strip height must find that contract already true.
