// case: five // lines inside a plain string are not comments

const PLAIN: &str = "
// x
// x
// x
// x
// x
";

// case: five // lines inside a raw string holding a quote are not comments

const RAW: &str = r#"a quote " here
// x
// x
// x
// x
// x
"#;

// case: an escaped quote, a two-hash raw string and a br prefix keep // in the string

const ESC: &str = "escaped \" quote // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd";
const RAW2: &str = r##"has "# inside // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd"##;
const BRAW: &[u8] = br#"raw " bytes // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd"#;

// case: r#ident is an identifier, not a raw string opener

fn raw_ident() { let r#match = 1; }
// after raw ident one
// after raw ident two
// after raw ident three

// case: '"' is a char literal, so its trailing comment is still seen

const QUOTE: char = '"'; // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abc

// case: 'a is a lifetime, and a later string holding // is not a comment

fn life<'a>(x: &'a str) -> &'a str { x } // seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abc
const AFTER_LIFETIME: &str = "// seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd";

// case: nested block comments; the inner */ does not close the outer

/* outer /* inner */
   // still outer */
// real one
// real two
fn after_nested() {}
