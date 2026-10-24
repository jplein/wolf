# `crates/wolf-core/tests/nav_history_live.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Growth needs distinct targets

Sections 1 to 13 drive nvim by typing. Section 14 drives it the way a front-end does, through the `InputEvent`s that a file picker, a sidebar row, a project-search hit, a buffer-picker row and an Open Recent row send. It measures which of them record a navigation entry, the claim `crates/wolf-gnome/src/README.md` makes.

`history_entries` goes through `PaneHistory::menu_rows`, which keeps the first occurrence of each distinct `NavTarget` walking newest-first, so pushing a target the pane has already visited leaves the row count unchanged. Row growth is observable only when the target is new to the pane, so each case arranges that and asserts the precondition rather than assuming it. The buffer-picker case, whose point is switching to a buffer another pane listed, uses a buffer this pane has never shown, built with `:badd`.
