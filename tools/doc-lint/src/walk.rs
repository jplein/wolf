//! The repository walk behind `--root`. It never follows symlinks.

use std::fs;
use std::path::Path;

use crate::WalkConfig;

/// Every regular file under `root`, root-relative with `/` separators, sorted.
pub(crate) fn walk(root: &Path, cfg: &WalkConfig) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![String::new()];
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
            } else if kind.is_file() && !cfg.skips_prefix(&rel) {
                out.push(rel);
            }
        }
    }
    out.sort();
    out
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
