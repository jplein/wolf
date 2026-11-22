# `crates/wolf-gnome/src/window.rs`

Last verified at commit `9627d9de77a37544d3d73f9af36c192624f9cbed`.

## Overlay stacking across tabs

The add order and the reasons for it are in [Window structure](../../../wolf-gnome/window-structure.md). The empty state, the message toast and the completion popup are per page because the toast belongs to a pane view, so the empty state has to move with it; otherwise a message in a project-less window would draw underneath the status page.

The three transient overlays stay above the opaque empty state across the tab boundary because they are overlay children of an ancestor overlay, the same GTK stacking rule the preview relies on. The toast and the popup are sized to their own content and hidden when there is nothing to show, so neither takes layout space or a pointer event at rest.
