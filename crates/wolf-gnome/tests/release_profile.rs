//! Guards on the workspace `[profile.release]` block, and on the artifact that
//! block produces.
//!
//! Two of the three tests read `<CARGO_MANIFEST_DIR>/../../Cargo.toml`: the
//! profile is a promise, and those two are a tripwire on it, the same shape as
//! `smoke_headless.rs`'s
//! `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`. The
//! third reads the built binary instead, so it is evidence rather than a
//! restatement — but it only DISCRIMINATES the release profile when the binary
//! under test is the release one:
//!
//! ```sh
//! cargo test --release -p wolf-gnome --test release_profile -- --nocapture
//! ```
//!
//! Under a plain `cargo test` the binary is the dev one, whose default `debug`
//! is on, so the third test passes there without saying anything about
//! `[profile.release]`. It prints the path it read, which is what makes a
//! `--nocapture` run its own evidence that `--release` retargeted the binary.
//!
//! `std` only, deliberately: a `toml` or `object` dev-dependency would move
//! `Cargo.lock`, which is a `crate_universe` input in `MODULE.bazel`, creating a
//! repin obligation that cannot be discharged from a Linux host. Shelling out to
//! `readelf`/`objdump` is not available either — neither is in the devShell's
//! `packages`, so it would mean a `flake.nix` edit and a stale direnv for every
//! developer.
//!
//! NO BAZEL WIRING, and none is missing. `crates/wolf-gnome` has no
//! `BUILD.bazel` at all — that absence is deliberate and load-bearing, see
//! [`crates/wolf-gnome/DECISIONS.md`](../DECISIONS.md) — so there is no
//! `rust_test` target for this file to be added to, and no silent macOS-only gap
//! for an unwired file to fall into. Do not "fix" the missing target.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// What is asserted, and what is deliberately not.

/// The section header the scanner and both manifest tests key on. One spelling,
/// in one place, so the two cannot drift apart.
const RELEASE_SECTION: &str = "[profile.release]";

const PANIC_KEY: &str = "panic";
const DEBUG_KEY: &str = "debug";
const STRIP_KEY: &str = "strip";

/// The only acceptable `panic` value, re-typed rather than read from anywhere.
const EXPECT_PANIC: &str = "unwind";

/// The only acceptable `strip` value when the key is present at all.
const EXPECT_STRIP: &str = "none";

/// Every value cargo accepts for `debug` that fails this profile's purpose.
///
/// Cargo's documented set is `false` / `0` / `"none"`, `"line-directives-only"`,
/// `"line-tables-only"`, `1` / `"limited"`, and `2` / `true` / `"full"`. The last
/// three groups all carry a DWARF line table and are all fine here — which of them
/// is used is a size trade this file deliberately does not pin.
///
/// The first group means no debug info at all. `"line-directives-only"` is denied
/// for a different reason and on DOCUMENTED rather than measured grounds: cargo
/// describes it as enabling profiling on the `nvptx*` targets and says
/// `"line-tables-only"` is what other use cases want, so on this target it is at
/// best not the thing item 2's profiling asked for. Nobody has run it here; it is
/// denied so that nobody reaches for it believing it is a cheaper spelling of the
/// line-tables value.
///
/// This list is complete against cargo's documented set for the toolchain in use
/// (rustc 1.96.0) and cannot stay complete by itself: a spelling cargo adds later
/// would pass this test and be caught only by
/// [`the_binary_under_test_carries_line_tables`], and only under `--release`. That
/// is the residual, stated rather than hidden — an allow-list would trade it for
/// the opposite failure, rejecting a new and perfectly good value.
const DEBUG_OFF_VALUES: [&str; 4] = ["0", "false", "none", "line-directives-only"];

/// The DWARF section that carries the address-to-`file:line` mapping a profiler
/// needs. `.debug_info` would be a stronger claim than `debug = "line-tables-only"`
/// makes, so this is the section to look for.
///
/// Its PRESENCE is a guard on `strip`, not on `debug`, and that is measured rather
/// than reasoned — see [`the_binary_under_test_carries_line_tables`], which is why
/// a second, semantic assertion sits beside it.
const DWARF_LINE_SECTION: &str = ".debug_line";

/// The sections searched, in order, for [`OWN_SOURCE_NEEDLE`]. The first one that
/// is PRESENT and carries the needle satisfies the assertion.
///
/// Three rather than one, because which section holds a compilation unit's source
/// paths depends on the DWARF version, and rustc's default can move. Measured on
/// this host (rustc 1.96.0, LLVM 22.1.2), which emits DWARF **4** uniformly:
/// `.debug_str` holds the `DW_AT_name` paths and `.debug_line`'s own directory
/// table holds the bare string as well, while `.debug_line_str` — a DWARF 5
/// section — is absent entirely.
///
/// The order is deliberate. `.debug_str` is where `DW_AT_name` lives under both
/// DWARF 4 and 5, so it leads. `.debug_line_str` follows because DWARF 5 emits
/// file and directory names as `DW_FORM_line_strp` into it, moving them out from
/// under `.debug_line` — which is exactly why `.debug_line` may not be keyed on
/// alone, and why it comes last rather than not at all.
const NEEDLE_SECTIONS: [&str; 3] = [".debug_str", ".debug_line_str", ".debug_line"];

