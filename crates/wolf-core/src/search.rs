//! The ripgrep-backed, streaming CONTENT search and its [`SearchSession`].
//!
//! A [`SearchSession`] is the sidebar search panel's view of "every line under
//! the search root that matches the query". It is created by
//! [`crate::api::WolfCore::start_search`], which spawns a pinned `rg --json`
//! subprocess and streams its JSON match events into a versioned snapshot behind
//! a lock — the exact shape of [`crate::files::FileSearch`]'s traversal, with the
//! `ignore`-crate walk swapped for an `rg` subprocess.
//!
//! ## Shape (mirrors [`crate::files`])
//!
//! The collected matches live in a [`SearchState`] behind a single
//! [`parking_lot::RwLock`]. The streaming task takes the *write* lock only to
//! append a finished batch (and bump [`SearchState::version`]), never holding it
//! across IO or the wake callback. Readers take an *owned* read guard via
//! [`SearchSession::read`] (owning an `Arc` clone of the lock), so the guard's
//! lifetime is independent of the [`SearchSession`].
//!
//! Matches accumulate in rg EMISSION order (per-file grouped, ascending line
//! within a file, but not cross-file sorted); the display order is imposed later,
//! at projection time, by [`crate::search_tree::project`]. Dropping (or
//! [`SearchSession::cancel`]-ing) a session kills the rg subprocess.
//!
//! ## Non-UTF-8 limitation
//!
//! rg emits a path/line as a `{"bytes": …}` object (base64) rather than
//! `{"text": …}` when it is not valid UTF-8. This module deserializes only the
//! `text` form and SKIPS any event whose required text is absent (a documented
//! limitation — no base64 dependency), so a non-UTF-8 file or line simply does
//! not appear in the results.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::{ArcRwLockReadGuard, Mutex, RawRwLock, RwLock};
use serde::Deserialize;
use serde_json::Value;

use crate::redraw::WakeFn;

/// Number of matches accumulated before a batch is appended to the shared state
/// (and a wake fired). Keeps lock hold-times short while still streaming results
/// to the UI promptly. Mirrors [`crate::files`]'s `TRAVERSAL_BATCH`.
const SEARCH_BATCH: usize = 128;

/// The knobs that shape a single content search, set by the search panel's text
/// field + three toggle buttons.
#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// The search text (a literal when `use_regex` is `false`, else a regex).
    pub query: String,
    /// `true` treats [`query`](Self::query) as a regular expression; `false`
    /// treats it literally (rg `-F`).
    pub use_regex: bool,
    /// `true` matches case-sensitively (rg `-s`); `false` matches
    /// case-insensitively (rg `-i`). Seeded from nvim's `ignorecase`, then owned
    /// by this search only.
    pub case_sensitive: bool,
    /// `true` searches every file; `false` (the default) restricts to Markdown
    /// (`--glob '*.md' --glob '*.markdown'`).
    pub all_files: bool,
}

/// One matching line found by the search: its file (absolute + `/`-separated
/// relative path), the 1-based `line_number`, the 0-based byte `column` of the
/// first submatch, the raw matched line (trailing newline stripped), and the byte
/// `[start, end)` ranges of every submatch within `line_text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    /// Absolute path to the file containing the match (used to issue `open_at`).
    pub abs_path: PathBuf,
    /// The file's `/`-separated path relative to the search root (the display /
    /// sort key). Falls back to the absolute path when not under the root.
    pub rel_path: String,
    /// 1-based line number of the match (rg's `line_number`).
    pub line_number: u64,
    /// 0-based byte column of the FIRST submatch (rg's first submatch `start`).
    pub column: u32,
    /// The raw matched line with any trailing `\n` / `\r\n` stripped.
    pub line_text: String,
    /// Byte `[start, end)` ranges of every submatch within
    /// [`line_text`](Self::line_text).
    pub match_ranges: Vec<(u32, u32)>,
}

/// The collected state of a [`SearchSession`]: the matches found so far, whether
/// rg has finished, an optional error string, and a monotonic `version` bumped on
/// every change. Mirrors [`crate::files::FileSearchState`] (minus `truncated` /
/// `seen`).
pub struct SearchState {
    /// All matches collected so far, in rg emission order (sorted for display at
    /// projection time by [`crate::search_tree::project`]).
    pub matches: Vec<SearchMatch>,
    /// `true` once the rg subprocess has finished (or will never run).
    pub complete: bool,
    /// A human-readable error (rg failed to spawn, exited abnormally, or
    /// `WOLF_RG_BIN` was unset) — set alongside `complete`.
    pub error: Option<String>,
    /// Monotonic version, bumped on each append so readers can detect changes.
    pub version: u64,
}

