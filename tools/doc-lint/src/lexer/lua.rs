//! Lua: `--` comments (`---` included), `--[[ ]]` and `--[==[ ]==]` long comments, strings.

use super::{at, find_close, line_end, run_of, skip_quoted, Sink};

pub(super) fn lex(b: &[u8], sink: &mut Sink) {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'-' && at(b, i + 1) == b'-' {
            if let Some(level) = long_bracket(b, i + 2) {
                let body = i + 4 + level;
                let (end, closed) = long_close(b, body, level);
                let closer = if closed {
                    closer_len(b, body, end, level)
                } else {
                    0
                };
                sink.block(i, end, "--[[", false, (level + 4, closer));
                i = end;
            } else {
                let end = line_end(b, i);
                sink.line(i, end, "--", false);
                i = end;
            }
        } else if c == b'"' || c == b'\'' {
            let end = skip_quoted(b, i, c, true, true);
            sink.string(i, end);
            i = end;
        } else if let Some(level) = long_bracket(b, i) {
            let (end, _) = long_close(b, i + 2 + level, level);
            sink.string(i, end);
            i = end;
        } else {
            i += 1;
        }
    }
}

/// The level (count of `=`) of a long bracket `[=*[` opening at `i`.
fn long_bracket(b: &[u8], i: usize) -> Option<usize> {
    if at(b, i) != b'[' {
        return None;
    }
    let level = run_of(b, i + 1, b'=');
    (at(b, i + 1 + level) == b'[').then_some(level)
}

/// The length of the closer ending at `end`, counting a `--` written before `]]`, as in `--]]`.
fn closer_len(b: &[u8], body: usize, end: usize, level: usize) -> usize {
    let start = end - (level + 2);
    if start >= body + 2 && &b[start - 2..start] == b"--" {
        level + 4
    } else {
        level + 2
    }
}

/// Just past the `]=*]` of `level` closing a long bracket whose body starts at `i`.
fn long_close(b: &[u8], i: usize, level: usize) -> (usize, bool) {
    let mut closer = vec![b'='; level + 2];
    closer[0] = b']';
    closer[level + 1] = b']';
    find_close(b, i, &closer)
}
