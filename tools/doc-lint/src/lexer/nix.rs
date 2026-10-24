//! Nix: `#` and `/* */` comments; `"…"` and `''…''` strings with `${ }` antiquotes.

use super::{at, find_close, line_end, starts, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'#' {
            let end = line_end(b, i);
            sink.line(i, end, "#", false);
            i = end;
        } else if c == b'/' && at(b, i + 1) == b'*' {
            let (end, closed) = find_close(b, i + 2, b"*/");
            sink.block(i, end, "/*", false, (2, if closed { 2 } else { 0 }));
            i = end;
        } else if c == b'"' {
            let end = dstring_end(b, i + 1);
            sink.string(i, end);
            i = end;
        } else if starts(b, i, b"''") {
            let end = istring_end(b, i + 2);
            sink.string(i, end);
            i = end;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            i = ident_end(b, i);
        } else {
            i += 1;
        }
    }
}

/// Identifiers may hold `'` and `-`, so `x''` never opens an indented string.
fn ident_end(b: &[u8], i: usize) -> usize {
    let mut j = i;
    while at(b, j).is_ascii_alphanumeric() || matches!(at(b, j), b'_' | b'\'' | b'-') {
        j += 1;
    }
    j
}

/// Just past the `"` closing a string whose body starts at `i`.
fn dstring_end(b: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
        } else if b[j] == b'"' {
            return j + 1;
        } else if starts(b, j, b"${") {
            j = antiquote_end(b, j + 2);
        } else {
            j += 1;
        }
    }
    b.len()
}

/// Just past the `''` closing an indented string whose body starts at `i`.
fn istring_end(b: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < b.len() {
        if starts(b, j, b"'''") || starts(b, j, b"''$") {
            j += 3;
        } else if starts(b, j, b"''\\") {
            j += 4;
        } else if starts(b, j, b"''") {
            return j + 2;
        } else if starts(b, j, b"${") {
            j = antiquote_end(b, j + 2);
        } else {
            j += 1;
        }
    }
    b.len()
}

/// Just past the `}` closing an antiquote whose code starts at `i`.
fn antiquote_end(b: &[u8], i: usize) -> usize {
    let mut depth = 1usize;
    let mut j = i;
    while j < b.len() {
        let c = b[j];
        if c == b'{' {
            depth += 1;
            j += 1;
        } else if c == b'}' {
            depth -= 1;
            j += 1;
            if depth == 0 {
                return j;
            }
        } else if c == b'"' {
            j = dstring_end(b, j + 1);
        } else if starts(b, j, b"''") {
            j = istring_end(b, j + 2);
        } else if c == b'#' {
            j = line_end(b, j);
        } else if starts(b, j, b"/*") {
            j = find_close(b, j + 2, b"*/").0;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            j = ident_end(b, j);
        } else {
            j += 1;
        }
    }
    b.len()
}
