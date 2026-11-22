# `crates/wolf-gnome/src/tab_view.rs`

Last verified at commit `9851fcf52ad90d1d41673b5d2409768023b5f5a0`.

## Wake bridge tests

The argument itself is in [Threading and the wake](../../../wolf-gnome-decisions.md#threading-and-the-wake). These tests pin its transport half, so a change to `async-channel`'s bounded semantics, such as a ring buffer that drops the old item instead of refusing the new one, fails here rather than silently stranding a death wake.

They read the production `WAKE_CHANNEL_CAP` rather than a literal `1`, so raising the capacity cannot quietly turn them into tests of something else.
