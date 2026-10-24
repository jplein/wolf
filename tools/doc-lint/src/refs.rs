//! Root-relative Markdown paths written in comments: the `doc-ref` and `doc-anchor` rules.

use std::path::Path;

use crate::lexer::Comment;
use crate::markdown::SlugCache;
use crate::{Finding, Rule, WalkConfig};

/// The top-level directories a reference may start with.
const PREFIXES: [&str; 7] = [
    "notes/",
    "specs/",
    "crates/",
    "tools/",
    "runtime/",
    "bazel/",
    "resources/",
];

/// A Markdown path found in a comment, with its optional `#anchor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocRef {
    pub line: usize,
    pub path: String,
    pub anchor: Option<String>,
}

/// Every reference in `comments` (doc comments included) of the normalized `text`.
pub(crate) fn refs_in(text: &str, comments: &[Comment]) -> Vec<DocRef> {
    let mut out = Vec::new();
    for comment in comments {
        let body = &text[comment.byte_span.0..comment.byte_span.1];
        for (offset, line) in body.split('\n').enumerate() {
            scan_line(line, comment.start_line + offset, &mut out);
        }
    }
    out
}

fn scan_line(line: &str, line_no: usize, out: &mut Vec<DocRef>) {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let boundary = i == 0 || is_boundary(b[i - 1]);
        let prefixed = boundary && PREFIXES.iter().any(|p| b[i..].starts_with(p.as_bytes()));
        if !prefixed {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < b.len() && is_path_byte(b[end]) {
            end += 1;
        }
        if let Some(md_end) = md_end(&line[i..end]) {
            let path_end = i + md_end;
            let anchor = if path_end == end && b.get(end) == Some(&b'#') {
                let len = b[end + 1..]
                    .iter()
                    .take_while(|c| is_anchor_byte(**c))
                    .count();
                (len > 0).then(|| line[end + 1..end + 1 + len].to_string())
            } else {
                None
            };
            out.push(DocRef {
                line: line_no,
                path: line[i..path_end].to_string(),
                anchor,
            });
        }
        i = end.max(i + 1);
    }
}

/// The token truncated to its last `.md` that is not followed by a word char, as a length.
fn md_end(token: &str) -> Option<usize> {
    let b = token.as_bytes();
    (3..=b.len()).rev().find(|&end| {
        &b[end - 3..end] == b".md"
            && !b
                .get(end)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    })
}

fn is_boundary(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b'`' | b'\'' | b'"' | b'(' | b'[' | b'<')
}

fn is_path_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'@' | b'+' | b'/' | b'-')
}

fn is_anchor_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// `doc-ref` and `doc-anchor` findings for `refs`, resolved against `root`.
pub(crate) fn ref_findings(
    root: &Path,
    display: &str,
    refs: &[DocRef],
    cfg: &WalkConfig,
    cache: &mut SlugCache,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in refs.iter().filter(|r| !cfg.skips_prefix(&r.path)) {
        let target = root.join(&r.path);
        let (rule, message) = if !target.is_file() {
            (Rule::DocRef, format!("{} does not exist", r.path))
        } else {
            match &r.anchor {
                Some(anchor) if !cache.has(&target, anchor) => (
                    Rule::DocAnchor,
                    format!("{} has no heading with anchor #{anchor}", r.path),
                ),
                _ => continue,
            }
        };
        findings.push(Finding {
            path: display.to_string(),
            line: r.line,
            rule,
            message,
        });
    }
    findings
}
