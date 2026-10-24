//! The `bazel-coverage` rule: every Bazel package feeds the lint package's aggregates.

use std::path::Path;

use crate::{read_lossy, Finding, Rule};

/// The lint package whose `BUILD.bazel` must list every package's filegroups.
const LINT_BUILD: &str = "tools/lint/BUILD.bazel";

/// The `//<dir>:comment_srcs` and `//<dir>:markdown_srcs` labels absent from `lint_build_text`.
///
/// `""` is the root package (`//:comment_srcs`). A label counts only when quoted in full, so
/// `//crates/wolf-core:…` never covers `crates/wolf`.
pub fn missing_coverage(package_dirs: &[&str], lint_build_text: &str) -> Vec<String> {
    let mut missing = Vec::new();
    for dir in package_dirs {
        for name in ["comment_srcs", "markdown_srcs"] {
            let label = format!("//{dir}:{name}");
            let quoted = |q: char| format!("{q}{label}{q}");
            if !lint_build_text.contains(&quoted('"')) && !lint_build_text.contains(&quoted('\'')) {
                missing.push(label);
            }
        }
    }
    missing
}

/// Findings, one per missing label, against each package's `BUILD.bazel` in the walked `files`.
pub(crate) fn coverage_findings(root: &Path, files: &[String]) -> Vec<Finding> {
    let Some(lint_text) = read_lossy(&root.join(LINT_BUILD)) else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    for file in files {
        let dir = match file.strip_suffix("BUILD.bazel") {
            Some("") => "",
            Some(parent) => match parent.strip_suffix('/') {
                Some(dir) => dir,
                None => continue,
            },
            None => continue,
        };
        for label in missing_coverage(&[dir], &lint_text) {
            findings.push(Finding {
                path: file.clone(),
                line: 1,
                rule: Rule::BazelCoverage,
                message: format!("{LINT_BUILD} does not list \"{label}\""),
            });
        }
    }
    findings
}
