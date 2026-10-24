//! Swift: `//` and nesting `/* */` comments; strings with `\( )`, multi-line and raw strings.

use super::{at, line_end, nested_block_end, run_of, starts, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && at(b, i + 1) == b'/' {
            let end = line_end(b, i);
            let doc = at(b, i + 2) == b'/' && at(b, i + 3) != b'/';
            sink.line(i, end, if doc { "///" } else { "//" }, doc);
            i = end;
        } else if c == b'/' && at(b, i + 1) == b'*' {
            let next = at(b, i + 3);
            let doc = at(b, i + 2) == b'*' && next != b'*' && next != b'/';
            let (end, closed) = nested_block_end(b, i);
            sink.block(i, end, "/*", doc, (2, if closed { 2 } else { 0 }));
            i = end;
        } else if c == b'"' {
            let end = string_end(b, i, 0);
            sink.string(i, end);
            i = end;
        } else if c == b'#' {
            let hashes = run_of(b, i, b'#');
            if at(b, i + hashes) == b'"' {
                let end = string_end(b, i + hashes, hashes);
                sink.string(i, end);
                i = end;
            } else {
                // A directive such as `#if` or `#selector`: code.
                i += hashes;
            }
        } else {
            i += 1;
        }
    }
}

/// The end of a string whose opening `"` is at `q`, raw with `hashes` `#`s when nonzero.
fn string_end(b: &[u8], q: usize, hashes: usize) -> usize {
    let multi = starts(b, q, b"\"\"\"");
    let mut j = if multi { q + 3 } else { q + 1 };
    while j < b.len() {
        let c = b[j];
        if c == b'\\' && run_of(b, j + 1, b'#') >= hashes {
            let k = j + 1 + hashes;
            j = if at(b, k) == b'(' {
                interpolation_end(b, k + 1)
            } else {
                k + 1
            };
            continue;
        }
        if multi {
            if starts(b, j, b"\"\"\"") && run_of(b, j + 3, b'#') >= hashes {
                return j + 3 + hashes;
            }
        } else if c == b'"' && run_of(b, j + 1, b'#') >= hashes {
            return j + 1 + hashes;
        } else if c == b'\n' {
            return j;
        }
        j += 1;
    }
    b.len()
}

/// Just past the `)` closing an interpolation whose code starts at `i`, skipping nested strings.
fn interpolation_end(b: &[u8], i: usize) -> usize {
    let mut depth = 1usize;
    let mut j = i;
    while j < b.len() {
        match b[j] {
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
            b'"' => j = string_end(b, j, 0),
            b'#' => {
                let hashes = run_of(b, j, b'#');
                j = if at(b, j + hashes) == b'"' {
                    string_end(b, j + hashes, hashes)
                } else {
                    j + hashes
                };
            }
            _ => j += 1,
        }
    }
    b.len()
}
