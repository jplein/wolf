//! Fixture-driven tests for doc-lint: per-language sidecars, repo mode, `same-code`, and the CLI.
//!
//! The fixture layout and the sidecar format are described in `tools/doc-lint/tests/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use doc_lint::{check_root, language_for, lint_source, run, strip_comments, Finding, WalkConfig};

/// The `Debug` name of every `Language` variant. Each must have at least one fixture under `lang/`,
/// so a language whose fixtures were deleted or never mapped fails here rather than going untested.
const ALL_LANGUAGES: [&str; 10] = [
    "C", "Css", "HashLine", "Lua", "Nix", "Rust", "Shell", "Starlark", "Swift", "Toml",
];

/// The fixture root: `DOC_LINT_FIXTURES` when set (Bazel, relative to the runfiles root), else the
/// source tree's `tests/fixtures` (Cargo).
fn fixtures_dir() -> PathBuf {
    let dir = match std::env::var_os("DOC_LINT_FIXTURES") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")),
    };
    assert!(
        dir.is_dir(),
        "fixture dir {} does not exist: under Bazel the DOC_LINT_FIXTURES env or the `data` glob \
         of the rust_test is miswired; under Cargo tests/fixtures is missing",
        dir.display()
    );
    dir
}

/// The fixture root as a path relative to the working directory, for CLI output that echoes the
/// paths it was given. Cargo runs integration tests from the package root; Bazel runs them from
/// the runfiles root, where `DOC_LINT_FIXTURES` is already relative.
fn relative_fixtures_dir() -> String {
    let dir = std::env::var("DOC_LINT_FIXTURES").unwrap_or_else(|_| "tests/fixtures".to_string());
    assert!(
        Path::new(&dir).is_dir(),
        "relative fixture dir {dir} is not a directory from the cwd {:?}: Cargo is expected to run \
         this test from the package root, Bazel from the runfiles root",
        std::env::current_dir()
    );
    dir
}

/// Every regular file under `dir`, recursively, sorted. Follows symlinks on purpose: Bazel stages
/// the fixtures as a runfiles tree of symlinks.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current)
            .unwrap_or_else(|e| panic!("cannot read fixture dir {}: {e}", current.display()));
        for entry in entries {
            let path = entry
                .unwrap_or_else(|e| panic!("cannot list {}: {e}", current.display()))
                .path();
            let meta = fs::metadata(&path)
                .unwrap_or_else(|e| panic!("cannot stat fixture {}: {e}", path.display()));
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// `path` relative to `root`, with `/` separators.
fn slash_relative(root: &Path, path: &Path) -> String {
    let rel = path
        .strip_prefix(root)
        .unwrap_or_else(|_| panic!("{} is not under {}", path.display(), root.display()));
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", parent.display()));
    }
    fs::write(path, text).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}

/// Parses a sidecar: one `LINE RULE` pair per non-blank line; an empty file means no findings.
fn parse_sidecar(text: &str, sidecar: &Path) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(number), Some(rule), None) = (parts.next(), parts.next(), parts.next()) else {
            panic!(
                "{}:{}: malformed sidecar line {line:?}; the format is `LINE RULE`",
                sidecar.display(),
                index + 1
            );
        };
        let number: usize = number.parse().unwrap_or_else(|_| {
            panic!(
                "{}:{}: {number:?} is not a line number",
                sidecar.display(),
                index + 1
            )
        });
        out.push((number, rule.to_string()));
    }
    out.sort();
    out
}

/// The `(line, rule id)` multiset of `findings`, sorted. A duplicate report is a mismatch too.
fn line_rule_pairs(findings: &[Finding]) -> Vec<(usize, String)> {
    let mut pairs: Vec<(usize, String)> = findings
        .iter()
        .map(|f| (f.line, f.rule.id().to_string()))
        .collect();
    pairs.sort();
    pairs
}

