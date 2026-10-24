# `crates/wolf-gnome/src/palette.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## No hardcoded Deferred row

The Deferred class is covered in the classification table by the `undispatched_probe()` row, which resolves from the registry and so cannot go stale, and totally by `every_registered_command_is_classified_by_the_palette`. A hardcoded Deferred row adds no coverage and goes red the day someone ships the feature it names, which is a false alarm. `HideApplication` stays hardcoded because NoGnomeEquivalent means the command will never be implemented, so that row cannot go stale.