/// The workspace-relative source prefix that only THIS workspace's own compilation
/// units put in the DWARF strings.
///
/// Measured: with `debug = "line-tables-only"` `.debug_str` holds seventeen entries
/// shaped like `crates/wolf-gnome/src/lib.rs/@/wolf_gnome.<hash>-cgu.NN` (sixteen
/// CGUs plus `main.rs`). With `debug = false` only `/rustc/<hash>/library/…` and
/// `/rust/deps/{addr2line,gimli,object,miniz_oxide,libc,hashbrown,memchr}` survive
/// — std's own paths, which `[profile.release] debug` does not govern.
///
/// Both forms are relative, resolved against `DW_AT_comp_dir`, which is consistent
/// with `build.md`'s requirement that release builds run from the repository root.
const OWN_SOURCE_NEEDLE: &str = "crates/wolf-gnome/src";

/// How many source paths a failed needle assertion quotes back, so the reader can
/// tell "our crates emitted no debug info" from "the paths were remapped".
const SOURCE_SAMPLE_LIMIT: usize = 8;

// Nothing here asserts on `opt-level`, `lto` or `codegen-units`. None of the
// three has a silent failure mode — a wrong `opt-level` shows up in any
// measurement, and `lto`/`codegen-units` are a compile-time-versus-run-time
// trade whose right value is whatever the next measurement says. Pinning them
// would convert a future measured retune into a test edit, which is how a guard
// stops meaning anything.
//
// Nothing asserts on `.symtab` either, and that omission is not an oversight: no
// revert isolates it, because every `strip` value that removes the symbol table
// (`"symbols"`, `true`) is documented to remove the debug sections as well, so
// `the_binary_under_test_carries_line_tables` goes red for those values before
// any `.symtab` check could add anything. Documented rather than measured — the
// `strip` revert that was actually run here is `"debuginfo"`.

// ---------------------------------------------------------------------------
// Manifest helpers.

/// The workspace manifest, resolved from this crate's own directory.
///
/// This file lives in `wolf-gnome` rather than `wolf-core` for two reasons that
/// point the same way: `wolf-gnome` is the crate with no `BUILD.bazel`, so the
/// file needs no `rust_test` wiring and cannot become a macOS-only gap; and
/// `CARGO_BIN_EXE_wolf-gnome`, which the artifact half needs, is exported only to
/// this crate's own integration tests. The manifest read is the price: it is a
/// workspace file two directories up, and it is deliberately NOT a Bazel runfile.
fn workspace_manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml")
}

/// A deliberately small line scanner over the `[profile.release]` table. NOT a
/// TOML parser — parsing TOML properly would mean a dependency, which is exactly
/// what this file exists to avoid.
///
/// It tracks the current `[section]` header, collects `key = value` pairs only
/// while that header is exactly [`RELEASE_SECTION`], and stops at the next `[`
/// so a later `[profile.release.package."*"]` or `[profile.release.build-override]`
/// sub-table is not folded into the parent's keys. Blank lines and whole-line `#`
/// comments are skipped; surrounding quotes are trimmed from values, so
/// `panic = "unwind"` and a hypothetical unquoted form read the same.
///
/// STATED COUPLING with `Cargo.toml`: one key per line, and no trailing inline
/// comment on a profile key (`debug = "line-tables-only" # why` would yield the
/// value `line-tables-only" # why`). Both hold in the manifest as written, and a
/// change to either shows up as a failed assertion here rather than as a wrong
/// answer somewhere else. Put the rationale in the comment block ABOVE the
/// profile, which is where it already lives.
fn release_profile_keys(manifest: &str) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    let mut in_release = false;

    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_release = line == RELEASE_SECTION;
            continue;
        }
        if !in_release {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        keys.insert(key.trim().to_string(), value.to_string());
    }

    keys
}

/// Reads the workspace manifest, or panics naming the path it tried.
fn read_workspace_manifest(guard: &str) -> (PathBuf, String) {
    let path = workspace_manifest_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{guard}: could not read the workspace manifest at {}: {e}. That path is \
             CARGO_MANIFEST_DIR joined with `../../Cargo.toml`, so an error here means this \
             crate moved relative to the workspace root, not that the release profile regressed.",
            path.display()
        )
    });
    (path, text)
}

/// Whether the manifest declares the section at all, checked independently of
/// whether the scanner found any keys under it.
///
/// Kept apart from `!keys.is_empty()` on purpose: "no `[profile.release]`" and
/// "`[profile.release]` with nothing in it" are different defects with different
/// fixes, and a single emptiness check would report them identically.
fn declares_release_profile(manifest: &str) -> bool {
    manifest.lines().any(|line| line.trim() == RELEASE_SECTION)
}

