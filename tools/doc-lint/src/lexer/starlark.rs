//! Starlark: `#` comments; `'`, `"`, `'''` and `"""` strings (an `r` or `b` prefix changes nothing).

use super::{line_end, skip_quoted, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'#' {
            let end = line_end(b, i);
            sink.line(i, end, "#", false);
            i = end;
        } else if c == b'"' || c == b'\'' {
            let end = if b.get(i..i + 3) == Some(&[c, c, c][..]) {
                triple_end(b, i + 3, c)
            } else {
                skip_quoted(b, i, c, true, true)
            };
            sink.string(i, end);
            i = end;
        } else {
            i += 1;
        }
    }
}

/// Just past the triple quote of `q` closing a string whose body starts at `i`.
fn triple_end(b: &[u8], i: usize, q: u8) -> usize {
    let mut j = i;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
        } else if b.get(j..j + 3) == Some(&[q, q, q][..]) {
            return j + 3;
        } else {
            j += 1;
        }
    }
    b.len()
}
