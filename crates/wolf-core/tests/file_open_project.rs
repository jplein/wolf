//! Headless tests of `wolf_core::OpenTarget::of`, which decides directory versus file as
//! `specs/cli-and-file-open.md` §1 says, and of `wolf_core::project_directory`, which finds the
//! project directory `P` of §5.1 step 2. They run against real files in a temp root `T` and real
//! repositories the `git` binary builds: a plain repository, a nested open project, a submodule
//! under an open superproject, and a linked worktree inside its main checkout.
//!
//! ## Premises about the components
//!
//! Each is asserted before a test relies on it, panicking with an explanation rather than
//! skipping:
//!
//! - No ancestor of `T` holds a `.git`, so every repository a walk finds is one a test built.
//! - `git worktree add` writes a `.git` file in the worktree, and `git rev-parse
//!   --git-common-dir` there resolves to the main checkout's `.git`. `project_directory` reads
//!   `.git` and `commondir` rather than running `git`; this measures what it must agree with.
//! - `git submodule add` writes a `.git` file in the submodule whose common dir is not the
//!   superproject's `.git`, so a submodule is never a worktree of its superproject.
//!
//! `git` runs with no system or global configuration, set per `Command`, never process-wide.
//! Every expected path is canonical, as tab directories are.

use std::path::{Path, PathBuf};
use std::process::Command;

use wolf_core::{project_directory, OpenTarget};

/// The file every repository's first commit holds.
const COMMITTED: &str = "a.md";

/// A canonical temp root `T`, kept alive with its guard.
struct Root {
    _guard: tempfile::TempDir,
    path: PathBuf,
}

impl Root {
    fn new() -> Root {
        let guard = tempfile::tempdir().expect("failed to create the temp root");
        let path = std::fs::canonicalize(guard.path()).expect("failed to canonicalize the root");
        if let Some(above) = path.ancestors().find(|dir| dir.join(".git").exists()) {
            panic!(
                "premise: no ancestor of the temp root {} may hold a .git, but {} does. A walk \
                 from a file outside every test repository would find it, so the no-repository \
                 cases could not pass. Point TMPDIR outside any repository.",
                path.display(),
                above.display()
            );
        }
        Root {
            _guard: guard,
            path,
        }
    }

    fn join(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }
}

/// Runs `git <args>` in `cwd` with a hermetic configuration, asserting success, and returns its
/// trimmed stdout.
fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Wolf Test",
            "-c",
            "user.email=wolf-test@example.invalid",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "protocol.file.allow=always",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("failed to spawn git; it must be on PATH, as file_search.rs also needs");
    assert!(
        output.status.success(),
        "`git {args:?}` in {} failed with {:?}: {}",
        cwd.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a fixture file has a parent"))
        .unwrap_or_else(|e| panic!("failed to create the parent of {}: {e}", path.display()));
    std::fs::write(path, text)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
}

fn mkdir(path: &Path) {
    std::fs::create_dir_all(path)
        .unwrap_or_else(|e| panic!("failed to create {}: {e}", path.display()));
}

fn symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap_or_else(|e| {
        panic!(
            "failed to link {} to {}: {e}",
            link.display(),
            target.display()
        )
    });
}