fn render(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "    (none)".to_string();
    }
    findings
        .iter()
        .map(|f| format!("    {f}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A fresh scratch directory: under `TEST_TMPDIR` for Bazel, else the system temp dir.
fn scratch_dir(name: &str) -> PathBuf {
    let base = std::env::var_os("TEST_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("doc-lint-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    dir
}

/// Copies `from` to `to` as regular files, dereferencing symlinks, so that a walk that never follows
/// symlinks still sees the fixture under Bazel, whose runfiles are symlinks.
fn copy_tree(from: &Path, to: &Path) {
    for file in files_under(from) {
        let dest = to.join(slash_relative(from, &file));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("cannot create {}: {e}", parent.display()));
        }
        fs::copy(&file, &dest).unwrap_or_else(|e| {
            panic!("cannot copy {} to {}: {e}", file.display(), dest.display())
        });
    }
}

// ---- per-language fixtures ----

/// Every file under `lang/` with a known language is linted with `lint_source`, and its
/// `(line, rule)` findings must equal its `<file>.expected` sidecar exactly.
///
/// Fails loudly, rather than passing vacuously, when: no fixture is found; a fixture with a known
/// language has no sidecar; a sidecar exists for a file with no language or no source; a
/// `Language` variant has no fixture; or the no-language control `lang/other/notes.txt` is absent.
#[test]
fn language_fixtures_match_their_sidecars() {
    let root = fixtures_dir();
    let lang_dir = root.join("lang");
    let files = files_under(&lang_dir);
    assert!(
        !files.is_empty(),
        "no fixtures under {}: the fixture data is not staged (Bazel `data` glob) or was deleted",
        lang_dir.display()
    );

    let mut checked = 0usize;
    let mut languages = BTreeSet::new();
    let mut skipped = Vec::new();
    let mut failures = Vec::new();

    for path in &files {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(source_name) = name.strip_suffix(".expected") {
            let source = path.with_file_name(source_name);
            assert!(
                source.is_file(),
                "sidecar {} has no source fixture {}: a stale sidecar would hide a deleted case",
                path.display(),
                source.display()
            );
            continue;
        }

        let display = slash_relative(&root, path);
        let sidecar = path.with_file_name(format!("{name}.expected"));
        let Some(lang) = language_for(path) else {
            assert!(
                !sidecar.exists(),
                "{display} has a sidecar but language_for() maps it to None: either the \
                 file-name mapping for it was dropped, or the sidecar does not belong there"
            );
            skipped.push(display);
            continue;
        };

        assert!(
            sidecar.is_file(),
            "{display} has a known language ({lang:?}) but no sidecar {}: every linted \
             fixture needs one, even if empty, so an unexpected mapping cannot pass silently",
            sidecar.display()
        );
        let expected = parse_sidecar(&read(&sidecar), &sidecar);
        let findings = lint_source(&display, &read(path), lang);

        for finding in &findings {
            assert_eq!(
                finding.path, display,
                "lint_source must report the display path it was given, not another path"
            );
        }

        let got = line_rule_pairs(&findings);
        if got != expected {
            failures.push(format!(
                "{display} ({lang:?}):\n  expected (line, rule): {expected:?}\n  got (line, rule):      \
                 {got:?}\n  findings:\n{}",
                render(&findings)
            ));
        }
        languages.insert(format!("{lang:?}"));
        checked += 1;
    }

    assert!(
        checked > 0,
        "no fixture under {} mapped to a language, so nothing was linted",
        lang_dir.display()
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} language fixture(s) disagree with their sidecars:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );

    let want: BTreeSet<String> = ALL_LANGUAGES.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        languages, want,
        "the set of languages exercised by lang/ fixtures differs from every Language variant: a \
         missing entry means a language lost its fixtures or its file-name mapping"
    );
    assert!(
        skipped.iter().any(|s| s == "lang/other/notes.txt"),
        "the no-language control lang/other/notes.txt was not seen as skipped; skipped: {skipped:?}"
    );
}

