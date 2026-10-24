// case: a 101-char trailing comment fails comment-width

const X: u8 = 1; // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abc

// case: a trailing comment never joins the full-line comments after it

const Y: u8 = 2; // trailing
// full one
// full two

// case: delimiter-only lines of a block comment do not count

/*
 a
 b
*/
fn delimited() {}
