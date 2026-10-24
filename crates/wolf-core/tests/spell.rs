//! Integration tests for SPELL CHECKING on prose filetypes: the `WolfSpell`
//! FileType autocmd turns window-local `'spell'` ON for markdown/text/gitcommit
//! and OFF for everything else, `'spelllang'` is pinned to `en`, and the global
//! `'spellfile'` points at `<state_dir()>/spell/en.utf-8.add` (via spawn's
//! `WOLF_SPELL_DIR` export). Drives a REAL, hermetic nvim (via `WOLF_NVIM_BIN`)
//! — no mocks.
//!
//! State-dir strategy: SHARED HERMETIC (`common::hermetic_state_dir()`), no env
//! mutation, so the tests in this binary are concurrent-safe. The cases that
//! need an exclusive, inspectable `WOLF_STATE_DIR` (durability across a
//! restart, comma-escaping) live in `spell_durable.rs`.
//!
//! Observation channel: the file probe (`common::probe_values`) — core exposes
//! no eval RPC, so nvim itself writes the probed option values to a temp file.
//! Because the probe command queues behind earlier keys, a probe after an `:e`
//! observes the post-`:e` window state with no extra settle.

mod common;

/// Opening a `.md` file must enable window-local spell with the pinned language
/// and the durable spellfile. `'spelllang' == "en"` is nvim's default too, so
/// the discriminating assertions are `&l:spell` (the autocmd) and `&spellfile`
/// (the export + `opt.spellfile` assignment).
#[test]
fn spell_enables_for_markdown_on_open() {
    let (core, _waiter, _file) =
        common::start_core_with_temp_md("# Spell Fixture\nplain body words\n");

    let probe_dir = tempfile::tempdir().expect("create probe dir");
    let vals = common::probe_values(
        &core,
        &["string(&l:spell)", "&spelllang", "&spellfile"],
        &probe_dir.path().join("markdown.txt"),
    );

    assert_eq!(
        vals[0], "1",
        "opening a .md file must turn window-local 'spell' ON (0 means the WolfSpell \
         FileType autocmd is missing, or markdown was dropped from its prose set)"
    );
    assert_eq!(
        vals[1], "en",
        "'spelllang' must be pinned to en — the only language whose .spl the pinned \
         nvim ships; any other value would raise a blocking download prompt that has \
         nowhere to draw under ext_messages"
    );
    let expected = common::hermetic_state_dir()
        .join("spell")
        .join("en.utf-8.add");
    assert_eq!(
        vals[2],
        expected.to_string_lossy(),
        "'spellfile' must point at the durable <state_dir>/spell/en.utf-8.add (a mismatch \
         means spawn's WOLF_SPELL_DIR export or init.lua's opt.spellfile assignment is gone, \
         so zg words would die with the process)"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The other two prose filetypes: `text` and `gitcommit`. Each pair asserts the
/// detected filetype first as a precondition, so a spell failure is
/// attributable to the prose set and not to filetype detection.
#[test]
fn spell_enables_for_text_and_gitcommit() {
    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);
    let probe_dir = tempfile::tempdir().expect("create probe dir");

    // The files need not exist: filetype detection is by name, and the startup
    // buffer is unmodified so `:e` abandons it freely.
    common::send_ex(&core, "e notes.txt");
    let vals = common::probe_values(
        &core,
        &["string(&l:spell)", "&filetype"],
        &probe_dir.path().join("text.txt"),
    );
    assert_eq!(
        vals[1], "text",
        "precondition: notes.txt must detect as filetype=text, or the spell assertion \
         below tests the wrong filetype"
    );
    assert_eq!(
        vals[0], "1",
        "'spell' must be ON for filetype=text (0 means text was dropped from the \
         WolfSpell prose set)"
    );

    common::send_ex(&core, "e COMMIT_EDITMSG");
    let vals = common::probe_values(
        &core,
        &["string(&l:spell)", "&filetype"],
        &probe_dir.path().join("gitcommit.txt"),
    );
    assert_eq!(
        vals[1], "gitcommit",
        "precondition: COMMIT_EDITMSG must detect as filetype=gitcommit, or the spell \
         assertion below tests the wrong filetype"
    );
    assert_eq!(
        vals[0], "1",
        "'spell' must be ON for filetype=gitcommit (0 means gitcommit was dropped from \
         the WolfSpell prose set)"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// The design pin for the `"*"`-pattern-with-explicit-off autocmd: after a
/// prose buffer enabled spell, switching the SAME window to a non-prose file
/// must leave it OFF. Measured on the pinned nvim: `vim.opt_local.spell` sets
/// only the window-LOCAL value, which nvim resets from the window-global value
/// (off) on a buffer switch — so an on-only three-pattern autocmd also yields
/// `&l:spell == 0` here, and what this test falsifies is an always-on `"*"`
/// callback. The explicit off is determinism hardening: it guarantees spell off
/// for non-prose regardless of the window-global value, and keeps holding even
/// if future code sets `vim.o.spell`.
#[test]
fn spell_disabled_for_non_prose_after_prose() {
    let (core, _waiter, _file) = common::start_core_with_temp_md("# Prose First\nbody\n");
    let probe_dir = tempfile::tempdir().expect("create probe dir");

    // Precondition: the prose file enabled spell in this window — without this,
    // the "0" below could mean spell never turned on at all.
    let vals = common::probe_values(
        &core,
        &["string(&l:spell)"],
        &probe_dir.path().join("prose.txt"),
    );
    assert_eq!(
        vals[0], "1",
        "precondition: the opened .md must have 'spell' ON before the window switches \
         to a non-prose file"
    );

    common::send_ex(&core, "e main.lua");
    let vals = common::probe_values(
        &core,
        &["string(&l:spell)", "&filetype"],
        &probe_dir.path().join("lua.txt"),
    );
    assert_eq!(
        vals[1], "lua",
        "precondition: main.lua must detect as filetype=lua, or the spell assertion \
         below tests the wrong filetype"
    );
    assert_eq!(
        vals[0], "0",
        "'spell' must end up OFF when the same window switches from a prose file to a \
         lua file: an autocmd that only ever turns spell on (an always-on \"*\" \
         callback), or anything that leaves spell on globally, fails this — and every \
         code file would get prose spell-check underlines"
    );

    core.shutdown().expect("shutdown should be Ok");
}
