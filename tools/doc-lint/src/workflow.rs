//! The `workflow-ref` rule: comments that cite planning documents absent from the repo.

use crate::lexer::Comment;
use crate::{Finding, Rule};

/// What must follow a pattern's text for it to match.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tail {
    /// Nothing more: the text alone.
    Exact,
    /// One or more ASCII digits, taken greedily.
    Digits,
    /// One or more ASCII letters or digits, taken greedily.
    Alphanumeric,
}

/// The banned phrases and id prefixes, lowercase. The README's rule table lists them.
const PATTERNS: [(&str, Tail); 21] = [
    ("note to coder", Tail::Exact),
    ("note to the coder", Tail::Exact),
    ("the coder", Tail::Exact),
    ("the coders", Tail::Exact),
    ("test-writer", Tail::Exact),
    ("test writer", Tail::Exact),
    ("test-writers", Tail::Exact),
    ("test writers", Tail::Exact),
    ("architect plan", Tail::Exact),
    ("the architect", Tail::Exact),
    ("per plan", Tail::Exact),
    ("per the plan", Tail::Exact),
    ("from the plan", Tail::Exact),
    ("feature plan", Tail::Exact),
    ("fix plan", Tail::Exact),
    ("plan §", Tail::Exact),
    ("plan section", Tail::Exact),
    ("od", Tail::Digits),
    ("oq", Tail::Digits),
    ("od-", Tail::Alphanumeric),
    ("oq-", Tail::Alphanumeric),
];

/// One finding per comment line holding a pattern, quoting its leftmost, longest match.
///
/// `text` is the normalized source the `comments` were lexed from, in source order. Doc
/// comments are scanned too; strings never are, since they are not comments.
pub(crate) fn workflow_findings(path: &str, text: &str, comments: &[Comment]) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    for comment in comments {
        let body = &text[comment.byte_span.0..comment.byte_span.1];
        for (offset, line) in body.split('\n').enumerate() {
            let line_no = comment.start_line + offset;
            if findings.last().is_some_and(|f| f.line == line_no) {
                continue;
            }
            if let Some(matched) = first_match(line) {
                findings.push(Finding {
                    path: path.to_string(),
                    line: line_no,
                    rule: Rule::WorkflowRef,
                    message: format!(
                        "workflow reference \"{matched}\": it cites a planning document that is \
                         not in the repo; state the reason in present tense or delete it"
                    ),
                });
            }
        }
    }
    findings
}

/// The original slice of the leftmost match in `line`, the longest one at that position.
fn first_match(line: &str) -> Option<&str> {
    let lower = line.to_ascii_lowercase();
    for (start, _) in lower.char_indices() {
        let longest = PATTERNS
            .iter()
            .filter_map(|&(pattern, tail)| match_at(&lower, start, pattern, tail))
            .max();
        if let Some(end) = longest {
            return Some(&line[start..end]);
        }
    }
    None
}

/// The end of `pattern` matched at `start` of the lowercased `lower`, if it matches there.
fn match_at(lower: &str, start: usize, pattern: &str, tail: Tail) -> Option<usize> {
    if !lower[start..].starts_with(pattern) {
        return None;
    }
    let mut end = start + pattern.len();
    let accepts: Option<fn(&u8) -> bool> = match tail {
        Tail::Exact => None,
        Tail::Digits => Some(u8::is_ascii_digit),
        Tail::Alphanumeric => Some(u8::is_ascii_alphanumeric),
    };
    if let Some(accepts) = accepts {
        let run = lower[end..].bytes().take_while(accepts).count();
        if run == 0 {
            return None;
        }
        end += run;
    }
    let matched = &lower[start..end];
    if matched.starts_with(is_word) && lower[..start].chars().next_back().is_some_and(is_word) {
        return None;
    }
    if matched.ends_with(is_word) && lower[end..].starts_with(is_word) {
        return None;
    }
    Some(end)
}

/// A char that continues a word, so it cannot sit at a match's boundary.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
