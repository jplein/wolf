//! `strip` and `same-code`: code with every comment removed, to prove an edit touched only comments.

use crate::lexer::{lex, normalize};
use crate::Language;

/// `text` with every comment (doc comments too) removed and strings kept verbatim.
///
/// Outside strings, trailing whitespace is trimmed and blank lines are dropped, so deleting a
/// comment block, or the blank lines around it, does not change the result.
pub fn strip_comments(text: &str, lang: Language) -> String {
    let text = normalize(text);
    let lexed = lex(&text, lang);
    let mut keep = vec![true; text.len()];
    for comment in &lexed.comments {
        keep[comment.byte_span.0..comment.byte_span.1].fill(false);
    }
    let inside = |pos: usize| {
        let k = lexed.strings.partition_point(|&(start, _)| start < pos);
        k > 0 && pos < lexed.strings[k - 1].1
    };
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut start = 0;
    while start <= bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(bytes.len(), |p| start + p);
        let kept: Vec<u8> = (start..end)
            .filter(|&i| keep[i])
            .map(|i| bytes[i])
            .collect();
        let kept = String::from_utf8_lossy(&kept);
        let end_inside = end < bytes.len() && inside(end);
        let line = if end_inside {
            &kept[..]
        } else {
            kept.trim_end()
        };
        if !line.trim().is_empty() || inside(start) || end_inside {
            out.push_str(line);
            out.push('\n');
        }
        start = end + 1;
    }
    out
}

/// The first differing line of two strips, 1-based, with the old and new text; `None` if equal.
pub(crate) fn first_difference(old: &str, new: &str) -> Option<(usize, String, String)> {
    if old == new {
        return None;
    }
    let mut a = old.lines();
    let mut b = new.lines();
    let mut index = 1;
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if x == y => index += 1,
            (x, y) => {
                let show = |s: Option<&str>| s.map_or("<end of file>".to_string(), str::to_string);
                return Some((index, show(x), show(y)));
            }
        }
    }
}
