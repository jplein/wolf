//! C and CSS: non-nesting `/* */` comments, `//` comments in C only, quoted strings.

use super::{at, find_close, line_end, skip_quoted, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink, line_comments: bool) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if line_comments && c == b'/' && at(b, i + 1) == b'/' {
            let end = line_end(b, i);
            sink.line(i, end, "//", false);
            i = end;
        } else if c == b'/' && at(b, i + 1) == b'*' {
            let (end, closed) = find_close(b, i + 2, b"*/");
            sink.block(i, end, "/*", false, (2, if closed { 2 } else { 0 }));
            i = end;
        } else if c == b'"' || c == b'\'' {
            let end = skip_quoted(b, i, c, true, true);
            sink.string(i, end);
            i = end;
        } else {
            i += 1;
        }
    }
}
