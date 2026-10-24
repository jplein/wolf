//! The command line: argument parsing, output and exit codes.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::strip::first_difference;
use crate::{check_files, check_root, language_for, strip_comments, Finding, WalkConfig};

const USAGE: &str = "usage:
  doc-lint --root <dir>            every rule over the tree under <dir>
  doc-lint --files <path>...       comment rules and doc references, relative to the cwd
  doc-lint strip <file>            the file with every comment removed
  doc-lint same-code <old> <new>   exit 0 when the two differ only in comments";

/// Runs the CLI on `args` (without the program name) and returns the exit code.
///
/// Findings go to `out`, one per line, sorted by path then line; a clean run prints nothing
/// there. Summaries and errors go to `err`. Exit 0 is clean, 1 is findings or a code
/// difference, 2 is a usage or I/O error.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match strs.as_slice() {
        ["--root", dir] => root_mode(Path::new(dir), out, err),
        ["--files", files @ ..] => files_mode(files, out, err),
        ["strip", file] => strip_mode(file, out, err),
        ["same-code", old, new] => same_code_mode(old, new, out, err),
        ["-h" | "--help"] => {
            let _ = writeln!(out, "{USAGE}");
            0
        }
        _ => usage_error(err),
    }
}

fn usage_error(err: &mut dyn Write) -> i32 {
    let _ = writeln!(err, "{USAGE}");
    2
}

fn root_mode(dir: &Path, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if !dir.is_dir() {
        let _ = writeln!(err, "doc-lint: {}: not a directory", dir.display());
        return 2;
    }
    report(&check_root(dir, &WalkConfig::repo_default()), out, err)
}

fn files_mode(files: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if let Some(missing) = files.iter().find(|f| !Path::new(f).is_file()) {
        let _ = writeln!(err, "doc-lint: {missing}: no such file");
        return 2;
    }
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    match check_files(Path::new("."), &paths, &WalkConfig::repo_default()) {
        Ok(findings) => report(&findings, out, err),
        Err(e) => {
            let _ = writeln!(err, "doc-lint: {e}");
            2
        }
    }
}

fn report(findings: &[Finding], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    for finding in findings {
        let _ = writeln!(out, "{finding}");
    }
    if findings.is_empty() {
        return 0;
    }
    let _ = writeln!(err, "doc-lint: {} finding(s)", findings.len());
    1
}

/// The stripped text of `file`, or the exit code of the error already reported.
fn stripped(file: &str, lang_from: &str, err: &mut dyn Write) -> Result<String, i32> {
    let Some(lang) = language_for(Path::new(lang_from)) else {
        let _ = writeln!(err, "doc-lint: {lang_from}: no known language");
        return Err(2);
    };
    match std::fs::read(file) {
        Ok(bytes) => Ok(strip_comments(&String::from_utf8_lossy(&bytes), lang)),
        Err(e) => {
            let _ = writeln!(err, "doc-lint: {file}: {e}");
            Err(2)
        }
    }
}

fn strip_mode(file: &str, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match stripped(file, file, err) {
        Ok(text) => {
            let _ = write!(out, "{text}");
            0
        }
        Err(code) => code,
    }
}

fn same_code_mode(old: &str, new: &str, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let (a, b) = match (stripped(old, new, err), stripped(new, new, err)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(code), _) | (_, Err(code)) => return code,
    };
    match first_difference(&a, &b) {
        None => 0,
        Some((line, was, is)) => {
            let _ = writeln!(out, "stripped code differs at stripped line {line}:");
            let _ = writeln!(out, "- {was}");
            let _ = writeln!(out, "+ {is}");
            1
        }
    }
}
