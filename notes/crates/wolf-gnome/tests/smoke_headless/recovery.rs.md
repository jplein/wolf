# `crates/wolf-gnome/tests/smoke_headless/recovery.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Restart button click point

`gtk::Widget::set_size_request` sets a minimum and the button is `halign(Center)`, so a builder requesting too little still renders a natural-width pill centred on the rect's centre. A builder requesting `RESTART_BUTTON_WIDTH` (320) renders x 605..924; one requesting 40 renders x 669..861 (193 px). Both are centred on x 765, so a centre click hits both, and even the 3/4 point (x 845) hits the small one.

A tenth in from the left edge puts the click at x 638: 33 px inside the correct button and 31 px clear of the natural-width one. The rect agrees with the render to within half a pixel (x 605..925, y 392..456 against a derived x 605.5..925.5, y 391.5..455.5), and the anchor is the overlay's own measured origin, so the derivation error is far below either margin.

As a bound: the button spans `[765 - W/2, 765 + W/2]` with `W = max(requested, natural ~193)`, and a click at x 638 misses it if and only if `W < 254`. It catches a builder width below about 254 px, the 40 px revert included, but widths between 254 and 320 go undiscriminated.

Y stays at the rect's vertical centre. The vertical derivation, two `vexpand` spacers splitting the leftover, is the harder half and is most robust there, and no documented revert turns on the button's height, the other axis this click leaves undiscriminated.
