//! Rewritten crate docs.

// Adds one; the rest moved to a note.
/// Returns one more than `x`.
fn add_one(x: i32) -> i32 {
    let url = "see // the docs";
    x + 1
}

fn main() {
    println!("{}", add_one(1));
}
