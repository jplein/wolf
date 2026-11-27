//! The repository walk behind `--root`. It never follows symlinks.

use std::fs;
use std::path::Path;

use crate::WalkConfig;

/// Every regular file under `root`, then every `.md` file under each of `cfg.markdown_prefixes`,
/// root-relative with `/` separators, sorted and deduplicated.
pub(crate) fn walk(root: &Path, cfg: &WalkConfig) -> Vec<String> {
    let mut out = Vec::new();
    walk_from(root, String::new(), cfg, false, &mut out);
    for prefix in &cfg.markdown_prefixes {
        let rel_dir = prefix.trim_end_matches('/');
        if is_real_dir(root, rel_dir) {
            walk_from(root, rel_dir.to_string(), cfg, true, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Whether every component of `rel_dir` under `root` is a directory, none of them a symlink.
fn is_real_dir(root: &Path, rel_dir: &str) -> bool {
    let mut path = root.to_path_buf();
    for component in rel_dir.split('/') {
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_dir() => {}
            _ => return false,
        }
    }
    true
}

fn walk_from(root: &Path, start: String, cfg: &WalkConfig, md_only: bool, out: &mut Vec<String>) {
    let mut stack = vec![start];
    while let Some(rel_dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(root.join(&rel_dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if rel_dir.is_empty() {
                name.clone()
            } else {
                format!("{rel_dir}/{name}")
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !skips_name(cfg, &name) && !cfg.skips_prefix(&format!("{rel}/")) {
                    stack.push(rel);
                }
            } else if kind.is_file()
                && !cfg.skips_prefix(&rel)
                && (!md_only || name.ends_with(".md"))
            {
                out.push(rel);
            }
        }
    }
}

fn skips_name(cfg: &WalkConfig, name: &str) -> bool {
    cfg.skip_dir_names.iter().any(|pattern| {
        if let Some(prefix) = pattern.strip_suffix('*') {
            name.starts_with(prefix)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else {
            name == pattern
        }
    })
}