/// Trailing whitespace after a 100-char comment is trimmed before measuring, and a CRLF line
/// ending is stripped. Built in code because the fixture files cannot carry trailing whitespace
/// reliably (editors and formatters strip it).
#[test]
fn trailing_whitespace_and_carriage_returns_do_not_count_toward_width() {
    let hundred = "// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd \
                   seg08abcd seg09abcd seg10ab";
    assert_eq!(
        hundred.chars().count(),
        100,
        "test premise: the base line is 100 chars"
    );
    let rust = language_for(Path::new("x.rs")).expect("x.rs must map to a language");

    let spaces = format!("{hundred}     \nfn a() {{}}\n");
    let findings = lint_source("spaces.rs", &spaces, rust);
    assert!(
        findings.is_empty(),
        "100 chars plus trailing spaces must pass: a width finding means trailing whitespace \
         is not trimmed before measuring; got:\n{}",
        render(&findings)
    );

    let tabs = format!("{hundred}\t \t\nfn a() {{}}\n");
    let findings = lint_source("tabs.rs", &tabs, rust);
    assert!(
        findings.is_empty(),
        "100 chars plus trailing tabs must pass: trailing whitespace includes tabs; got:\n{}",
        render(&findings)
    );

    let crlf = format!("{hundred}\r\n// two\r\nfn a() {{}}\r\n");
    let findings = lint_source("crlf.rs", &crlf, rust);
    assert!(
        findings.is_empty(),
        "a 100-char line ending in CRLF must pass: a width finding means \\r was counted; got:\n{}",
        render(&findings)
    );

    let control = format!("{hundred}x     \nfn a() {{}}\n");
    let pairs = line_rule_pairs(&lint_source("control.rs", &control, rust));
    assert_eq!(
        pairs,
        vec![(1, "comment-width".to_string())],
        "control: 101 visible chars plus trailing spaces must still fail comment-width, or the \
         passes above prove nothing"
    );
}

// ---- repo mode ----

const THREE_LINE_COMMENT: &str = "// one\n// two\n// three\nfn x() {}\n";

/// `check_root` over `repo/` reports exactly the planted findings: doc-ref and doc-anchor from
/// comments (doc comments included, strings and placeholders excluded), notes-orphan and
/// notes-unlinked for mirrors, md-link and md-anchor for Markdown, and nothing from skipped dirs,
/// skipped prefixes or symlinks.
///
/// The fixture is copied to a scratch dir first, since Bazel's runfiles are symlinks and the
/// walk never follows them. The space-named dir, `node_modules/`, the generated dir and the
/// symlinks are created at run time: Bazel runfiles have historically rejected paths with spaces,
/// `node_modules/` is commonly ignored by a global gitignore, which would drop it from commits.
#[test]
fn repo_mode_reports_exactly_the_planted_findings() {
    let scratch = scratch_dir("repo");
    let root = scratch.join("root");
    copy_tree(&fixtures_dir().join("repo"), &root);
    assert!(
        root.join("src/a.rs").is_file() && root.join("docs/x.md").is_file(),
        "the repo fixture did not copy: src/a.rs or docs/x.md is missing under {}",
        root.display()
    );

    write_file(
        &root.join("docs/dir with space/f.md"),
        "# F\n\nThe target of a percent-encoded link.\n",
    );
    write_file(&root.join("node_modules/x.rs"), THREE_LINE_COMMENT);
    write_file(
        &root.join("crates/wolf-ffi/generated/g.swift"),
        THREE_LINE_COMMENT,
    );
    let outside = scratch.join("outside");
    write_file(&outside.join("bad.rs"), THREE_LINE_COMMENT);
    std::os::unix::fs::symlink(&outside, root.join("linked"))
        .expect("cannot create the directory symlink fixture");
    std::os::unix::fs::symlink(outside.join("bad.rs"), root.join("src/linked_file.rs"))
        .expect("cannot create the file symlink fixture");

    let findings = check_root(&root, &WalkConfig::repo_default());

    let mut got: Vec<(String, usize, String)> = findings
        .iter()
        .map(|f| (f.path.clone(), f.line, f.rule.id().to_string()))
        .collect();
    got.sort();
    let mut want: Vec<(String, usize, String)> = [
        ("docs/x.md", 7, "md-link"),
        ("docs/x.md", 13, "md-anchor"),
        ("docs/x.md", 28, "md-link"),
        ("notes/src/a.rs.md", 21, "notes-unlinked"),
        ("notes/src/deleted.rs.md", 1, "notes-orphan"),
        ("src/a.rs", 10, "doc-ref"),
        ("src/a.rs", 13, "doc-anchor"),
        ("src/a.rs", 16, "doc-ref"),
    ]
    .iter()
    .map(|(p, l, r)| (p.to_string(), *l, r.to_string()))
    .collect();
    want.sort();
    assert_eq!(
        got,
        want,
        "check_root findings differ from the planted set (paths are root-relative). An extra \
         src/a.rs doc-anchor or notes-unlinked on a Dup or foo heading means the slug kept \
         backticks or skipped the -1 suffix; a missing src/a.rs:16 means doc comments are not \
         scanned; anything under node_modules, crates/wolf-ffi/generated, linked or \
         src/linked_file.rs means a skip rule or the no-symlink rule is broken. Findings:\n{}",
        render(&findings)
    );

    let messages = [
        ("src/a.rs", 10, "notes/src/missing.rs.md does not exist"),
        (
            "src/a.rs",
            13,
            "notes/src/a.rs.md has no heading with anchor #nope",
        ),
        ("src/a.rs", 16, "specs/nope.md does not exist"),
        (
            "notes/src/deleted.rs.md",
            1,
            "notes/src/deleted.rs.md mirrors src/deleted.rs, which does not exist",
        ),
        (
            "notes/src/a.rs.md",
            21,
            "heading \"Unlinked\" (#unlinked) is not linked from any comment in src/a.rs",
        ),
    ];
    for (path, line, message) in messages {
        let finding = findings
            .iter()
            .find(|f| f.path == path && f.line == line)
            .unwrap_or_else(|| panic!("no finding at {path}:{line}"));
        assert_eq!(
            finding.message, message,
            "message wording at {path}:{line} differs from the documented rule text"
        );
    }

    let _ = fs::remove_dir_all(&scratch);
}

