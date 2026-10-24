# `crates/wolf-gnome/tests/paint_golden.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Measured cell height

`pangocairo::functions::context_get_resolution` returns -1.0 on a context created over an `ImageSurface`, which is measured rather than inferred, so `for_font`'s 96 dpi arm is the live path on every headless run and not a defensive guard. Every other cell-metric assertion in the file is relative (taller than, equal to, centred within) and would hold at any dpi applied consistently, so this test is the only one that pins it.

`SMOKE_PROBE_CELL_H` is measured for the pinned DejaVu Sans Mono at 28 pt and 118%, and it is also that face's `ascent + descent` at that size. A change means one of two things with different fixes: the dpi fallback stopped firing (a context that reports a real resolution, or a changed fallback condition), or the pinned font moved (a different `FONTCONFIG_FILE`, or a nixpkgs bump of `dejavu_fonts`). Re-measure before touching the literal.

## Separate face fixture

The face cases get a third fixture and render helper. The decoration cases are calibrated against a band below the baseline of a row of `x`, while the face cases use whole-cell ink counts and a left edge, a different glyph and a different band. Keeping the fixtures apart also keeps the falsifiers' evidence apart: a decoration leaking forward would reach face cells appended to the decoration fixture, so one red run would indict two independent pieces of painter state.

The pixel reader, the band helpers, `ink_count` and the colours are shared. `ink_count` measures against `DEFAULT_BG`, which is why the face fixture must keep `DEFAULT_FG` on `DEFAULT_BG`.
