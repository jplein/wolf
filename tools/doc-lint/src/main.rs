//! The `doc-lint` binary; see `doc_lint::run`.

use std::io::{stderr, stdout};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(doc_lint::run(&args, &mut stdout(), &mut stderr()));
}