// ---- strip and same-code ----

fn same_code_path(name: &str) -> PathBuf {
    fixtures_dir().join("same_code").join(name)
}

/// Strips `name` using the language of its own file name, as `same-code` does for `<new>`.
fn stripped(name: &str) -> String {
    let path = same_code_path(name);
    let lang =
        language_for(&path).unwrap_or_else(|| panic!("{} must map to a language", path.display()));
    strip_comments(&read(&path), lang)
}

#[test]
fn strip_removes_every_comment_and_keeps_strings_verbatim() {
    let old = stripped("old.rs");
    for comment_text in [
        "Crate docs",
        "Adds one",
        "Returns",
        "trailing note",
        "inline note",
    ] {
        assert!(
            !old.contains(comment_text),
            "stripped old.rs still contains comment text {comment_text:?}: strip must remove \
             every comment, doc comments included; stripped:\n{old}"
        );
    }
    assert!(
        old.contains("let url = \"see // the docs\";"),
        "stripped old.rs lost the string holding //: strip must keep strings verbatim; \
         stripped:\n{old}"
    );
    assert!(
        old.lines().all(|l| l == l.trim_end()),
        "stripped old.rs keeps trailing whitespace where a comment was removed:\n{old}"
    );
}

#[test]
fn a_comment_only_edit_strips_to_identical_code() {
    assert_eq!(
        stripped("old.rs"),
        stripped("comment_only.rs"),
        "old.rs and comment_only.rs differ only in comments, so their strips must be equal; a \
         difference in line count means emptied comment lines were kept"
    );
}

#[test]
fn a_one_token_code_edit_strips_to_different_code() {
    assert_ne!(
        stripped("old.rs"),
        stripped("code_changed.rs"),
        "code_changed.rs changes `x + 1` to `x + 2`; equal strips mean strip drops code"
    );
}

#[test]
fn an_edit_inside_a_string_holding_slashes_is_a_code_change() {
    assert_ne!(
        stripped("old.rs"),
        stripped("string_changed.rs"),
        "string_changed.rs edits text after // inside a string literal; equal strips mean strip \
         treats // inside strings as a comment (a regex strip)"
    );
}

#[test]
fn nix_comment_edits_are_equal_but_edits_inside_indented_strings_are_not() {
    assert_eq!(
        stripped("old.nix"),
        stripped("comment_only.nix"),
        "comment_only.nix edits only a Nix # comment; unequal strips mean Nix comments survive \
         strip"
    );
    assert_ne!(
        stripped("old.nix"),
        stripped("string_comment.nix"),
        "string_comment.nix edits a # line inside a '' string, which is string content; equal \
         strips mean strip removed it as a comment"
    );
}

// ---- CLI ----

fn run_cli(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|a| String::from(*a)).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&args, &mut out, &mut err);
    let out = String::from_utf8(out).expect("stdout must be UTF-8");
    let err = String::from_utf8(err).expect("stderr must be UTF-8");
    (code, out, err)
}

