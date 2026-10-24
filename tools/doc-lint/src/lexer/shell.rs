//! Shell: `#` comments at a word start; quotes, `$'…'`, `${…}`, `$#`, `(( ))` and heredoc bodies.

use super::{at, find_close, line_end, skip_quoted, starts, Sink};

/// A heredoc whose body starts on the line after its operator.
struct Heredoc {
    word: Vec<u8>,
    strip_tabs: bool,
}

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = if starts(b, 0, b"#!") {
        line_end(b, 0)
    } else {
        0
    };
    let mut pending: Vec<Heredoc> = Vec::new();
    // Open parens inside an arithmetic `(( ))` or `$(( ))`, where `<<` is a shift.
    let mut arith = 0usize;
    while i < b.len() {
        match b[i] {
            b'(' if arith > 0 => {
                arith += 1;
                i += 1;
            }
            b')' if arith > 0 => {
                arith -= 1;
                i += 1;
            }
            b'(' if arith_open(b, i) => {
                arith = 2;
                i += 2;
            }
            b'\n' => {
                i += 1;
                if !pending.is_empty() {
                    let end = heredoc_bodies_end(b, i, &pending);
                    sink.string(i, end);
                    pending.clear();
                    i = end;
                }
            }
            b'\\' => i += 2,
            b'\'' => {
                let end = find_close(b, i + 1, b"'").0;
                sink.string(i, end);
                i = end;
            }
            b'"' => {
                let end = dquote_end(b, i + 1);
                sink.string(i, end);
                i = end;
            }
            b'$' => match at(b, i + 1) {
                b'\'' => {
                    let end = skip_quoted(b, i + 1, b'\'', true, false);
                    sink.string(i, end);
                    i = end;
                }
                b'#' => i += 2,
                b'{' => i = param_end(b, i + 2),
                _ => i += 1,
            },
            b'#' if word_start(b, i) => {
                let end = line_end(b, i);
                sink.line(i, end, "#", false);
                i = end;
            }
            b'<' if at(b, i + 1) == b'<' => {
                if arith > 0 {
                    i += 2;
                } else if at(b, i + 2) == b'<' {
                    i += 3;
                } else if let Some((doc, next)) = heredoc_op(b, i + 2) {
                    pending.push(doc);
                    i = next;
                } else {
                    i += 2;
                }
            }
            _ => i += 1,
        }
    }
}

/// `#` opens a comment only at the start of a word.
fn word_start(b: &[u8], i: usize) -> bool {
    i == 0
        || matches!(
            b[i - 1],
            b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>'
        )
}

/// Whether the `((` at `i` opens arithmetic: a command `(( … ))` or an expansion `$(( … ))`.
fn arith_open(b: &[u8], i: usize) -> bool {
    at(b, i + 1) == b'(' && (word_start(b, i) || b[i - 1] == b'$')
}

/// Just past the `"` closing a double-quoted string whose body starts at `i`.
fn dquote_end(b: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return j + 1,
            b'`' => j = find_close(b, j + 1, b"`").0,
            b'$' if at(b, j + 1) == b'(' => j = subst_end(b, j + 2),
            b'$' if at(b, j + 1) == b'{' => j = param_end(b, j + 2),
            _ => j += 1,
        }
    }
    b.len()
}

/// Just past the `)` closing a `$(` whose body starts at `i`.
fn subst_end(b: &[u8], i: usize) -> usize {
    let mut depth = 1usize;
    let mut j = i;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'\'' => j = find_close(b, j + 1, b"'").0,
            b'"' => j = dquote_end(b, j + 1),
            b'(' => {
                depth += 1;
                j += 1;
            }
            b')' => {
                depth -= 1;
                j += 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => j += 1,
        }
    }
    b.len()
}

/// Just past the `}` closing a `${` whose body starts at `i`; `#` inside is an operator.
fn param_end(b: &[u8], i: usize) -> usize {
    let mut depth = 1usize;
    let mut j = i;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => j = dquote_end(b, j + 1),
            b'{' => {
                depth += 1;
                j += 1;
            }
            b'}' => {
                depth -= 1;
                j += 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => j += 1,
        }
    }
    b.len()
}

/// The heredoc whose operator `<<` ends just before `i`, and the offset after its word.
///
/// The word must be quoted or start with a letter or `_`; `lex` never calls this inside `(( ))`.
fn heredoc_op(b: &[u8], i: usize) -> Option<(Heredoc, usize)> {
    let mut j = i;
    let strip_tabs = at(b, j) == b'-';
    if strip_tabs {
        j += 1;
    }
    while matches!(at(b, j), b' ' | b'\t') {
        j += 1;
    }
    let q = at(b, j);
    let (word, next) = if q == b'\'' || q == b'"' {
        let len = b[j + 1..].iter().position(|&c| c == q || c == b'\n')?;
        if b[j + 1 + len] != q {
            return None;
        }
        (b[j + 1..j + 1 + len].to_vec(), j + 2 + len)
    } else {
        if q == b'\\' {
            j += 1;
        }
        let first = at(b, j);
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return None;
        }
        let start = j;
        while at(b, j).is_ascii_alphanumeric() || matches!(at(b, j), b'_' | b'-') {
            j += 1;
        }
        (b[start..j].to_vec(), j)
    };
    if word.is_empty() {
        return None;
    }
    Some((Heredoc { word, strip_tabs }, next))
}

/// The offset after the terminator line of the last of `docs`, whose bodies start at `i`.
fn heredoc_bodies_end(b: &[u8], i: usize, docs: &[Heredoc]) -> usize {
    let mut j = i;
    for doc in docs {
        while j < b.len() {
            let end = line_end(b, j);
            let mut line = &b[j..end];
            if doc.strip_tabs {
                while let [b'\t', rest @ ..] = line {
                    line = rest;
                }
            }
            j = (end + 1).min(b.len());
            if line == doc.word.as_slice() {
                break;
            }
        }
    }
    j
}
