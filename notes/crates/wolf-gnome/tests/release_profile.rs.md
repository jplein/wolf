# `crates/wolf-gnome/tests/release_profile.rs`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Unasserted keys and sections

`opt-level`, `lto` and `codegen-units` have no silent failure mode: a wrong `opt-level` shows in any measurement, and `lto` and `codegen-units` are a compile-time versus run-time trade whose right value is whatever the next measurement says. Pinning them would turn a measured retune into a test edit, which is how a guard stops meaning anything.

`.symtab` has no assertion because no revert isolates it. Every `strip` value that removes the symbol table (`"symbols"`, `true`) is documented to remove the debug sections too, so `the_binary_under_test_carries_line_tables` fails first. That is documented rather than measured: the `strip` revert actually run is `"debuginfo"`.

## Section walk not substring scan

Both needles occur in a binary that carries none of this workspace's debug info, so a whole-file substring scan passes on exactly the artifact the test rejects. Every Rust binary links std's backtrace machinery (`object`, `gimli`), which carries the `DWARF` section names as string literals in `.rodata`, so `strings … | grep debug_line` matches a fully stripped binary. The `debug = false` binary contains `crates/wolf-gnome/src` 18 times, all in `.rodata`, because `app.rs::init_lua_path` bakes `env!("CARGO_MANIFEST_DIR")` and `panic!` locations carry source paths. Both cases are measured.

The evidence is a section header naming the section, with the needle found inside that section's own extent. A wildcard check that some `.debug*` section exists fails for a third reason: `strip = "debuginfo"` removes every `.debug*` section except `.debug_gdb_scripts`, which survives at 34 bytes. That is why the test keys on specifically named sections.

## Lossy uncompressed reads

`.debug_str` and `.debug_line_str` are null-separated UTF-8, and `.debug_line` is a binary line program with its `DWARF` 4 directory and file names embedded. Because the needle is ASCII, invalid bytes around it become `U+FFFD` without splitting the run, and the owned copy is what the failure path quotes entries from.

The read assumes the section is uncompressed, which holds on this toolchain: no `SHF_COMPRESSED` section has been observed. A linker `--compress-debug-sections=zlib` or a future rustc default would make the needle unfindable in readable bytes, so the failure message names compression as a third candidate cause. Inflating the section here would need a dependency, and the file is std-only.