impl SearchState {
    /// A fresh, empty state (`version = 1`, `complete = false`).
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            matches: Vec::new(),
            complete: false,
            error: None,
            version: 1,
        }
    }

    /// An immediately-COMPLETE state carrying `error` (used when rg cannot run at
    /// all, e.g. `WOLF_RG_BIN` unset).
    #[must_use]
    pub(crate) fn errored(error: String) -> Self {
        Self {
            matches: Vec::new(),
            complete: true,
            error: Some(error),
            version: 1,
        }
    }
}

/// An owned read guard over a [`SearchState`].
///
/// Owns an `Arc` clone of the lock (so its lifetime is independent of the
/// [`SearchSession`]); `Deref`s to [`SearchState`] so callers read
/// `guard.matches`, `guard.complete`, `guard.error`, and `guard.version`
/// directly. Mirrors [`crate::files::FileSearchReadGuard`].
pub struct SearchReadGuard {
    guard: ArcRwLockReadGuard<RawRwLock, SearchState>,
}

impl std::ops::Deref for SearchReadGuard {
    type Target = SearchState;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// The three shared handles a [`SearchSession`] and its background streaming task
/// both hold: the result `state`, the `cancel` flag, and the `child` slot (the rg
/// process, so a cancel can kill it). Grouped so [`spawn_search`] and
/// [`SearchSession::from_parts`] take one value instead of three parallel args.
#[derive(Clone)]
pub(crate) struct SearchHandles {
    pub(crate) state: Arc<RwLock<SearchState>>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) child: Arc<Mutex<Option<std::process::Child>>>,
}

/// A live content-search session: the shared result state, a cancel flag, and a
/// slot holding the rg child (so a cancel can kill it).
///
/// Dropping a [`SearchSession`] cancels its streaming task AND kills the rg
/// subprocess (so closing the panel, or starting a new search, stops the old rg
/// promptly).
pub struct SearchSession {
    handles: SearchHandles,
}

impl SearchSession {
    /// Assembles a [`SearchSession`] from its shared [`SearchHandles`] (the
    /// streaming task having already been spawned against the same handles).
    pub(crate) fn from_parts(handles: SearchHandles) -> Self {
        Self { handles }
    }

    /// Reads the current search state (matches / complete / error / version).
    #[must_use]
    pub fn read(&self) -> SearchReadGuard {
        SearchReadGuard {
            guard: self.handles.state.read_arc(),
        }
    }

    /// Cancels the streaming task AND kills the rg subprocess. Idempotent; also
    /// fired on `Drop`.
    pub fn cancel(&self) {
        self.handles.cancel.store(true, Ordering::SeqCst);
        if let Some(child) = self.handles.child.lock().as_mut() {
            let _ = child.kill();
        }
    }
}

