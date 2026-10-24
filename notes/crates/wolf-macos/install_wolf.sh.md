# `crates/wolf-macos/install_wolf.sh`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Archive and staging

`$1` is the rootpath of the `:WolfRelease` archive, staged in this target's runfiles; Bazel builds it before the run, so the optimized bundle already exists when the script executes. Why the path arrives through a filegroup is in [the wolf-macos decisions](../../wolf-macos-decisions.md#why-installwolf-takes-the-archive-through-a-filegroup).

The zip is the input, not the `<name>_archive-root/Wolf.app` directory beside it. That directory is an undeclared side effect of the bundling action that nothing tracks, so runfiles cannot carry it. `ditto -x -k` is the extractor because it preserves the symlinks, exec bits and code signature that make the bundle launchable, and `unzip` does not.

Extraction goes to a staging directory first, and the existing app is removed only once a complete new bundle exists. A failed or interrupted extraction then leaves the installed app untouched rather than destroying it.