// ---------------------------------------------------------------------------
// ELF helpers.
//
// A WHOLE-FILE SUBSTRING SCAN IS NOT SOUND FOR EITHER NEEDLE THIS TEST USES, AND
// MUST NEVER REPLACE THIS WALK. Both instances are measured, not reasoned:
//
//   * `.debug_line` — every Rust binary links std's backtrace machinery
//     (`object` / `gimli`), which carries DWARF section names as string literals
//     in `.rodata`, so `strings … | grep debug_line` matches on a FULLY STRIPPED
//     binary too.
//   * `crates/wolf-gnome/src` — the `debug = false` binary contains that string
//     18 times, every one of them in `.rodata`: `app.rs::init_lua_path` bakes
//     `env!("CARGO_MANIFEST_DIR")`, and `panic!` location strings carry source
//     paths. A whole-file scan would therefore pass on exactly the artifact this
//     test exists to reject.
//
// The rule behind both: the bytes being present in the file says nothing. A
// section header naming the section, and the needle found INSIDE that section's
// own extent, is the evidence.
//
// A related shortcut is defeated for a third reason, so do not "simplify" toward
// it: `strip = "debuginfo"` removes every `.debug*` section EXCEPT
// `.debug_gdb_scripts`, which survives at 34 bytes. Any wildcard "some `.debug*`
// section exists" check therefore passes on a stripped binary. This file keys on
// specifically named sections for that reason.

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELF_HEADER_LEN: usize = 64;

// `e_ident` indices, and the two values this walk requires of them.
const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;

// ELF64 header field offsets.
const E_SHOFF: usize = 0x28;
const E_SHENTSIZE: usize = 0x3A;
const E_SHNUM: usize = 0x3C;
const E_SHSTRNDX: usize = 0x3E;

// The ELF64 section header size, and the field offsets read from one.
const SHENTSIZE64: usize = 64;
const SH_NAME: usize = 0x00;
const SH_OFFSET: usize = 0x18;
const SH_SIZE: usize = 0x20;

/// Above this, `e_shnum` and `e_shstrndx` carry extended meanings stored in
/// section header 0 rather than in the ELF header.
const SHN_LORESERVE: u16 = 0xFF00;

fn u16_at(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(
        bytes[off..off + 2]
            .try_into()
            .expect("two bytes for a u16 at a checked offset"),
    )
}

fn u32_at(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(
        bytes[off..off + 4]
            .try_into()
            .expect("four bytes for a u32 at a checked offset"),
    )
}

fn u64_at(bytes: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(
        bytes[off..off + 8]
            .try_into()
            .expect("eight bytes for a u64 at a checked offset"),
    )
}

/// Seeks and reads exactly `len` bytes, rather than slurping a multi-megabyte
/// binary into memory to index into it.
fn read_at(file: &mut File, bin: &Path, what: &str, offset: u64, len: usize) -> Vec<u8> {
    file.seek(SeekFrom::Start(offset)).unwrap_or_else(|e| {
        panic!(
            "could not seek to {what} at offset {offset} in {}: {e}",
            bin.display()
        )
    });
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).unwrap_or_else(|e| {
        panic!(
            "could not read {len} bytes of {what} at offset {offset} in {}: {e}. A short read \
             means the file is truncated or is not the ELF64 object this walk assumes.",
            bin.display()
        )
    });
    buf
}

/// Opens the binary under test, or panics naming it.
fn open_binary(bin: &Path) -> File {
    File::open(bin).unwrap_or_else(|e| {
        panic!(
            "could not open the binary under test at {}: {e}. Cargo exports that path as \
             CARGO_BIN_EXE_wolf-gnome and builds the binary before running this test, so a \
             missing file here is a harness problem, not a profile regression.",
            bin.display()
        )
    })
}

