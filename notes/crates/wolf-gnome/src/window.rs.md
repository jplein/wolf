# `crates/wolf-gnome/src/window.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Overlay stacking across tabs

The add order and the reasons for it are in [Window structure](../../../wolf-gnome-decisions.md#window-structure). The empty state, the message toast and the completion popup are per page because the toast belongs to a pane view, so the empty state has to move with it; otherwise a message in a project-less window would draw underneath the status page.

The four transient overlays stay above the opaque empty state across the tab boundary because they are overlay children of an ancestor overlay, the same GTK stacking rule the preview relies on. The toast and the popup are sized to their own content and hidden when there is nothing to show, so neither takes layout space or a pointer event at rest.

## Wake bridge tests

The argument itself is in [Threading and the wake](../../../wolf-gnome-decisions.md#threading-and-the-wake). These tests pin its transport half, so a change to `async-channel`'s bounded semantics, such as a ring buffer that drops the old item instead of refusing the new one, fails here rather than silently stranding a death wake.

They read the production `WAKE_CHANNEL_CAP` rather than a literal `1`, so raising the capacity cannot quietly turn them into tests of something else.
