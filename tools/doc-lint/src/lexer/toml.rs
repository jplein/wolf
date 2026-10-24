//! TOML: `#` comments; basic and literal strings, single-line and multi-line.

use super::{line_end, run_of, skip_quoted, starts, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'#' {
            let end = line_end(b, i);
            sink.line(i, end, "#", false);
            i = end;
        } else if c == b'"' || c == b'\'' {
            let escapes = c == b'"';
            let end = if starts(b, i, &[c, c, c]) {
                multi_end(b, i + 3, c, escapes)
            } else {
                skip_quoted(b, i, c, escapes, true)
            };
            sink.string(i, end);
            i = end;
        } else {
            i += 1;
        }
    }
}

/// Just past a multi-line string's closing triple quote, which may absorb two more quotes.
fn multi_end(b: &[u8], i: usize, q: u8, escapes: bool) -> usize {
    let mut j = i;
    while j < b.len() {
        if escapes && b[j] == b'\\' {
            j += 2;
        } else if starts(b, j, &[q, q, q]) {
            return j + 3 + run_of(b, j + 3, q).min(2);
        } else {
            j += 1;
        }
    }
    b.len()
}
