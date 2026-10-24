//! The `--files` CLI mode resolving doc references against the working directory.
//!
//! This is its own test binary holding a single test, because the test changes the process cwd
//! and any other test in the same process would race with it. Do not add a second test here.

use std::path::PathBuf;

use doc_lint::run;

/// The `files_mode/` fixture root as an absolute path, so it survives the change of cwd.
/// `DOC_LINT_FIXTURES` is relative to the runfiles root under Bazel; Cargo uses the source tree.
fn files_mode_root() -> PathBuf {
    let fixtures = match std::env::var_os("DOC_LINT_FIXTURES") {
        Some(dir) => std::env::current_dir()
            .expect("the cwd must be readable to resolve DOC_LINT_FIXTURES")
            .join(dir),
        None => PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")),
    };
    let root = fixtures.join("files_mode");
    assert!(
        root.is_dir(),
        "fixture dir {} does not exist: under Bazel the DOC_LINT_FIXTURES env or the `data` glob \
         of the rust_test is miswired; under Cargo tests/fixtures/files_mode is missing",
        root.display()
    );
    root
}

/// `--files` resolves a root-relative reference against the cwd, not against the directory of the
/// file that holds it. Bazel runs `//tools/lint:comments` from the runfiles root, which is the
/// repository root there, so this is the base every reference in the tree is written against.
///
/// With the cwd at the `files_mode/` root, `src/refs.rs` holds one reference that resolves there,
/// anchor included, and one that does not. The root lacks the second target, but `src/` holds a
/// decoy of that name and lacks the first, so a resolver that uses the file's directory moves the
/// single finding to the other line instead of removing it.
#[test]
fn files_mode_resolves_references_against_the_working_directory() {
    let root = files_mode_root();
    for (rel, should_exist) in [
        ("notes/kept.md", true),
        ("notes/gone.md", false),
        ("src/notes/gone.md", true),
        ("src/notes/kept.md", false),
    ] {
        assert_eq!(
            root.join(rel).is_file(),
            should_exist,
            "fixture premise: {rel} must {} under {}; without it the test cannot tell resolution \
             against the cwd from resolution against the file's directory",
            if should_exist { "exist" } else { "be absent" },
            root.display()
        );
    }

    std::env::set_current_dir(&root)
        .unwrap_or_else(|e| panic!("cannot change the cwd to {}: {e}", root.display()));
    let args = ["--files".to_string(), "src/refs.rs".to_string()];
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&args, &mut out, &mut err);
    let out = String::from_utf8(out).expect("stdout must be UTF-8");
    let err = String::from_utf8(err).expect("stderr must be UTF-8");

    assert_eq!(
        code, 1,
        "one unresolvable reference must exit 1 (0 = no reference was reported, so --files skipped \
         doc-ref or resolved both; 2 = usage or I/O error, e.g. src/refs.rs not read relative to \
         the cwd); stdout:\n{out}\nstderr:\n{err}"
    );
    assert_eq!(
        out, "src/refs.rs:6: [doc-ref] notes/gone.md does not exist\n",
        "stdout must hold exactly the one doc-ref for the missing file. A lone doc-ref at line 2 \
         for notes/kept.md means references resolve against the file's directory (src/), where \
         the decoy sits; doc-refs at both lines mean they resolve against neither the cwd nor \
         src/; a doc-anchor at line 2 means the anchor lookup or slug is broken; a comment-lines \
         or comment-width finding means the fixture itself breaks the limits. stderr:\n{err}"
    );
}
