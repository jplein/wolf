//! Per-language lexing into comments and string spans, sharing one post-processing step.

mod c_like;
mod hashline;
mod lua;
mod nix;
mod rust;
mod shell;
mod starlark;
mod swift;
mod toml;

use crate::Language;

/// Whether a comment runs to the end of its line or between delimiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Line,
    Block,
}

/// One physical line of a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommentLine {
    pub line_no: usize,
    /// The text measured for width: from the marker (first line) or the first non-space char.
    pub from_marker: String,
    /// The text after the marker, delimiters and a leading `*` removed, trimmed.
    pub body: String,
    /// False for a block comment's delimiter-only first or last line.
    pub counts: bool,
}

/// One comment token. Line comments are one token per line; blocks are grouped by `rules`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comment {
    pub kind: Kind,
    /// Line comments join a block only with the same family, e.g. `//`, `///`, `//!`, `#`.
    pub family: &'static str,
    pub doc: bool,
    /// Code precedes the comment on its first line.
    pub trailing: bool,
    pub start_line: usize,
    pub end_line: usize,
    pub byte_span: (usize, usize),
    pub lines: Vec<CommentLine>,
}

/// A lexed source: comments in source order, and the sorted, disjoint byte spans of strings.
pub(crate) struct Lexed {
    pub comments: Vec<Comment>,
    pub strings: Vec<(usize, usize)>,
}

/// A comment as a language lexer reports it, before line splitting.
struct Raw {
    start: usize,
    end: usize,
    kind: Kind,
    family: &'static str,
    doc: bool,
    opener_len: usize,
    closer_len: usize,
}

/// Collects what a language lexer finds.
#[derive(Default)]
pub(super) struct Sink {
    raws: Vec<Raw>,
    strings: Vec<(usize, usize)>,
}

impl Sink {
    /// A line comment from its marker at `start` to `end`, the line's end.
    fn line(&mut self, start: usize, end: usize, family: &'static str, doc: bool) {
        self.raws.push(Raw {
            start,
            end,
            kind: Kind::Line,
            family,
            doc,
            opener_len: family.len(),
            closer_len: 0,
        });
    }

    /// A block comment; `closer_len` is 0 when the comment is unterminated.
    fn block(
        &mut self,
        start: usize,
        end: usize,
        family: &'static str,
        doc: bool,
        delims: (usize, usize),
    ) {
        self.raws.push(Raw {
            start,
            end,
            kind: Kind::Block,
            family,
            doc,
            opener_len: delims.0,
            closer_len: delims.1,
        });
    }

    fn string(&mut self, start: usize, end: usize) {
        if end > start {
            self.strings.push((start, end));
        }
    }
}

/// Joins CRLF line endings, so every later offset is into `\n`-only text.
pub(crate) fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Lexes normalized `text` as `lang`.
pub(crate) fn lex(text: &str, lang: Language) -> Lexed {
    let b = text.as_bytes();
    let mut sink = Sink::default();
    match lang {
        Language::Rust => rust::lex(b, &mut sink),
        Language::Swift => swift::lex(b, &mut sink),
        Language::C => c_like::lex(b, &mut sink, true),
        Language::Css => c_like::lex(b, &mut sink, false),
        Language::Lua => lua::lex(b, &mut sink),
        Language::Shell => shell::lex(b, &mut sink),
        Language::Starlark => starlark::lex(b, &mut sink),
        Language::Nix => nix::lex(b, &mut sink),
        Language::Toml => toml::lex(b, &mut sink),
        Language::HashLine => hashline::lex(b, &mut sink),
    }
    let starts = line_starts(text);
    let comments = sink
        .raws
        .iter()
        .map(|raw| build(text, &starts, raw))
        .collect();
    let mut strings = sink.strings;
    strings.sort_unstable();
    Lexed { comments, strings }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|(_, c)| *c == b'\n')
            .map(|(i, _)| i + 1),
    );
    starts
}

