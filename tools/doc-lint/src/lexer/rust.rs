//! Rust: `//` and nesting `/* */` comments; plain, byte, C and raw strings; char literals.

use super::{at, is_word, line_end, nested_block_end, run_of, skip_quoted, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && at(b, i + 1) == b'/' {
            let end = line_end(b, i);
            let (family, doc) = match (at(b, i + 2), at(b, i + 3)) {
                (b'/', next) if next != b'/' => ("///", true),
                (b'!', _) => ("//!", true),
                _ => ("//", false),
            };
            sink.line(i, end, family, doc);
            i = end;
        } else if c == b'/' && at(b, i + 1) == b'*' {
            let doc = match (at(b, i + 2), at(b, i + 3)) {
                (b'*', next) => next != b'*' && next != b'/',
                (b'!', _) => true,
                _ => false,
            };
            let (end, closed) = nested_block_end(b, i);
            sink.block(i, end, "/*", doc, (2, if closed { 2 } else { 0 }));
            i = end;
        } else if c == b'"' {
            let end = skip_quoted(b, i, b'"', true, false);
            sink.string(i, end);
            i = end;
        } else if c == b'\'' {
            i = char_or_lifetime(b, i, sink);
        } else if is_word(c) {
            i = word(b, i, sink);
        } else {
            i += 1;
        }
    }
}

/// A word, or the string literal its prefix opens (`r"`, `r#"`, `b"`, `br#"`, `c"`, `b'`).
fn word(b: &[u8], i: usize, sink: &mut Sink) -> usize {
    let mut j = i;
    while j < b.len() && is_word(b[j]) {
        j += 1;
    }
    let w = &b[i..j];
    if matches!(w, b"r" | b"br" | b"cr") {
        let hashes = run_of(b, j, b'#');
        if at(b, j + hashes) == b'"' {
            let end = raw_string_end(b, j + hashes + 1, hashes);
            sink.string(i, end);
            return end;
        }
    }
    if matches!(w, b"b" | b"c") && at(b, j) == b'"' {
        let end = skip_quoted(b, j, b'"', true, false);
        sink.string(i, end);
        return end;
    }
    if w == b"b" && at(b, j) == b'\'' {
        let end = skip_quoted(b, j, b'\'', true, true);
        sink.string(i, end);
        return end;
    }
    j
}

/// Just past the `"` and `hashes` `#`s closing a raw string whose body starts at `i`.
fn raw_string_end(b: &[u8], i: usize, hashes: usize) -> usize {
    let mut j = i;
    while j < b.len() {
        if b[j] == b'"' && run_of(b, j + 1, b'#') >= hashes {
            return j + 1 + hashes;
        }
        j += 1;
    }
    b.len()
}

/// A char literal (`'x'`, `'\n'`, `'"'`) is a string; a lifetime or label (`'a`) is code.
fn char_or_lifetime(b: &[u8], i: usize, sink: &mut Sink) -> usize {
    let next = at(b, i + 1);
    let is_char = if next == b'\\' {
        true
    } else if next == 0 || next == b'\n' {
        false
    } else {
        at(b, i + 1 + utf8_len(next)) == b'\''
    };
    if !is_char {
        return i + 1;
    }
    let end = skip_quoted(b, i, b'\'', true, true);
    sink.string(i, end);
    end
}

/// The length of the UTF-8 sequence that `lead` starts.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}
