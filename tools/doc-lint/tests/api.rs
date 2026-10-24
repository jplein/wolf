//! Public-API tests for doc-lint: file-name to language mapping, heading slugs, and the
//! `bazel-coverage` check behind `missing_coverage`.
//!
//! Mappings for names that cannot be fixture files (`BUILD.bazel`, `Cargo.toml`, `flake.nix`,
//! `.envrc`, `.gitignore`) live here, since a fixture with one of those names would be picked up
//! by Bazel, Cargo, Nix or git as a real file.

use std::path::Path;

use doc_lint::{heading_slug, language_for, missing_coverage, Language};

fn lang(name: &str) -> Option<Language> {
    language_for(Path::new(name))
}

fn assert_maps(names: &[&str], want: Option<Language>) {
    for name in names {
        assert_eq!(
            lang(name),
            want,
            "language_for({name:?}) must be {want:?}: a different language lexes it with the \
             wrong comment syntax, and None leaves it unlinted"
        );
    }
}

#[test]
fn bazel_file_names_map_to_starlark() {
    assert_maps(
        &[
            "BUILD.bazel",
            "BUILD",
            "MODULE.bazel",
            "WORKSPACE",
            "WORKSPACE.bazel",
            "defs.bzl",
            "crates/wolf-core/BUILD.bazel",
            "bazel/nvim/BUILD",
        ],
        Some(Language::Starlark),
    );
}

#[test]
fn shell_names_include_envrc_and_sh_templates() {
    assert_maps(
        &[
            ".envrc",
            "x.sh",
            "x.sh.tpl",
            "tools/lint/clippy_fix_launcher.sh.tpl",
            "sub/.envrc",
        ],
        Some(Language::Shell),
    );
}

#[test]
fn ignore_and_rc_dotfiles_map_to_hashline() {
    assert_maps(
        &[".gitignore", ".prettierignore", ".bazelignore", ".bazelrc"],
        Some(Language::HashLine),
    );
}

#[test]
fn extensions_map_to_their_languages() {
    assert_maps(&["x.rs", "build.rs"], Some(Language::Rust));
    assert_maps(&["x.swift"], Some(Language::Swift));
    assert_maps(&["x.c", "x.h"], Some(Language::C));
    assert_maps(&["x.css"], Some(Language::Css));
    assert_maps(&["x.lua", "init.lua"], Some(Language::Lua));
    assert_maps(&["x.nix", "flake.nix"], Some(Language::Nix));
    assert_maps(&["x.toml", "Cargo.toml"], Some(Language::Toml));
}

#[test]
fn unlinted_files_map_to_none() {
    assert_maps(
        &[
            "Cargo.lock",
            "fonts.conf",
            "x.json",
            "README.md",
            "notes.txt",
            "x.tpl",
            "BUILD.md",
            "Makefile",
            "x.rs.expected",
        ],
        None,
    );
}

fn assert_slugs(cases: &[(&str, &str)]) {
    for (heading, want) in cases {
        assert_eq!(
            heading_slug(heading),
            *want,
            "heading_slug({heading:?}) must follow GitHub: lowercase, drop every char that is not \
             alphanumeric, space, - or _, then spaces to -"
        );
    }
}

#[test]
fn heading_slug_lowercases_and_turns_spaces_into_hyphens() {
    assert_slugs(&[
        ("Good heading", "good-heading"),
        ("Step 1 run it", "step-1-run-it"),
        ("pre-existing hyphen", "pre-existing-hyphen"),
    ]);
}

#[test]
fn heading_slug_drops_punctuation_backticks_and_dots() {
    assert_slugs(&[
        ("The `foo()` call", "the-foo-call"),
        ("Read `Cargo.toml` first", "read-cargotoml-first"),
        ("v1.2 release", "v12-release"),
        ("Step 1: run it", "step-1-run-it"),
        ("A, B & C!", "a-b--c"),
    ]);
}

#[test]
fn heading_slug_keeps_underscores_and_unicode_letters() {
    assert_slugs(&[
        ("snake_case and kebab-case", "snake_case-and-kebab-case"),
        ("Café déjà vu", "café-déjà-vu"),
        ("Über Straße", "über-straße"),
    ]);
}

/// A `tools/lint/BUILD.bazel` stand-in that lists the root and `crates/a` fully, and
/// `crates/b` for comments only.
const LINT_BUILD: &str = r#"
filegroup(
    name = "all_comment_srcs",
    srcs = [
        "//:comment_srcs",
        "//crates/a:comment_srcs",
        "//crates/b:comment_srcs",
    ],
)

filegroup(
    name = "all_markdown_srcs",
    srcs = [
        "//:markdown_srcs",
        "//crates/a:markdown_srcs",
    ],
)
"#;

fn sorted(mut labels: Vec<String>) -> Vec<String> {
    labels.sort();
    labels
}

#[test]
fn missing_coverage_is_empty_when_every_package_is_listed() {
    let missing = missing_coverage(&["", "crates/a"], LINT_BUILD);
    assert!(
        missing.is_empty(),
        "the root and crates/a list both labels, so nothing is missing; got {missing:?}"
    );
}

#[test]
fn missing_coverage_reports_each_unlisted_label() {
    let missing = sorted(missing_coverage(
        &["", "crates/a", "crates/b", "crates/c"],
        LINT_BUILD,
    ));
    assert_eq!(
        missing,
        vec![
            "//crates/b:markdown_srcs".to_string(),
            "//crates/c:comment_srcs".to_string(),
            "//crates/c:markdown_srcs".to_string(),
        ],
        "crates/b lacks only its markdown label and crates/c lacks both; an empty result means \
         the check never reports, and a listed package in the result means it over-reports"
    );
}

#[test]
fn missing_coverage_uses_the_root_label_form() {
    let missing = sorted(missing_coverage(
        &[""],
        "\"//crates/a:comment_srcs\"\n\"//crates/a:markdown_srcs\"\n",
    ));
    assert_eq!(
        missing,
        vec![
            "//:comment_srcs".to_string(),
            "//:markdown_srcs".to_string()
        ],
        "the root package is written //:name; a package label elsewhere must not satisfy it"
    );
}

#[test]
fn missing_coverage_does_not_accept_a_longer_package_name() {
    let missing = sorted(missing_coverage(
        &["crates/wolf"],
        "\"//crates/wolf-core:comment_srcs\"\n\"//crates/wolf-core:markdown_srcs\"\n",
    ));
    assert_eq!(
        missing,
        vec![
            "//crates/wolf:comment_srcs".to_string(),
            "//crates/wolf:markdown_srcs".to_string()
        ],
        "a label for crates/wolf-core must not count as coverage for crates/wolf"
    );
}
