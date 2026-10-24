// See notes/src/a.rs.md#good-heading
fn good() {}

// See notes/src/a.rs.md#the-foo-call and notes/src/a.rs.md#dup
fn foo() {}

// See notes/src/a.rs.md#dup-1, and the whole file in notes/src/a.rs.md.
fn dup() {}

// Broken path: notes/src/missing.rs.md
fn missing() {}

// Broken anchor: notes/src/a.rs.md#nope
fn nope() {}

/// Doc comments are scanned for references too: specs/nope.md
fn doc() {}

fn string() -> &'static str {
    "notes/x.md"
}

// A placeholder is not a reference: notes/<crate>-decisions.md
fn placeholder() {}
