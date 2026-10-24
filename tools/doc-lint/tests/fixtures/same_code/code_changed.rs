//! Crate docs that a comment-only edit may rewrite.

// Adds one to its argument.
// This explanation runs to three lines,
// which a comment-only migration shortens.
/// Returns `x + 1`.
fn add_one(x: i32) -> i32 {
    let url = "see // the docs"; // trailing note
    x + 2 /* inline note */
}

fn main() {
    println!("{}", add_one(1));
}