/// Makes `dir` a repository with one commit holding [`COMMITTED`].
fn init_repo(dir: &Path) {
    mkdir(dir);
    git(dir, &["init", "-q"]);
    write(&dir.join(COMMITTED), "# committed\n");
    git(dir, &["add", COMMITTED]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// `git rev-parse --git-common-dir` in `dir`, resolved against `dir` and canonicalized.
fn common_dir(dir: &Path) -> PathBuf {
    let raw = git(dir, &["rev-parse", "--git-common-dir"]);
    let path = dir.join(&raw);
    std::fs::canonicalize(&path).unwrap_or_else(|e| {
        panic!(
            "failed to canonicalize the common dir {}: {e}",
            path.display()
        )
    })
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|e| panic!("failed to canonicalize {}: {e}", path.display()))
}

/// Runs each `(row, file, open, expected)` through `project_directory`.
fn check(rows: Vec<(&str, PathBuf, Vec<PathBuf>, PathBuf)>, meaning: &str) {
    for (row, file, open, expected) in rows {
        let got = project_directory(&file, &open);
        assert_eq!(
            got,
            expected,
            "row {row}: project_directory({}, {open:?}) gave {} where {} is right. {meaning}",
            file.display(),
            got.display(),
            expected.display()
        );
    }
}

#[test]
fn open_target_decides_directory_versus_file() {
    let t = Root::new();
    let d = t.join("d");
    mkdir(&d);
    mkdir(&t.join("x"));
    let r = t.join("r");
    write(&r.join("docs/a.md"), "# a\n");
    let f = t.join("f.md");
    write(&f, "# f\n");
    symlink(&d, &t.join("dlink"));
    symlink(&r, &t.join("link-to-r"));
    symlink(&f, &t.join("alias.md"));

    let rows: Vec<(&str, Option<PathBuf>, OpenTarget)> = vec![
        ("no path", None, OpenTarget::Activate),
        (
            "a directory",
            Some(d.clone()),
            OpenTarget::Directory(d.clone()),
        ),
        (
            "a link to a directory",
            Some(t.join("dlink")),
            OpenTarget::Directory(d.clone()),
        ),
        (
            "an existing file",
            Some(f.clone()),
            OpenTarget::File(f.clone()),
        ),
        (
            "T/x/../d",
            Some(t.join("x/../d")),
            OpenTarget::Directory(d.clone()),
        ),
        (
            "a file under a linked directory",
            Some(t.join("link-to-r/docs/a.md")),
            OpenTarget::File(r.join("docs/a.md")),
        ),
        (
            "a missing file under a linked directory",
            Some(t.join("link-to-r/missing/new.md")),
            OpenTarget::File(r.join("missing/new.md")),
        ),
        (
            "a missing file under missing directories",
            Some(t.join("new/deeper/f.md")),
            OpenTarget::File(t.join("new/deeper/f.md")),
        ),
        (
            "a link to a file, whose last component is kept",
            Some(t.join("alias.md")),
            OpenTarget::File(t.join("alias.md")),
        ),
    ];
    for (row, path, expected) in rows {
        let got = OpenTarget::of(path.as_deref());
        assert_eq!(
            got, expected,
            "row {row}: OpenTarget::of({path:?}) gave {got:?} where {expected:?} is right. A \
             path that is a directory, through links too, is a Directory of its canonical path; \
             anything else is a File whose directory is canonical and whose last component is \
             kept. File for a link to a directory means the link was not followed; an unresolved \
             link-to-r means the File path was not canonicalized; T/f.md for alias.md means the \
             last component was resolved."
        );
    }
}

#[test]
fn a_plain_repository_resolves_to_its_root() {
    let t = Root::new();
    let r = t.join("repo");
    init_repo(&r);
    write(&r.join("docs/a.md"), "# a\n");
    check(
        vec![
            (
                "R/docs/a.md, nothing open",
                r.join("docs/a.md"),
                vec![],
                r.clone(),
            ),
            (
                "missing R/new/deeper/f.md, nothing open",
                r.join("new/deeper/f.md"),
                vec![],
                r.clone(),
            ),
        ],
        "With nothing open, P is the repository root. T/repo/new/deeper or T/repo/new for the \
         missing file means the walk did not find .git from the nearest existing ancestor.",
    );
}

#[test]
fn the_nearest_open_directory_inside_the_root_wins() {
    let t = Root::new();
    let r = t.join("repo");
    init_repo(&r);
    let docs = r.join("docs");
    write(&docs.join("a.md"), "# a\n");
    write(&r.join("other/c.md"), "# c\n");
    check(
        vec![
            (
                "R/docs/a.md, open [R, R/docs]",
                docs.join("a.md"),
                vec![r.clone(), docs.clone()],
                docs.clone(),
            ),
            (
                "R/docs/a.md, open [R/docs, R]",
                docs.join("a.md"),
                vec![docs.clone(), r.clone()],
                docs.clone(),
            ),
            (
                "R/b.md, open [R, R/docs]",
                r.join("b.md"),
                vec![r.clone(), docs.clone()],
                r.clone(),
            ),
            (
                "R/other/c.md, open [R/docs]",
                r.join("other/c.md"),
                vec![docs.clone()],
                r.clone(),
            ),
        ],
        "Rule 1 takes the nearest open directory that contains F inside R. R for R/docs/a.md \
         means the farthest was taken; R/docs for a file outside it means containment was not \
         checked.",
    );
}

#[test]
fn an_open_directory_above_a_plain_root() {
    let t = Root::new();
    let outer = t.join("outer");
    let r = outer.join("repo");
    init_repo(&r);
    let file = r.join(COMMITTED);
    check(
        vec![
            (
                "open [T/outer]",
                file.clone(),
                vec![outer.clone()],
                outer.clone(),
            ),
            (
                "open [T, T/outer]",
                file.clone(),
                vec![t.path.clone(), outer.clone()],
                outer.clone(),
            ),
        ],
        "Rule 2 takes the nearest open directory above a plain repository's root. T/outer/repo \
         means rule 2 was skipped; T means the farthest was taken.",
    );
}

#[test]
fn a_submodule_resolves_to_its_open_superproject() {
    let t = Root::new();
    let source = t.join("subsrc");
    init_repo(&source);
    let s = t.join("super");
    init_repo(&s);
    let source_arg = source.to_str().expect("the temp root is UTF-8");
    git(&s, &["submodule", "add", "-q", source_arg, "sub"]);
    let sub = s.join("sub");
    assert!(
        sub.join(".git").is_file(),
        "premise: `git submodule add` must leave a .git file in {}, which is what makes it a \
         submodule rather than a nested repository.",
        sub.display()
    );
    assert_ne!(
        common_dir(&sub),
        canonical(&s.join(".git")),
        "premise: a submodule's git common dir must not be its superproject's .git."
    );
    write(&sub.join("x.md"), "# x\n");
    check(
        vec![
            (
                "S/sub/x.md, open [S]",
                sub.join("x.md"),
                vec![s.clone()],
                s.clone(),
            ),
            (
                "S/sub/x.md, nothing open",
                sub.join("x.md"),
                vec![],
                sub.clone(),
            ),
        ],
        "A submodule's root is S/sub, and rule 2 takes its open superproject S. S/sub with S \
         open means every .git file is taken for a linked worktree; S with nothing open means \
         only a .git directory counts as a repository root.",
    );
}

#[test]
fn a_linked_worktree_never_resolves_to_its_main_checkout() {
    let t = Root::new();
    let m = t.join("main");
    init_repo(&m);
    git(&m, &["worktree", "add", "-q", "wt"]);
    let wt = m.join("wt");
    assert!(
        wt.join(".git").is_file(),
        "premise: `git worktree add` must leave a .git file in {}.",
        wt.display()
    );
    assert_eq!(
        common_dir(&wt),
        canonical(&m.join(".git")),
        "premise: `git rev-parse --git-common-dir` in a linked worktree must resolve to its main \
         checkout's .git, which is the relation §5.1 rule 2 tests."
    );
    let file = wt.join(COMMITTED);
    assert!(
        file.is_file(),
        "premise: the worktree must check out {COMMITTED}."
    );
    check(
        vec![
            ("open [M]", file.clone(), vec![m.clone()], wt.clone()),
            (
                "open [M, M/wt]",
                file.clone(),
                vec![m.clone(), wt.clone()],
                wt.clone(),
            ),
            ("nothing open", file.clone(), vec![], wt.clone()),
            (
                "open [T, M]",
                file.clone(),
                vec![t.path.clone(), m.clone()],
                wt.clone(),
            ),
            (
                "M/a.md, open [M]",
                m.join(COMMITTED),
                vec![m.clone()],
                m.clone(),
            ),
        ],
        "A file in a linked worktree resolves to the worktree, never to its main checkout M. M \
         means the worktree exception is missing; T on the [T, M] row means the exception went \
         on to the next open directory up rather than yielding R.",
    );
}

#[test]
fn a_file_outside_any_repository() {
    let t = Root::new();
    let notes = t.join("notes");
    write(&notes.join("a.md"), "# a\n");
    check(
        vec![
            (
                "T/notes/a.md, nothing open",
                notes.join("a.md"),
                vec![],
                notes.clone(),
            ),
            (
                "T/notes/a.md, open [T]",
                notes.join("a.md"),
                vec![t.path.clone()],
                t.path.clone(),
            ),
            (
                "T/notes/a.md, open [T, T/notes]",
                notes.join("a.md"),
                vec![t.path.clone(), notes.clone()],
                notes.clone(),
            ),
            (
                "T/notes/a.md, open [\"\"]",
                notes.join("a.md"),
                vec![PathBuf::new()],
                notes.clone(),
            ),
            (
                "missing T/notes/new/b.md, nothing open",
                notes.join("new/b.md"),
                vec![],
                notes.clone(),
            ),
        ],
        "With no repository, P is the nearest open directory containing F, or else F's \
         directory, or its nearest existing ancestor. T/notes with T open means open directories \
         are ignored; T/notes/new means the lexical parent of a missing file was used; an empty \
         path means an empty open entry was taken to contain everything.",
    );
}

#[test]
fn paths_compare_canonically() {
    let t = Root::new();
    let r = t.join("repo");
    init_repo(&r);
    let docs = r.join("docs");
    write(&docs.join("a.md"), "# a\n");
    symlink(&r, &t.join("link"));
    check(
        vec![(
            "T/link/docs/a.md, open [R/docs]",
            t.join("link/docs/a.md"),
            vec![docs.clone()],
            docs.clone(),
        )],
        "A file reached through a link to R resolves as if named through R. T/link means F's \
         directory was not canonicalized before the walk and the comparisons.",
    );
}
