//! doc-lint: comment limits, doc-path references, `notes/` mirrors and Markdown links.
//!
//! The rules, their wording and the CLI are described in `tools/doc-lint/README.md`.

mod cli;
mod coverage;
mod lexer;
mod markdown;
mod notes;
mod refs;
mod rules;
mod strip;
mod walk;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use cli::run;
pub use coverage::missing_coverage;
pub use markdown::heading_slug;
pub use strip::strip_comments;

/// The most physical lines a non-doc comment block may span.
pub const MAX_COMMENT_LINES: usize = 2;

/// The most characters a non-doc comment line may hold, counted from its comment marker.
pub const MAX_COMMENT_WIDTH: usize = 100;

/// A check doc-lint performs. Each finding names exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    CommentLines,
    CommentWidth,
    DocRef,
    DocAnchor,
    NotesOrphan,
    NotesUnlinked,
    MdLink,
    MdAnchor,
    BazelCoverage,
}

impl Rule {
    /// The rule's id as printed between brackets in a finding.
    pub fn id(&self) -> &'static str {
        match self {
            Rule::CommentLines => "comment-lines",
            Rule::CommentWidth => "comment-width",
            Rule::DocRef => "doc-ref",
            Rule::DocAnchor => "doc-anchor",
            Rule::NotesOrphan => "notes-orphan",
            Rule::NotesUnlinked => "notes-unlinked",
            Rule::MdLink => "md-link",
            Rule::MdAnchor => "md-anchor",
            Rule::BazelCoverage => "bazel-coverage",
        }
    }
}

/// One violation, displayed as `path:line: [rule] message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The path as the caller named it: root-relative in `--root` mode, as given in `--files`.
    pub path: String,
    /// 1-based.
    pub line: usize,
    pub rule: Rule,
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: [{}] {}",
            self.path,
            self.line,
            self.rule.id(),
            self.message
        )
    }
}

/// A comment syntax. Every variant has a row in the language table of the README.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Swift,
    C,
    Css,
    Lua,
    Shell,
    Starlark,
    Nix,
    Toml,
    HashLine,
}

/// The language of a file, from its name alone; `None` means doc-lint never reads it.
pub fn language_for(path: &Path) -> Option<Language> {
    let name = path.file_name()?.to_str()?;
    let by_name = match name {
        "BUILD.bazel" | "BUILD" | "MODULE.bazel" => Some(Language::Starlark),
        ".envrc" => Some(Language::Shell),
        ".gitignore" | ".prettierignore" | ".bazelignore" | ".bazelrc" => Some(Language::HashLine),
        _ if name.starts_with("WORKSPACE") => Some(Language::Starlark),
        _ if name.ends_with(".sh.tpl") => Some(Language::Shell),
        _ => None,
    };
    if by_name.is_some() {
        return by_name;
    }
    let (_, ext) = name.rsplit_once('.')?;
    match ext {
        "rs" => Some(Language::Rust),
        "swift" => Some(Language::Swift),
        "c" | "h" => Some(Language::C),
        "css" => Some(Language::Css),
        "lua" => Some(Language::Lua),
        "sh" => Some(Language::Shell),
        "bzl" => Some(Language::Starlark),
        "nix" => Some(Language::Nix),
        "toml" => Some(Language::Toml),
        _ => None,
    }
}

/// Which directories and path prefixes a walk skips, and which references are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkConfig {
    /// Directory names skipped wherever they occur; a leading or trailing `*` is a wildcard.
    pub skip_dir_names: Vec<String>,
    /// Root-relative prefixes, ending in `/`, that are neither walked nor checked as references.
    pub skip_prefixes: Vec<String>,
}

impl WalkConfig {
    /// The exclusions for this repository, listed in the README.
    pub fn repo_default() -> WalkConfig {
        let names = [
            ".git",
            "target",
            ".direnv",
            ".worktrees",
            ".claude",
            "node_modules",
            ".wolf",
            "bazel-*",
            "*.xcodeproj",
        ];
        let prefixes = [
            "crates/wolf-ffi/generated/",
            "tools/doc-lint/tests/fixtures/",
            "tools/lint/testdata/",
        ];
        WalkConfig {
            skip_dir_names: names.iter().map(|s| s.to_string()).collect(),
            skip_prefixes: prefixes.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn skips_prefix(&self, rel: &str) -> bool {
        self.skip_prefixes
            .iter()
            .any(|p| rel.starts_with(p.as_str()))
    }
}

/// The comment-limit findings for one source text. `display_path` is used verbatim in findings.
pub fn lint_source(display_path: &str, text: &str, lang: Language) -> Vec<Finding> {
    let text = lexer::normalize(text);
    let lexed = lexer::lex(&text, lang);
    rules::limit_findings(display_path, &lexed.comments)
}

/// The comment rules plus `doc-ref`/`doc-anchor` for `files`, resolved against `cwd`.
///
/// Files with no language are skipped. An unreadable file is an error, not a finding.
pub fn check_files(cwd: &Path, files: &[PathBuf], cfg: &WalkConfig) -> io::Result<Vec<Finding>> {
    let mut cache = markdown::SlugCache::default();
    let mut findings = Vec::new();
    for file in files {
        let display = display_path(file);
        let Some(lang) = language_for(file) else {
            continue;
        };
        let bytes = fs::read(cwd.join(file))
            .map_err(|e| io::Error::new(e.kind(), format!("{display}: {e}")))?;
        let text = String::from_utf8_lossy(&bytes);
        findings.extend(source_findings(cwd, &display, &text, lang, cfg, &mut cache));
    }
    sort_findings(&mut findings);
    Ok(findings)
}

/// Every rule over the tree under `root`, with root-relative paths. Unreadable files are skipped.
pub fn check_root(root: &Path, cfg: &WalkConfig) -> Vec<Finding> {
    let files = walk::walk(root, cfg);
    let mut cache = markdown::SlugCache::default();
    let mut findings = Vec::new();
    for rel in &files {
        let Some(text) = read_lossy(&root.join(rel)) else {
            continue;
        };
        if let Some(lang) = language_for(Path::new(rel)) {
            findings.extend(source_findings(root, rel, &text, lang, cfg, &mut cache));
        } else if rel.ends_with(".md") {
            findings.extend(markdown::link_findings(root, rel, &text, &mut cache));
            findings.extend(notes::mirror_findings(root, rel, &text));
        }
    }
    findings.extend(coverage::coverage_findings(root, &files));
    sort_findings(&mut findings);
    findings
}

fn source_findings(
    root: &Path,
    display: &str,
    text: &str,
    lang: Language,
    cfg: &WalkConfig,
    cache: &mut markdown::SlugCache,
) -> Vec<Finding> {
    let text = lexer::normalize(text);
    let lexed = lexer::lex(&text, lang);
    let mut findings = rules::limit_findings(display, &lexed.comments);
    let found = refs::refs_in(&text, &lexed.comments);
    findings.extend(refs::ref_findings(root, display, &found, cfg, cache));
    findings
}

fn display_path(path: &Path) -> String {
    let shown = path.to_string_lossy().replace('\\', "/");
    match shown.strip_prefix("./") {
        Some(rest) => rest.to_string(),
        None => shown,
    }
}

pub(crate) fn read_lossy(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub(crate) fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        (a.path.as_str(), a.line, a.rule, a.message.as_str()).cmp(&(
            b.path.as_str(),
            b.line,
            b.rule,
            b.message.as_str(),
        ))
    });
}
