//! Heading slugs, links and anchors in Markdown: the `md-link` and `md-anchor` rules.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::{read_lossy, Finding, Rule};

/// The GitHub anchor of a heading, given its text without the leading `#`s.
///
/// Lowercased; every char that is not alphanumeric, a space, `-` or `_` is dropped; spaces
/// become `-`. Duplicate suffixes (`-1`, `-2`) are added per file, not here.
pub fn heading_slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}

/// An ATX heading outside fenced code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Heading {
    pub level: usize,
    pub text: String,
    pub line: usize,
    /// The anchor, with the duplicate suffix applied.
    pub slug: String,
}

/// Every heading of `text`, in order.
pub(crate) fn headings(text: &str) -> Vec<Heading> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for (index, line) in prose_lines(text) {
        let Some((level, title)) = atx(line) else {
            continue;
        };
        let base = heading_slug(title);
        let count = seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        out.push(Heading {
            level,
            text: title.to_string(),
            line: index + 1,
            slug,
        });
    }
    out
}

/// The level and text of an ATX heading line.
fn atx(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let level = rest.bytes().take_while(|c| *c == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let after = &rest[level..];
    if !(after.is_empty() || after.starts_with([' ', '\t'])) {
        return None;
    }
    let title = after.trim();
    let closed = title.trim_end_matches('#');
    let title = if closed.is_empty() || closed.ends_with([' ', '\t']) {
        closed.trim_end()
    } else {
        title
    };
    Some((level, title))
}

/// The `(0-based index, line)` pairs outside fenced code blocks, fence lines excluded.
fn prose_lines(text: &str) -> Vec<(usize, &str)> {
    let mut fence: Option<(u8, usize)> = None;
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let marker = trimmed.bytes().next().filter(|c| *c == b'`' || *c == b'~');
        let run = marker.map_or(0, |m| trimmed.bytes().take_while(|c| *c == m).count());
        match (fence, marker) {
            (None, Some(m)) if run >= 3 => {
                fence = Some((m, run));
                continue;
            }
            (Some((open, len)), Some(m))
                if m == open && run >= len && trimmed[run..].trim().is_empty() =>
            {
                fence = None;
                continue;
            }
            _ => {}
        }
        if fence.is_none() {
            out.push((index, line));
        }
    }
    out
}

/// Heading anchors per Markdown file, read once.
#[derive(Default)]
pub(crate) struct SlugCache {
    files: HashMap<PathBuf, HashSet<String>>,
}

impl SlugCache {
    /// Whether the Markdown file at `path` has a heading anchored `slug`.
    pub(crate) fn has(&mut self, path: &Path, slug: &str) -> bool {
        self.files
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let text = read_lossy(path).unwrap_or_default();
                headings(&text).into_iter().map(|h| h.slug).collect()
            })
            .contains(slug)
    }
}

/// The `(1-based line, target)` of every link, image and reference definition outside code.
pub(crate) fn links(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (index, line) in prose_lines(text) {
        let line = blank_inline_code(line);
        if let Some(target) = reference_definition(&line) {
            out.push((index + 1, target));
        }
        let mut from = 0;
        while let Some(pos) = line[from..].find("](") {
            let start = from + pos + 2;
            if let Some(target) = destination(&line[start..]) {
                out.push((index + 1, target));
            }
            from = start;
        }
    }
    out
}

/// `line` with every backtick code span replaced by spaces.
fn blank_inline_code(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = line.as_bytes().to_vec();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'`' {
            i += 1;
            continue;
        }
        let run = b[i..].iter().take_while(|c| **c == b'`').count();
        let mut j = i + run;
        let mut close = None;
        while j < b.len() {
            let r = b[j..].iter().take_while(|c| **c == b'`').count();
            if r == run {
                close = Some(j);
                break;
            }
            j += r.max(1);
        }
        match close {
            Some(c) => {
                out[i..c + run].iter_mut().for_each(|x| *x = b' ');
                i = c + run;
            }
            None => i += run,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The target of a `[id]: target` line; footnotes (`[^1]:`) are not definitions.
fn reference_definition(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = line[indent..].strip_prefix('[')?;
    if indent > 3 || rest.starts_with('^') {
        return None;
    }
    let close = rest.find("]:")?;
    if close == 0 || rest[..close].contains(['[', ']']) {
        return None;
    }
    let target = rest[close + 2..].split_whitespace().next()?;
    let target = target
        .strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(target);
    (!target.is_empty()).then(|| target.to_string())
}

/// The destination at the start of an inline link's parenthesis, title excluded.
fn destination(s: &str) -> Option<String> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('<') {
        return rest
            .find('>')
            .map(|end| rest[..end].to_string())
            .filter(|t| !t.is_empty());
    }
    let mut depth = 0usize;
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            end = i;
            break;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            if depth == 0 {
                end = i;
                break;
            }
            depth -= 1;
        }
    }
    (end > 0).then(|| s[..end].to_string())
}

/// Whether `target` starts with a URI scheme such as `https:` or `mailto:`.
fn has_scheme(target: &str) -> bool {
    match target.find(':') {
        Some(colon) if colon > 0 => {
            let scheme = &target[..colon];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        }
        _ => false,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(byte) if b[i] == b'%' => {
                out.push(byte);
                i += 3;
            }
            _ => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `md-link` and `md-anchor` findings for the Markdown file `rel` under `root`.
pub(crate) fn link_findings(
    root: &Path,
    rel: &str,
    text: &str,
    cache: &mut SlugCache,
) -> Vec<Finding> {
    let own: HashSet<String> = headings(text).into_iter().map(|h| h.slug).collect();
    let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
    let mut findings = Vec::new();
    for (line, target) in links(text) {
        if has_scheme(&target) {
            continue;
        }
        let (path, fragment) = match target.split_once('#') {
            Some((p, f)) => (percent_decode(p), Some(f.to_string())),
            None => (percent_decode(&target), None),
        };
        let finding = |rule, message| Finding {
            path: rel.to_string(),
            line,
            rule,
            message,
        };
        if path.is_empty() {
            if let Some(f) = fragment.filter(|f| !f.is_empty() && !own.contains(f)) {
                findings.push(finding(
                    Rule::MdAnchor,
                    format!("{rel} has no heading with anchor #{f}"),
                ));
            }
            continue;
        }
        let resolved = match path.strip_prefix('/') {
            Some(abs) => root.join(abs),
            None => root.join(dir).join(&path),
        };
        if !resolved.exists() {
            findings.push(finding(Rule::MdLink, format!("{target} does not exist")));
            continue;
        }
        if let Some(f) = fragment.filter(|f| !f.is_empty()) {
            if path.ends_with(".md") && resolved.is_file() && !cache.has(&resolved, &f) {
                findings.push(finding(
                    Rule::MdAnchor,
                    format!("{path} has no heading with anchor #{f}"),
                ));
            }
        }
    }
    findings
}