/// The 1-based line holding byte `offset`.
fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|&s| s <= offset)
}

fn build(text: &str, starts: &[usize], raw: &Raw) -> Comment {
    let start_line = line_of(starts, raw.start);
    let end_line = line_of(starts, raw.end.saturating_sub(1).max(raw.start));
    let first_start = starts[start_line - 1];
    let trailing = !text[first_start..raw.start].trim().is_empty();
    let mut lines = Vec::new();
    for line_no in start_line..=end_line {
        let line_start = starts[line_no - 1];
        let line_end = match starts.get(line_no) {
            Some(next) => next - 1,
            None => text.len(),
        };
        let seg = &text[line_start.max(raw.start)..line_end.min(raw.end)];
        lines.push(split_line(
            raw,
            seg,
            line_no,
            line_no == start_line,
            line_no == end_line,
        ));
    }
    Comment {
        kind: raw.kind,
        family: raw.family,
        doc: raw.doc,
        trailing,
        start_line,
        end_line,
        byte_span: (raw.start, raw.end),
        lines,
    }
}

fn split_line(raw: &Raw, seg: &str, line_no: usize, first: bool, last: bool) -> CommentLine {
    let from_marker = if first { seg } else { seg.trim_start() };
    let mut body = seg;
    if last && raw.kind == Kind::Block {
        body = &body[..body.len().saturating_sub(raw.closer_len)];
    }
    if first {
        body = body.get(raw.opener_len..).unwrap_or("");
        if raw.kind == Kind::Block {
            body = body.trim_start_matches(['*', '!']);
        }
    } else {
        body = body.trim_start();
        body = body.strip_prefix('*').unwrap_or(body);
    }
    let body = body.trim();
    let counts = raw.kind == Kind::Line || !(body.is_empty() && (first || last));
    CommentLine {
        line_no,
        from_marker: from_marker.trim_end().to_string(),
        body: body.to_string(),
        counts,
    }
}

// ---- helpers shared by the language lexers ----

/// The byte at `i`, or 0 past the end, so lookahead needs no bounds checks.
fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

fn starts(b: &[u8], i: usize, pat: &[u8]) -> bool {
    b.get(i..i + pat.len()) == Some(pat)
}

/// The offset of the `\n` ending the line that holds `i`, or the text's length.
fn line_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| i + p)
}

/// An identifier or number byte; non-ASCII bytes count, so Unicode identifiers stay whole.
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// How many `c` bytes run from `i`.
fn run_of(b: &[u8], i: usize, c: u8) -> usize {
    b[i.min(b.len())..].iter().take_while(|&&x| x == c).count()
}

/// The end of a string opened by the `quote` at `i`: just past the closing quote, or where it
/// gave up (end of text, or end of line when `one_line`).
fn skip_quoted(b: &[u8], i: usize, quote: u8, escapes: bool, one_line: bool) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        let c = b[j];
        if escapes && c == b'\\' {
            j += 2;
        } else if c == quote {
            return j + 1;
        } else if one_line && c == b'\n' {
            return j;
        } else {
            j += 1;
        }
    }
    b.len()
}

/// Just past the first `pat` at or after `i`, or the text's length; the flag says it was found.
fn find_close(b: &[u8], i: usize, pat: &[u8]) -> (usize, bool) {
    let mut j = i;
    while j + pat.len() <= b.len() {
        if &b[j..j + pat.len()] == pat {
            return (j + pat.len(), true);
        }
        j += 1;
    }
    (b.len(), false)
}

/// The end of a nesting `/* */` comment opened at `i`, and whether it was closed.
fn nested_block_end(b: &[u8], i: usize) -> (usize, bool) {
    let mut depth = 1usize;
    let mut j = i + 2;
    while j < b.len() {
        if starts(b, j, b"/*") {
            depth += 1;
            j += 2;
        } else if starts(b, j, b"*/") {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return (j, true);
            }
        } else {
            j += 1;
        }
    }
    (b.len(), false)
}
