//! Block assembly and the `comment-lines` and `comment-width` rules.

use crate::lexer::{Comment, Kind};
use crate::{Finding, Rule, MAX_COMMENT_LINES, MAX_COMMENT_WIDTH};

/// The limit findings for `comments`, which must be in source order.
pub(crate) fn limit_findings(path: &str, comments: &[Comment]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for block in blocks(comments) {
        if block[0].doc {
            continue;
        }
        let lines: usize = block
            .iter()
            .map(|c| c.lines.iter().filter(|l| l.counts).count())
            .sum();
        if lines > MAX_COMMENT_LINES {
            findings.push(Finding {
                path: path.to_string(),
                line: block[0].start_line,
                rule: Rule::CommentLines,
                message: format!(
                    "comment is {lines} lines (max {MAX_COMMENT_LINES}): cut it, shorten it, or move \
                     the rest to notes/{path}.md under a heading and link it"
                ),
            });
        }
        for line in block.iter().flat_map(|c| c.lines.iter()) {
            let width = line.from_marker.chars().count();
            if width > MAX_COMMENT_WIDTH && line.body.contains(char::is_whitespace) {
                findings.push(Finding {
                    path: path.to_string(),
                    line: line.line_no,
                    rule: Rule::CommentWidth,
                    message: format!(
                        "comment line is {width} chars (max {MAX_COMMENT_WIDTH}), measured from \
                         the comment marker"
                    ),
                });
            }
        }
    }
    findings
}

/// Groups full-line line comments on consecutive lines with one family; all else stands alone.
fn blocks(comments: &[Comment]) -> Vec<Vec<&Comment>> {
    let mut out: Vec<Vec<&Comment>> = Vec::new();
    for comment in comments {
        let joins = match out.last().and_then(|block| block.last()) {
            Some(prev) => joinable(prev) && joinable(comment) && continues(prev, comment),
            None => false,
        };
        match out.last_mut() {
            Some(block) if joins => block.push(comment),
            _ => out.push(vec![comment]),
        }
    }
    out
}

fn joinable(c: &Comment) -> bool {
    c.kind == Kind::Line && !c.trailing
}

fn continues(prev: &Comment, next: &Comment) -> bool {
    prev.family == next.family && prev.doc == next.doc && prev.end_line + 1 == next.start_line
}
