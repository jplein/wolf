# `crates/wolf-gnome/src/preview_coalescer.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Test clock and constants

Every test drives the trailing timer by hand: a request hands out a generation, the test calls `fire` with it as the armed `glib` one-shot would, and calls `rendered` where the render pass's last statement would. Nothing sleeps and no clock is read twice, because every `Instant` in the module is built from one `Instant::now()` plus a `Duration`, so no reading can move under a loaded machine.

No constant is compared against its own literal, since a constant that agrees with itself states nothing. What is asserted is the rule: which request renders, which arms, how long it arms for, and which armed fire performs. The two relations between constants that are asserted, in `the_delay_is_capped_by_the_staleness_deadline` and `an_edit_just_after_a_render_does_not_take_a_second_leading_edge`, are each the premise of its test.

Both operands of those relations are bound to locals first. `clippy::assertions_on_constants` rejects an `assert!` whose whole condition is constant, this crate adds no `#[allow]`, and a local binding keeps the condition outside that lint by construction rather than by luck.
