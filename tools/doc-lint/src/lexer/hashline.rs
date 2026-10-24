//! Ignore and rc files: a line whose first non-space char is `#` is a comment; nothing else is.

use super::{line_end, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let end = line_end(b, i);
        if let Some(offset) = b[i..end].iter().position(|c| *c != b' ' && *c != b'\t') {
            if b[i + offset] == b'#' {
                sink.line(i + offset, end, "#", false);
            }
        }
        i = end + 1;
    }
}