impl Drop for SearchSession {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Spawns the streaming rg search of `root` onto the tokio runtime `handle`.
///
/// The rg subprocess + line-oriented stdout read is blocking IO, so it runs on
/// `spawn_blocking`. rg's JSON match events are parsed and appended to `state` in
/// [`SEARCH_BATCH`]-sized chunks (and on every `end` event), bumping `version` and
/// calling `wake` after the write lock is dropped; `cancel` is checked each line
/// (breaking + killing the child). On EOF the remainder is appended, `complete`
/// is set (unless cancelled), any abnormal exit / spawn error is recorded in
/// `state.error`, and a final `wake` fires. Analog of
/// [`crate::files::spawn_traversal`].
pub(crate) fn spawn_search(
    root: PathBuf,
    opts: SearchOptions,
    rg_bin: PathBuf,
    handles: &SearchHandles,
    wake: WakeFn,
    handle: tokio::runtime::Handle,
) {
    let state = handles.state.clone();
    let cancel = handles.cancel.clone();
    let child_slot = handles.child.clone();
    handle.spawn_blocking(move || {
        tracing::debug!(root = %root.display(), query = %opts.query, "spawn_search running rg");

        let mut cmd = Command::new(&rg_bin);
        cmd.arg("--json").arg("--no-config").arg("--hidden");
        if !opts.use_regex {
            cmd.arg("-F");
        }
        if opts.case_sensitive {
            cmd.arg("-s");
        } else {
            cmd.arg("-i");
        }
        if !opts.all_files {
            cmd.arg("--glob")
                .arg("*.md")
                .arg("--glob")
                .arg("*.markdown");
        }
        cmd.arg("-e").arg(&opts.query);
        cmd.arg("--").arg(&root);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                finalize(
                    &state,
                    &mut Vec::new(),
                    true,
                    Some(format!("failed to spawn rg: {e}")),
                );
                wake();
                return;
            }
        };

        let stdout = child.stdout.take();
        // Drain rg's stderr to tracing (mirrors the nvim stderr drain).
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        tracing::debug!(target: "rg_stderr", "{line}");
                    }
                }
            });
        }
        // Publish the child so a concurrent cancel() can kill it.
        *child_slot.lock() = Some(child);

        let mut batch: Vec<SearchMatch> = Vec::new();
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines() {
                if cancel.load(Ordering::SeqCst) {
                    if let Some(child) = child_slot.lock().as_mut() {
                        let _ = child.kill();
                    }
                    break;
                }
                let Ok(line) = line else {
                    break;
                };
                match parse_rg_line(&line) {
                    Some(RgEvent::Match(m)) => {
                        if let Some(sm) = match_from_event(&root, &m) {
                            batch.push(sm);
                            if batch.len() >= SEARCH_BATCH {
                                append_batch(&state, &mut batch, false);
                                wake();
                            }
                        }
                    }
                    // Flush on each file's `end` so a file's matches surface as a
                    // group promptly, even below the batch threshold.
                    Some(RgEvent::End(_)) => {
                        if !batch.is_empty() {
                            append_batch(&state, &mut batch, false);
                            wake();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Reap the child and classify its exit (rg: 0 = matches, 1 = no matches,
        // >= 2 = a real error). A cancel leaves the result incomplete + error-free.
        let cancelled = cancel.load(Ordering::SeqCst);
        let mut error: Option<String> = None;
        if let Some(mut child) = child_slot.lock().take() {
            match child.wait() {
                Ok(status) if !cancelled => match status.code() {
                    Some(0) | Some(1) => {}
                    Some(code) => error = Some(format!("rg exited with code {code}")),
                    None => error = Some("rg terminated by signal".to_string()),
                },
                Ok(_) => {}
                Err(e) => error = Some(format!("failed to wait on rg: {e}")),
            }
        }

        let complete = !cancelled;
        finalize(&state, &mut batch, complete, error);
        wake();
    });
}

/// Appends `batch` (draining it) to the shared state and bumps `version`. The
/// write lock is held only for this short, IO-free critical section and never
/// across a wake. Mirrors [`crate::files`]'s `append_batch`.
fn append_batch(state: &Arc<RwLock<SearchState>>, batch: &mut Vec<SearchMatch>, complete: bool) {
    let mut guard = state.write();
    guard.matches.append(batch);
    guard.version += 1;
    if complete {
        guard.complete = true;
    }
}

/// The final append: drains any remaining `batch`, bumps `version`, marks the
/// search finished when `complete`, and records `error` if any.
fn finalize(
    state: &Arc<RwLock<SearchState>>,
    batch: &mut Vec<SearchMatch>,
    complete: bool,
    error: Option<String>,
) {
    let mut guard = state.write();
    guard.matches.append(batch);
    guard.version += 1;
    if complete {
        guard.complete = true;
    }
    if error.is_some() {
        guard.error = error;
    }
}

/// Builds the `/`-separated display path for `path`: relative to `root` when under
/// it, otherwise the full path string as a fallback. Mirrors
/// [`crate::files`]'s `display_for`.
fn display_for(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Strips a single trailing `\n` (and a preceding `\r`) from a matched line.
fn strip_newline(s: &str) -> &str {
    s.strip_suffix('\n')
        .map_or(s, |t| t.strip_suffix('\r').unwrap_or(t))
}

// --- rg --json event model -------------------------------------------------

/// A ripgrep `--json` event (a single newline-delimited JSON object).
///
/// Only [`Match`](RgEvent::Match) carries data we keep; [`Begin`](RgEvent::Begin) /
/// [`End`](RgEvent::End) drive per-file grouping/flush, and
/// [`Summary`](RgEvent::Summary) / [`Context`](RgEvent::Context) are decoded (so a
/// well-formed stream never errors) but ignored.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
// Begin/Summary/Context payloads are decoded (so a well-formed stream never
// errors) but only Match/End are consumed; the inline tests DO read Begin/Summary.
#[allow(dead_code)]
enum RgEvent {
    Begin(RgBegin),
    Match(RgMatch),
    End(RgEnd),
    Summary(Value),
    Context(Value),
}

/// The `begin` event payload (the file about to be searched).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RgBegin {
    path: RgText,
}

/// The `match` event payload: the file, the matched line, its 1-based line number,
/// and the per-submatch byte ranges.
#[derive(Debug, Deserialize)]
struct RgMatch {
    path: RgText,
    lines: RgText,
    line_number: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    absolute_offset: u64,
    submatches: Vec<RgSubmatch>,
}

/// The `end` event payload (per-file trailer). Fields are ignored; the event only
/// triggers a batch flush.
#[derive(Debug, Deserialize)]
struct RgEnd {}

/// One submatch within a matched line: the matched text and its byte `[start,
/// end)` offsets into the line's `text`.
#[derive(Debug, Deserialize)]
struct RgSubmatch {
    #[serde(rename = "match")]
    #[allow(dead_code)]
    text: RgText,
    start: usize,
    end: usize,
}

/// rg's "arbitrary data" wrapper. We deserialize ONLY the `text` form; a
/// non-UTF-8 `{"bytes": …}` payload leaves `text = None` and the event is skipped.
#[derive(Debug, Deserialize)]
struct RgText {
    text: Option<String>,
}

/// Parses one rg `--json` line into an [`RgEvent`], returning `None` on any JSON
/// error (a malformed / partial line is skipped rather than aborting the stream).
fn parse_rg_line(line: &str) -> Option<RgEvent> {
    serde_json::from_str(line).ok()
}

/// Builds a [`SearchMatch`] from a decoded `match` event, or `None` when a
/// required field is missing (non-UTF-8 path/line -> no `text`; missing
/// `line_number`).
fn match_from_event(root: &Path, m: &RgMatch) -> Option<SearchMatch> {
    let path_text = m.path.text.as_deref()?;
    let raw_line = m.lines.text.as_deref()?;
    let line_number = m.line_number?;

    let line_text = strip_newline(raw_line).to_string();
    let abs_path = PathBuf::from(path_text);
    let rel_path = display_for(root, &abs_path);
    let match_ranges: Vec<(u32, u32)> = m
        .submatches
        .iter()
        .map(|s| (s.start as u32, s.end as u32))
        .collect();
    let column = m.submatches.first().map_or(0, |s| s.start as u32);

    Some(SearchMatch {
        abs_path,
        rel_path,
        line_number,
        column,
        line_text,
        match_ranges,
    })
}

#[cfg(test)]
mod tests {
    //! Inline unit tests for the PRIVATE rg-JSON parse functions (`parse_rg_line`
    //! / `match_from_event`). The PUBLIC pure projection/display fns
    //! (`project` / `file_label` / `context_window`) are covered by the external
    //! `tests/search_projection.rs`; these cover only the private parse + skip
    //! cases that are inseparable from this module.

    use super::*;

    /// A canonical `begin` line decodes to the `Begin` variant with its path.
    #[test]
    fn parse_begin_event() {
        let line = r#"{"type":"begin","data":{"path":{"text":"foo.md"}}}"#;
        match parse_rg_line(line) {
            Some(RgEvent::Begin(b)) => assert_eq!(b.path.text.as_deref(), Some("foo.md")),
            other => panic!("expected Begin, got {other:?}"),
        }
    }

    /// A canonical `match` line decodes to the `Match` variant.
    #[test]
    fn parse_match_event() {
        let line = r#"{"type":"match","data":{"path":{"text":"foo.md"},"lines":{"text":"the needle is here\n"},"line_number":1,"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":4,"end":10}]}}"#;
        match parse_rg_line(line) {
            Some(RgEvent::Match(m)) => {
                assert_eq!(m.line_number, Some(1));
                assert_eq!(m.submatches.len(), 1);
            }
            other => panic!("expected Match, got {other:?}"),
        }
    }

    /// The `end` and `summary` events decode without error.
    #[test]
    fn parse_end_and_summary_events() {
        let end =
            r#"{"type":"end","data":{"path":{"text":"foo.md"},"binary_offset":null,"stats":{}}}"#;
        assert!(matches!(parse_rg_line(end), Some(RgEvent::End(_))));

        let summary = r#"{"type":"summary","data":{"elapsed_total":{"secs":0},"stats":{}}}"#;
        assert!(matches!(parse_rg_line(summary), Some(RgEvent::Summary(_))));
    }

    /// A malformed / non-JSON line yields `None`.
    #[test]
    fn parse_malformed_line_is_none() {
        assert!(parse_rg_line("not json at all").is_none());
        assert!(parse_rg_line("{").is_none());
        assert!(parse_rg_line("").is_none());
    }

    /// `match_from_event` collects one submatch into a single byte range, sets
    /// `column` to its start, and strips the trailing `\n`.
    #[test]
    fn match_from_event_single_submatch_lf() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"text":"/root/a.md"},"lines":{"text":"the needle is here\n"},"line_number":3,"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":4,"end":10}]}}"#,
        );
        let sm = match_from_event(Path::new("/root"), &m).expect("match should convert");
        assert_eq!(sm.rel_path, "a.md", "rel_path is relative to the root");
        assert_eq!(sm.abs_path, PathBuf::from("/root/a.md"));
        assert_eq!(sm.line_number, 3);
        assert_eq!(sm.line_text, "the needle is here", "trailing \\n stripped");
        assert_eq!(sm.column, 4, "column is the first submatch start");
        assert_eq!(sm.match_ranges, vec![(4, 10)]);
    }

    /// Multiple submatches produce multiple ranges; `column` is the FIRST start.
    #[test]
    fn match_from_event_multi_submatch() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"text":"/root/b.md"},"lines":{"text":"needle and needle\n"},"line_number":2,"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":0,"end":6},{"match":{"text":"needle"},"start":11,"end":17}]}}"#,
        );
        let sm = match_from_event(Path::new("/root"), &m).expect("match should convert");
        assert_eq!(sm.match_ranges, vec![(0, 6), (11, 17)]);
        assert_eq!(sm.column, 0, "column is the first submatch start");
    }

    /// A CRLF line strips both `\r` and `\n`; the submatch byte ranges (which
    /// index before the newline) stay correct.
    #[test]
    fn match_from_event_strips_crlf() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"text":"/root/c.md"},"lines":{"text":"ab needle\r\n"},"line_number":1,"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":3,"end":9}]}}"#,
        );
        let sm = match_from_event(Path::new("/root"), &m).expect("match should convert");
        assert_eq!(sm.line_text, "ab needle", "\\r\\n stripped");
        assert_eq!(sm.match_ranges, vec![(3, 9)]);
    }

    /// A path OUTSIDE the root falls back to the absolute path string as rel_path.
    #[test]
    fn match_from_event_absolute_fallback() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"text":"/other/d.md"},"lines":{"text":"needle\n"},"line_number":1,"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":0,"end":6}]}}"#,
        );
        let sm = match_from_event(Path::new("/root"), &m).expect("match should convert");
        assert_eq!(
            sm.rel_path, "/other/d.md",
            "non-root path keeps its absolute string"
        );
    }

    /// A non-UTF-8 path (a `{"bytes": …}` payload, so no `text`) is SKIPPED.
    #[test]
    fn match_from_event_non_utf8_path_is_skipped() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"bytes":"AAAA"},"lines":{"text":"needle\n"},"line_number":1,"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":0,"end":6}]}}"#,
        );
        assert!(
            match_from_event(Path::new("/root"), &m).is_none(),
            "a non-UTF-8 path has no text and is skipped"
        );
    }

    /// A non-UTF-8 LINE (no `text`) is also SKIPPED.
    #[test]
    fn match_from_event_non_utf8_line_is_skipped() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"text":"/root/e.md"},"lines":{"bytes":"AAAA"},"line_number":1,"absolute_offset":0,"submatches":[]}}"#,
        );
        assert!(
            match_from_event(Path::new("/root"), &m).is_none(),
            "a non-UTF-8 line has no text and is skipped"
        );
    }

    /// A `match` event missing `line_number` is SKIPPED.
    #[test]
    fn match_from_event_missing_line_number_is_skipped() {
        let m = decode_match(
            r#"{"type":"match","data":{"path":{"text":"/root/f.md"},"lines":{"text":"needle\n"},"absolute_offset":0,"submatches":[{"match":{"text":"needle"},"start":0,"end":6}]}}"#,
        );
        assert!(
            match_from_event(Path::new("/root"), &m).is_none(),
            "a match without line_number is skipped"
        );
    }

    /// Helper: decode a `match` line into its inner [`RgMatch`], panicking on any
    /// other variant.
    fn decode_match(line: &str) -> RgMatch {
        match parse_rg_line(line) {
            Some(RgEvent::Match(m)) => m,
            other => panic!("expected a Match event, got {other:?}"),
        }
    }
}