/// The file offset and size of the first section header naming `section`, or
/// `None` if no header does.
///
/// A minimal ELF64 section-header walk. Everything it needs to be true of the
/// file — the magic, 64-bit class, little-endian data, a 64-byte section header
/// entry, a non-empty non-extended section header table — is asserted with the
/// path in the message, because a non-ELF or otherwise unexpected artifact means
/// this test's PREMISE is gone, not that the release profile regressed. Those two
/// outcomes need to be told apart in the failure output, so they are separate
/// panics with separate messages.
fn section_extent(bin: &Path, section: &str) -> Option<(u64, u64)> {
    let mut file = open_binary(bin);

    let ehdr = read_at(&mut file, bin, "the ELF header", 0, ELF_HEADER_LEN);

    let magic = &ehdr[..ELF_MAGIC.len()];
    assert!(
        magic == ELF_MAGIC.as_slice(),
        "{} does not start with the ELF magic (first four bytes: {magic:02x?}). This walk only \
         understands ELF64, so the test's premise is gone rather than the release profile \
         having regressed — find out what that artifact is before touching Cargo.toml.",
        bin.display()
    );
    assert_eq!(
        ehdr[EI_CLASS],
        ELFCLASS64,
        "{} has EI_CLASS {} rather than ELFCLASS64 ({ELFCLASS64}). Every field offset below is \
         the 64-bit layout's, so parsing a 32-bit object here would read garbage and report a \
         confident wrong answer. Premise gone, not a profile regression.",
        bin.display(),
        ehdr[EI_CLASS]
    );
    assert_eq!(
        ehdr[EI_DATA],
        ELFDATA2LSB,
        "{} has EI_DATA {} rather than ELFDATA2LSB ({ELFDATA2LSB}). Every scalar below is read \
         with `from_le_bytes`, so a big-endian object would byte-swap into nonsense. Premise \
         gone, not a profile regression.",
        bin.display(),
        ehdr[EI_DATA]
    );

    let e_shentsize = u16_at(&ehdr, E_SHENTSIZE);
    assert_eq!(
        e_shentsize as usize,
        SHENTSIZE64,
        "{} declares e_shentsize {e_shentsize}, not the {SHENTSIZE64} bytes an ELF64 section \
         header occupies. The table below is walked in fixed-size chunks, so a different entry \
         size would misalign every field. Premise gone, not a profile regression.",
        bin.display()
    );

    let e_shoff = u64_at(&ehdr, E_SHOFF);
    let e_shnum = u16_at(&ehdr, E_SHNUM);
    let e_shstrndx = u16_at(&ehdr, E_SHSTRNDX);

    assert!(
        e_shnum != 0,
        "{} declares e_shnum 0. That means one of two things and this walk can distinguish \
         neither: the section header table was removed entirely (which IS the failure this test \
         guards, reported here instead of as an absent `{DWARF_LINE_SECTION}`), or the file has \
         at least SHN_LORESERVE sections and the real count lives in section header 0's sh_size, \
         which this walk does not implement. Refusing rather than misparsing.",
        bin.display()
    );
    assert!(
        e_shstrndx < SHN_LORESERVE,
        "{} declares e_shstrndx {e_shstrndx}, at or above SHN_LORESERVE ({SHN_LORESERVE:#06x}), \
         so the section name string table index is stored in section header 0's sh_link rather \
         than in the ELF header. This walk does not implement extended numbering; refusing \
         rather than resolving names against the wrong table.",
        bin.display()
    );
    assert!(
        e_shstrndx < e_shnum,
        "{} declares e_shstrndx {e_shstrndx}, outside its own {e_shnum}-entry section header \
         table. Reported here rather than as a slice-index panic, because the two read very \
         differently: this is a malformed or unexpected artifact, not a profile regression.",
        bin.display()
    );

    let table = read_at(
        &mut file,
        bin,
        "the section header table",
        e_shoff,
        e_shnum as usize * SHENTSIZE64,
    );

    // Resolve section names through the e_shstrndx entry's own offset and size.
    let name_hdr_at = e_shstrndx as usize * SHENTSIZE64;
    let name_hdr = &table[name_hdr_at..name_hdr_at + SHENTSIZE64];
    let strtab = read_at(
        &mut file,
        bin,
        "the section name string table",
        u64_at(name_hdr, SH_OFFSET),
        u64_at(name_hdr, SH_SIZE) as usize,
    );

    // `chunks_exact`, not an index range: clippy::needless_range_loop, and the
    // chunk length is checked by construction rather than by arithmetic.
    for entry in table.chunks_exact(SHENTSIZE64) {
        let sh_name = u32_at(entry, SH_NAME) as usize;
        if sh_name >= strtab.len() {
            continue;
        }
        let name = strtab[sh_name..]
            .split(|b| *b == 0)
            .next()
            .unwrap_or_default();
        if name == section.as_bytes() {
            return Some((u64_at(entry, SH_OFFSET), u64_at(entry, SH_SIZE)));
        }
    }

    None
}

/// Size in bytes of the binary's [`DWARF_LINE_SECTION`], or `None` if no section
/// header names it.
fn dwarf_line_section_size(bin: &Path) -> Option<u64> {
    section_extent(bin, DWARF_LINE_SECTION).map(|(_, size)| size)
}

/// The bytes a named section occupies in the file, or `None` if no section header
/// names it.
///
/// Only `SHT_PROGBITS` sections are ever asked for here. Do not reuse this for a
/// `SHT_NOBITS` section such as `.bss`: its `sh_offset` does not describe file
/// contents at all, and this helper would hand back whatever happens to sit there.
fn section_bytes(bin: &Path, section: &str) -> Option<Vec<u8>> {
    let (offset, size) = section_extent(bin, section)?;
    let mut file = open_binary(bin);
    Some(read_at(&mut file, bin, section, offset, size as usize))
}