#[test]
fn cli_clean_file_exits_zero_and_prints_no_findings() {
    let path = format!("{}/cli/clean.rs", relative_fixtures_dir());
    let (code, out, err) = run_cli(&["--files", &path]);
    assert_eq!(
        code, 0,
        "a clean file must exit 0 (1 = a finding was reported, 2 = usage or I/O error); \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert_eq!(
        out, "",
        "a clean run must print no findings on stdout; stderr:\n{err}"
    );
}

#[test]
fn cli_one_finding_exits_one_with_the_exact_finding_line() {
    let path = format!("{}/cli/three_lines.rs", relative_fixtures_dir());
    let (code, out, err) = run_cli(&["--files", &path]);
    assert_eq!(
        code, 1,
        "a file with one finding must exit 1 (0 = finding missed, 2 = usage or I/O error); \
         stdout:\n{out}\nstderr:\n{err}"
    );
    let want = format!(
        "{path}:3: [comment-lines] comment is 3 lines (max 2): cut it, shorten it, or move the \
         rest to notes/{path}.md under a heading and link it\n"
    );
    assert_eq!(
        out, want,
        "stdout must hold exactly the one finding, in `path:line: [rule] message` form; \
         summaries belong on stderr. stderr:\n{err}"
    );
}

#[test]
fn cli_findings_are_sorted_by_path_then_line() {
    let dir = relative_fixtures_dir();
    let later = format!("{dir}/cli/three_lines.rs");
    let earlier = format!("{dir}/cli/a_first.rs");
    let (code, out, err) = run_cli(&["--files", &later, &earlier]);
    assert_eq!(
        code, 1,
        "two files with findings must exit 1; stderr:\n{err}"
    );
    let prefixes: Vec<&str> = out
        .lines()
        .map(|l| l.split(" [").next().unwrap_or(""))
        .collect();
    let first = format!("{earlier}:1:");
    let second = format!("{later}:3:");
    assert_eq!(
        prefixes,
        vec![first.as_str(), second.as_str()],
        "findings must be sorted by path, not by argument order; stdout:\n{out}"
    );
}

#[test]
fn cli_nonexistent_file_exits_two() {
    let path = format!("{}/cli/does-not-exist.rs", relative_fixtures_dir());
    let (code, out, err) = run_cli(&["--files", &path]);
    assert_eq!(
        code, 2,
        "a missing input file is an I/O error and must exit 2 (0 = silently skipped, \
         1 = reported as a finding); stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        !err.is_empty(),
        "an I/O error must explain itself on stderr"
    );
}

#[test]
fn cli_without_arguments_or_with_an_unknown_flag_exits_two() {
    let (code, out, err) = run_cli(&[]);
    assert_eq!(
        code, 2,
        "no arguments is a usage error and must exit 2; stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(!err.is_empty(), "a usage error must print usage on stderr");

    let (code, out, err) = run_cli(&["--bogus"]);
    assert_eq!(
        code, 2,
        "an unknown flag is a usage error and must exit 2; stdout:\n{out}\nstderr:\n{err}"
    );
}

#[test]
fn cli_same_code_exits_zero_for_comment_edits_and_one_for_code_edits() {
    let old_path = same_code_path("old.rs");
    let old = old_path.to_string_lossy();
    let comment_only = same_code_path("comment_only.rs");
    let code_changed = same_code_path("code_changed.rs");

    let (code, out, err) = run_cli(&["same-code", &old, &comment_only.to_string_lossy()]);
    assert_eq!(
        code, 0,
        "same-code on a comment-only edit must exit 0; stdout:\n{out}\nstderr:\n{err}"
    );

    let (code, out, err) = run_cli(&["same-code", &old, &code_changed.to_string_lossy()]);
    assert_eq!(
        code, 1,
        "same-code on a code edit must exit 1 (0 = change missed, 2 = usage or I/O error); \
         stdout:\n{out}\nstderr:\n{err}"
    );
    assert!(
        out.contains("x + 2") || err.contains("x + 2"),
        "same-code must print the first differing line, which holds `x + 2`; \
         stdout:\n{out}\nstderr:\n{err}"
    );
}
