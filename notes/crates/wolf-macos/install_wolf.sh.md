# `crates/wolf-macos/install_wolf.sh`

Last verified at commit `9826f311e244004a192c46b629db305f0d6b283d`.

## Archive and staging

`$1` is the rootpath of the `:WolfRelease` archive, staged in this target's runfiles; Bazel builds it before the run, so the optimized bundle already exists when the script executes. Why the path arrives through a filegroup is in [the wolf-macos decisions](../../wolf-macos-decisions.md#why-installwolf-takes-the-archive-through-a-filegroup).

The zip is the input, not the `<name>_archive-root/Wolf.app` directory beside it. That directory is an undeclared side effect of the bundling action that nothing tracks, so runfiles cannot carry it. `ditto -x -k` is the extractor because it preserves the symlinks, exec bits and code signature that make the bundle launchable, and `unzip` does not.

Extraction goes to a staging directory first, and the existing app is removed only once a complete new bundle exists. A failed or interrupted extraction then leaves the installed app untouched rather than destroying it.

## Launch Services registration

The release archive's entries are all dated 1980, and `ditto` keeps those dates. The installed bundle therefore never looks newer than a record Launch Services cached for the same path, and a record read before the document types existed can survive. `lsregister -f` forces Launch Services to read the bundle again whatever the dates.

A missing tool or a failed registration only warns: the app is installed and launches either way, and only Finder's Open With and drops on the Dock icon depend on registration.
