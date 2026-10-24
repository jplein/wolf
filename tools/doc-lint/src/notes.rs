//! `notes/<P>.md` mirrors of source files: the `notes-orphan` and `notes-unlinked` rules.

use std::collections::HashSet;
use std::path::Path;

use crate::markdown::headings;
use crate::{language_for, lexer, read_lossy, refs, Finding, Rule};

/// Findings for the Markdown file `rel` if it is a mirror; empty for any other file.
pub(crate) fn mirror_findings(root: &Path, rel: &str, text: &str) -> Vec<Finding> {
    let Some(source) = rel
        .strip_prefix("notes/")
        .and_then(|s| s.strip_suffix(".md"))
    else {
        return Vec::new();
    };
    let Some(lang) = language_for(Path::new(source)) else {
        return Vec::new();
    };
    let finding = |line, rule, message| Finding {
        path: rel.to_string(),
        line,
        rule,
        message,
    };
    let Some(source_text) = read_lossy(&root.join(source)) else {
        return vec![finding(
            1,
            Rule::NotesOrphan,
            format!("{rel} mirrors {source}, which does not exist"),
        )];
    };
    let source_text = lexer::normalize(&source_text);
    let lexed = lexer::lex(&source_text, lang);
    let found = refs::refs_in(&source_text, &lexed.comments);
    let linked: HashSet<&str> = found
        .iter()
        .filter(|r| r.path == rel)
        .filter_map(|r| r.anchor.as_deref())
        .collect();
    headings(text)
        .into_iter()
        .filter(|h| h.level == 2 && !linked.contains(h.slug.as_str()))
        .map(|h| {
            finding(
                h.line,
                Rule::NotesUnlinked,
                format!(
                    "heading \"{}\" (#{}) is not linked from any comment in {source}",
                    h.text, h.slug
                ),
            )
        })
        .collect()
}
