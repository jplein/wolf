//! Links libghostty-vt's static archive under Cargo. Bazel never runs this file: it links
//! `//bazel/ghostty-vt:ghostty_vt` instead. See
//! `specs/terminal-panes.md#building-libghostty-vt`.

use std::env;
use std::path::PathBuf;

const DIR_VAR: &str = "WOLF_GHOSTTY_VT_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={DIR_VAR}");
    println!("cargo:rerun-if-changed=build.rs");

    let Some(dir) = env::var_os(DIR_VAR).filter(|dir| !dir.is_empty()) else {
        panic!(
            "{DIR_VAR} is unset. It names the libghostty-vt install prefix holding \
             lib/libghostty-vt.a, which the flake's dev shell exports: run cargo inside \
             `nix develop`."
        );
    };
    let lib = PathBuf::from(dir).join("lib");
    let archive = lib.join("libghostty-vt.a");
    if !archive.is_file() {
        panic!(
            "{DIR_VAR} names {}, but {} does not exist. The variable must name the \
             libghostty-vt install prefix the flake's dev shell exports: run cargo inside \
             `nix develop`.",
            lib.parent().unwrap_or(&lib).display(),
            archive.display()
        );
    }

    // `static=` picks the archive over the shared library installed beside it.
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
}