/// Up to `limit` source-path-looking entries from EACH section that was present,
/// labelled by section, for a failed needle assertion to quote back.
///
/// This is what lets a reader tell "our own crates emitted no debug info" from
/// "the paths were remapped to something this needle does not match": the former
/// shows only `/rustc/<hash>/library/…` and `/rust/deps/…`, the latter shows our
/// files under a prefix that is not [`OWN_SOURCE_NEEDLE`].
///
/// EVERY present section is sampled rather than one, and that is not thoroughness
/// for its own sake — only some of them can show either shape. Measured: under
/// DWARF 4 the full paths live in `.debug_str`, while `.debug_line`'s own file
/// table holds bare basenames (`mod.rs`, `unix.rs`, `alloc.rs`). Sampling whichever
/// section happened to be read last would therefore hand the reader `mod.rs` eight
/// times while the message above told them to look for `/rustc/<hash>/library/…`
/// — a diagnostic contradicting its own instructions at the moment someone is
/// trying to follow them. Labelling each block is what keeps the two legible
/// together.
fn source_path_samples(sections: &[(&str, String)], limit: usize) -> String {
    if sections.is_empty() {
        return String::from("\n      (no section was present to sample)");
    }

    let mut out = String::new();
    for (name, text) in sections {
        out.push_str("\n      ");
        out.push_str(name);
        out.push(':');

        let mut any = false;
        for entry in text.split('\0').filter(|e| e.contains(".rs")).take(limit) {
            out.push_str("\n        ");
            out.push_str(entry);
            any = true;
        }
        if !any {
            out.push_str("\n        (no NUL-separated entry containing \".rs\")");
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Test 1: the panic strategy.

/// Pins `[profile.release]`'s `panic` to `unwind`, and pins that the section
/// exists at all.
///
/// ## Why this one needs a mechanical guard
///
/// `panic = "abort"` is the tempting edit — it is smaller and faster — and its
/// cost is invisible at the point of temptation. Cargo silently IGNORES `panic`
/// for the `test` and `bench` profiles, so it cannot honour `"abort"` for a test
/// binary: it compiles a second set of standard-library artifacts instead. That
/// forks `target/release/` between `cargo build --release` and `cargo test
/// --release`, and every alternation between the two rebuilds the whole
/// dependency graph. What matters is that the rebuild is TOTAL and SILENT, not
/// how many crates it is: nothing warns, nothing errors, and the only symptom is
/// that the build is inexplicably never warm.
///
/// `"abort"` also turns an application panic into `SIGABRT`, which changes the
/// exit-code evidence the cage smokes rest on — those distinguish "exited
/// non-zero" from "still alive" precisely because folding two causes into one
/// code destroys evidence.
///
/// ## Falsifier
///
/// Change `panic = "unwind"` to `"abort"` in the workspace `Cargo.toml`; assertion
/// 3 fires. Delete the `[profile.release]` block; assertion 1 fires first, and
/// says which of the two shapes it saw.
///
/// ## What it needs
///
/// Nothing: no compositor, no nvim, no built binary. It reads one file.
#[test]
fn the_release_profile_never_aborts_on_panic() {
    const GUARD: &str = "the_release_profile_never_aborts_on_panic";

    let (path, text) = read_workspace_manifest(GUARD);

    // --- Assertion 1: the section exists at all -------------------------------
    assert!(
        declares_release_profile(&text),
        "{GUARD}: {} declares no `{RELEASE_SECTION}` section. With no block there, `cargo build \
         --release` takes cargo's built-in defaults, so nothing in this file's contract holds — \
         and the two failures are different: an absent section means the block was deleted, \
         while a present-but-empty one means its keys were. Restore the block described in the \
         comment above it.",
        path.display()
    );

    // --- Assertion 2: it has keys under it ------------------------------------
    let keys = release_profile_keys(&text);
    assert!(
        !keys.is_empty(),
        "{GUARD}: {} declares `{RELEASE_SECTION}` but the scanner found no `key = value` pairs \
         under it. Either the block really is empty, or it now uses a shape this deliberately \
         small line scanner cannot read — one key per line, no trailing inline comment on a \
         profile key. See `release_profile_keys`; the coupling is stated there.",
        path.display()
    );

    // --- Assertion 3: the value ----------------------------------------------
    let actual = keys.get(PANIC_KEY).map(String::as_str);
    assert_eq!(
        actual,
        Some(EXPECT_PANIC),
        "{GUARD}: `{RELEASE_SECTION}` in {} has {PANIC_KEY} = {actual:?}, expected \
         Some({EXPECT_PANIC:?}). Three outcomes are distinguishable here and all are defects. \
         None: the key was dropped — it is cargo's default, so behaviour is unchanged today, but \
         the declaration is the thing that keeps the tempting edit from being made silently. \
         Some(\"abort\"): cargo silently ignores `panic` for the `test` and `bench` profiles, so \
         it builds a SECOND set of release artifacts for tests — `cargo build --release` and \
         `cargo test --release` then evict each other and rebuild the whole dependency graph on \
         every alternation, with no warning of any kind. It would also turn an application panic \
         into SIGABRT, destroying the exit-code evidence the cage smokes rest on. Anything else: \
         not a value cargo accepts.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Test 2: the debug info.

/// Pins that the release profile still emits debug info and does not strip it.
///
/// ## Why
///
/// Item 2 of the Linux work order profiles this binary. A `perf`-style profiler
/// needs function names and `file:line`, which is exactly what
/// `debug = "line-tables-only"` buys and what a stripped build takes away — a
/// stripped release binary gives a profiler nothing but addresses. `strip` is
/// asserted as well as `debug` because cargo's `strip` DEFAULT is version
/// dependent, so "we did not set it" is not a stable answer.
///
/// The `debug` half is deliberately loose about WHICH value is used:
/// `"line-tables-only"`, `1`, `"limited"`, `2`, `true` and `"full"` all carry line
/// tables, and choosing between them is a size trade this test has no business
/// pinning. What it rejects is [`DEBUG_OFF_VALUES`], whose membership and its one
/// documented-rather-than-measured entry are explained there. The `strip` half
/// tolerates the key being absent — that is cargo's own default and today it
/// strips nothing — but rejects any value that would remove what `debug` just
/// asked for.
///
/// ## Falsifier
///
/// `debug = "line-tables-only"` → `false`, or `strip = "none"` → `"debuginfo"`.
/// Either fires. Both are the reverts measured against
/// [`the_binary_under_test_carries_line_tables`], which they also redden under
/// `--release` — `strip` at its `.debug_line` assertion, `debug` at its
/// source-path needle — and that is the point: this test reads the promise and
/// that one reads the artifact.
///
/// `strip = "symbols"` and `strip = true` are rejected here too and are documented
/// to remove the debug sections as well, but neither was run, so neither is
/// claimed as a measured falsifier.
///
/// ## What it needs
///
/// Nothing. It reads one file.
#[test]
fn the_release_profile_keeps_its_debug_info() {
    const GUARD: &str = "the_release_profile_keeps_its_debug_info";

    let (path, text) = read_workspace_manifest(GUARD);
    assert!(
        declares_release_profile(&text),
        "{GUARD}: {} declares no `{RELEASE_SECTION}` section at all, so neither {DEBUG_KEY} nor \
         {STRIP_KEY} is stated anywhere and the release build takes cargo's defaults — whose \
         `strip` default is version dependent. Restore the block.",
        path.display()
    );

    let keys = release_profile_keys(&text);

    // --- debug: present, and not one of the "no debug info" spellings ---------
    let debug = keys.get(DEBUG_KEY).map(String::as_str);
    let Some(debug) = debug else {
        panic!(
            "{GUARD}: `{RELEASE_SECTION}` in {} states no `{DEBUG_KEY}` key. Cargo's release \
             default is no debug info at all, so item 2 of the Linux work order — profiling this \
             binary — would get addresses and no symbols from `perf`, and a release panic \
             backtrace would carry no file or line. Restore `{DEBUG_KEY} = \
             \"line-tables-only\"`.",
            path.display()
        )
    };
    assert!(
        !DEBUG_OFF_VALUES.contains(&debug),
        "{GUARD}: `{RELEASE_SECTION}` in {} has {DEBUG_KEY} = {debug:?}, which is one of the \
         refused values {DEBUG_OFF_VALUES:?}. Item 2 of the Linux work order profiles this \
         binary, and a release build with no DWARF line table gives a profiler addresses and no \
         `file:line`. Two different defects sit in that set: 0, false and none emit no debug \
         info at all, while \"line-directives-only\" is cargo's `nvptx*`-target profiling value \
         — cargo's own documentation says \"line-tables-only\" is what every other use case \
         wants, and it is refused on that documented basis rather than a measured one. Any of \
         \"line-tables-only\", 1, \"limited\", 2, true or \"full\" would satisfy this; the choice \
         between them is a size trade this test deliberately does not pin.",
        path.display()
    );

    // --- strip: absent, or explicitly none ------------------------------------
    let strip = keys.get(STRIP_KEY).map(String::as_str);
    assert!(
        strip.is_none() || strip == Some(EXPECT_STRIP),
        "{GUARD}: `{RELEASE_SECTION}` in {} has {STRIP_KEY} = {strip:?}, expected \
         Some({EXPECT_STRIP:?}) or no key at all. Two distinguishable states are acceptable and \
         everything else is not: the key absent (cargo's own default applies — tolerated, but \
         that default is version dependent, which is why the manifest states `none` explicitly), \
         or `none` (the contract: symbols and DWARF survive). \"debuginfo\" removes the very \
         sections `{DEBUG_KEY} = {debug:?}` just asked for, and \"symbols\" or true removes \
         those AND the symbol table — either way the profiling this profile exists to support \
         gets nothing.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Test 3: the artifact.

/// Reads the BUILT binary and asserts two things about it: that a non-empty
/// [`DWARF_LINE_SECTION`] is present, and that THIS WORKSPACE's own source paths
/// appear in the DWARF strings.
///
/// This is the half that is not a restatement of the manifest: the two tests
/// above assert what `Cargo.toml` promises, and this one asserts what cargo
/// actually produced. It prints the path it read and both findings, so a
/// `--nocapture` run is itself the evidence that `--release` retargeted the
/// binary — a `/release/` path proves the artifact under test is the optimized
/// one, and a `/debug/` path proves it is not.
///
/// ## Why one assertion is not enough, measured
///
/// `.debug_line` being present is a guard on `strip`, NOT on `debug`. Measured
/// with `debug = false` in `[profile.release]`: the section is still there, at
/// 271,586 bytes, because Rust's precompiled standard-library rlibs
/// (`std::backtrace_rs`, `addr2line`, `gimli`, `object`, `alloc`, `core`,
/// `compiler-builtins`) ship debug info that `[profile.release] debug` does not
/// govern. With `debug = "line-tables-only"` the same section is 5,981,172 bytes.
/// A presence check cannot tell those apart, and neither can a size threshold that
/// would age with every std release and every dependency added.
///
/// So the second assertion is semantic rather than dimensional: the DWARF strings
/// must contain [`OWN_SOURCE_NEEDLE`], which only this workspace's own compilation
/// units emit. `.debug_str` is populated either way — 1,326,607 bytes even with
/// `debug = false` — so it is the NEEDLE and not the section that discriminates.
///
/// Every size quoted above is from a whole-workspace `cargo build --release`. The
/// numbers this test PRINTS under the `-p wolf-gnome` command below run a few
/// hundred bytes lower — 5,980,598 rather than 5,981,172 for the line-tables case
/// — because restricting the build unifies features differently. Both are correct;
/// a small mismatch between the printed value and the documented one is that, not
/// a defect.
///
/// ## What it discriminates, and when
///
/// Under a plain `cargo test` the binary is the dev-profile one, whose default
/// `debug` is on, so a green run says nothing about `[profile.release]`. The one
/// command under which this test is discriminating for the release profile is:
///
/// ```sh
/// cargo test --release -p wolf-gnome --test release_profile -- --nocapture
/// ```
///
/// ## Falsifiers
///
/// Both verified under `--release`, and only these two are claimed:
///
/// * `debug = false` in `[profile.release]` — the needle assertion fires. The
///   `.debug_line` assertion does NOT, for the reason measured above.
/// * `strip = "debuginfo"` — both fire; every `.debug*` section is gone but a
///   34-byte `.debug_gdb_scripts`.
///
/// ## Soundness
///
/// See the comment above the ELF helpers. A whole-file substring scan is unsound
/// for BOTH needles — `.debug_line`'s name and `crates/wolf-gnome/src` alike sit
/// in `.rodata` of a binary that carries no debug info of ours — so each must be
/// found inside the extent a section header describes.
///
/// ## What it needs
///
/// The built `wolf-gnome` binary, which cargo builds for this test by declaring
/// `[[bin]]`, and a Linux ELF64 host. No compositor, no nvim, no display.
#[test]
fn the_binary_under_test_carries_line_tables() {
    const GUARD: &str = "the_binary_under_test_carries_line_tables";

    let bin = Path::new(env!("CARGO_BIN_EXE_wolf-gnome"));
    let found = dwarf_line_section_size(bin);

    // Printed, not merely asserted: which binary was read is the whole evidence
    // that `--release` retargeted this test. Visible with `-- --nocapture`.
    println!("{GUARD}: binary under test = {}", bin.display());
    match found {
        Some(size) => println!("{GUARD}: {DWARF_LINE_SECTION} = {size} bytes"),
        None => println!("{GUARD}: {DWARF_LINE_SECTION} = absent"),
    }

    // --- Assertion 1: a line table exists at all (the `strip` contract) -------
    let Some(size) = found else {
        panic!(
            "{GUARD}: {} has no `{DWARF_LINE_SECTION}` section header at all, so the binary \
             carries no DWARF line table — not even the standard library's — and a profiler can \
             map none of its addresses to a `file:line`. Measured: the cause of a MISSING section \
             is `{STRIP_KEY}` (\"debuginfo\", \"symbols\" or true), not `{DEBUG_KEY}`, which \
             leaves the section present and is what assertion 2 below catches. Which profile \
             produced the artifact is in the path printed above. Do not weaken this to a \
             byte-substring scan of the file: every Rust binary carries the literal string \
             \"{DWARF_LINE_SECTION}\" in .rodata via std's backtrace machinery, even when fully \
             stripped, so such a scan would pass on exactly the artifact this test exists to \
             reject.",
            bin.display()
        )
    };

    assert!(
        size > 0,
        "{GUARD}: {} has a `{DWARF_LINE_SECTION}` section header whose sh_size is 0. The section \
         exists but holds no line program, so it maps no address to any `file:line` — a linker \
         or `strip` that emptied the section rather than removing it leaves exactly this shape, \
         and an existence-only check would call it a pass.",
        bin.display()
    );

    // --- Assertion 2: the line tables are OURS, not just std's ----------------
    //
    // Assertion 1 above survives `debug = false` (271,586 bytes of std's own),
    // so this is the assertion actually attached to the `debug` contract. Each
    // section in `NEEDLE_SECTIONS` is searched in order and the first PRESENT one
    // carrying the needle wins, which is what makes this robust across a rustc
    // DWARF version bump rather than pinned to the DWARF 4 measured here.
    let mut searched: Vec<(&str, usize)> = Vec::new();
    let mut hit: Option<(&str, usize)> = None;
    // Every present section's text, in the order searched, so a failure quotes
    // back the one that can actually show the shapes its guidance names — see
    // `source_path_samples`. Retained rather than overwritten for that reason.
    let mut texts: Vec<(&str, String)> = Vec::new();

    for section in NEEDLE_SECTIONS {
        let Some(bytes) = section_bytes(bin, section) else {
            println!("{GUARD}: {section} = absent");
            continue;
        };
        println!("{GUARD}: {section} = {} bytes", bytes.len());
        searched.push((section, bytes.len()));

        // `.debug_str` and `.debug_line_str` are NUL-separated UTF-8; `.debug_line`
        // is a binary line program with its DWARF 4 directory and file names
        // embedded in it. A lossy view serves both: the needle is ASCII, so
        // surrounding invalid bytes become U+FFFD without splitting the run, and
        // the owned copy is what the failure path quotes entries back from.
        //
        // It also assumes the section is UNCOMPRESSED, which is true on this
        // toolchain and is NOT an observed failure — no `SHF_COMPRESSED` section
        // has been seen here. A linker `--compress-debug-sections=zlib` or a
        // future rustc default would make the needle unfindable in readable bytes,
        // so the failure message names compression as a third candidate cause
        // rather than leaving a reader to chase the two that would not be true.
        // Inflating it here would mean a dependency; see this file's header.
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let carries_needle = text.contains(OWN_SOURCE_NEEDLE);
        texts.push((section, text));
        if carries_needle {
            hit = Some((section, bytes.len()));
            break;
        }
    }

    match hit {
        Some((section, len)) => {
            println!("{GUARD}: {OWN_SOURCE_NEEDLE:?} found in {section} ({len} bytes)")
        }
        None => {
            println!("{GUARD}: {OWN_SOURCE_NEEDLE:?} found in none of {NEEDLE_SECTIONS:?}")
        }
    }

    assert!(
        hit.is_some(),
        "{GUARD}: {} carries no {OWN_SOURCE_NEEDLE:?} in any of {NEEDLE_SECTIONS:?}; the sections \
         that were present are {searched:?} (name, bytes). The binary therefore has debug info \
         that is not OURS: the standard library's precompiled rlibs ship their own, which \
         `[profile.release] {DEBUG_KEY}` does not govern, which is why `.debug_str` is populated \
         (measured: 1,326,607 bytes) and `{DWARF_LINE_SECTION}` non-empty (measured: 271,586 \
         bytes, against 5,981,172 with line tables) even with `{DEBUG_KEY} = false` — all from a \
         whole-workspace `cargo build --release`, so a `-p wolf-gnome` build prints a few hundred \
         bytes lower through feature unification. Three causes are distinguishable from the \
         samples below, and read `.debug_str`'s block for the first two: if it shows only \
         `/rustc/<hash>/library/…` and `/rust/deps/…`, this workspace's own crates emitted no \
         debug info and `{DEBUG_KEY}` is the key to look at; if it shows our files under some \
         other prefix, the paths were remapped and this needle needs updating instead. The third \
         is COMPRESSION, and it looks unlike either: a block of unreadable bytes, or the section \
         missing while a `.zdebug_`-prefixed one is not. This walk reads no `sh_flags` and \
         inflates nothing, so a `SHF_COMPRESSED` section — from a linker \
         `--compress-debug-sections=zlib` or a future rustc default — is unsearchable rather \
         than empty, and nothing above would say so. Decompression is deliberately not \
         implemented: it would mean a dependency, and this file is `std`-only by design. \
         `{DWARF_LINE_SECTION}`'s own block answers none of the three — its DWARF 4 file table \
         holds bare basenames such as `mod.rs` — so it is quoted for completeness only. \
         Samples, {SOURCE_SAMPLE_LIMIT} per section, in the order searched:{}",
        bin.display(),
        source_path_samples(&texts, SOURCE_SAMPLE_LIMIT)
    );
}
