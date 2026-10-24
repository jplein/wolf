# `Cargo.toml`

Last verified at commit `cc60afb1257ee0c5474105e363d0d452ba050afb`.

## Release profile

Only `--release` selects `[profile.release]`: `cargo build`, `cargo test`, `cargo clippy` and every step of the Linux gate use `dev` and `test`. `bench` inherits it, so `cargo test --release` pays its costs too.

`opt-level = 3` is stated rather than defaulted so the manifest carries the contract, and it reaches rusqlite's bundled SQLite because `cc` reads `OPT_LEVEL`. `lto = "thin"` buys cross-crate inlining along the wolf-gnome to wolf-core paint and snapshot path; unlike fat `lto` it is parallel and bounded, at a cost in link wall clock and peak memory.

`panic = "unwind"` is cargo's default, declared at the point of temptation. Cargo silently ignores `panic` for the `test` and `bench` profiles, so "abort" makes cargo build a second set of std artifacts, forking `target/release/` between `cargo build --release` and `cargo test --release` and rebuilding the whole graph on every alternation. "abort" would also turn an app panic into `SIGABRT`, destroying the exit-code evidence the cage smokes rest on.

`debug = "line-tables-only"` gives function names and `file:line` for perf and release panic backtraces. Nothing needs gdb variable inspection, so `debug = true` would cost size for nobody. `strip = "none"` is explicit because cargo's `strip` default depends on the cargo version, and the contract is that symbols survive.

Some keys stay unset on purpose. `codegen-units` keeps the release default of 16, because thin `lto` recovers most of what splitting gives up and 1 would serialize final codegen. `incremental`, `overflow-checks` and `debug-assertions` keep their release defaults, which are right, and stating them would invite retuning. `split-debuginfo` keeps the embedded default, which keeps the symbols of a copied binary; `[profile.release.package."*"]` and `[profile.release.build-override]` are unset because no measurement asks for either.
