//! The lazily-started, loopback-only HTTP server behind
//! [`crate::api::WolfCore::open_in_browser`].
//!
//! One server per [`crate::api::WolfCore`] (per project). It binds
//! `127.0.0.1:0` — a random OS-assigned port, LOOPBACK ONLY, never `0.0.0.0` (a
//! hard security requirement) — and serves a tiny STATIC SITE rooted at the
//! project root (`getcwd()`, falling back to the pinned file's parent directory),
//! so relative links in the rendered Markdown resolve in URL space. The server is
//! hand-rolled HTTP/1.1 on the core's existing tokio runtime (only tokio's `net`
//! feature; NO HTTP-server crate), honoring the crate's one hard rule (no GUI
//! toolkit in the dependency tree).
//!
//! Routes (`root` = the server's canonical root; `relpath` = the pinned file's
//! root-relative path):
//!   * `GET /` — 302-redirects to `/<relpath>` (or, when the pinned buffer is
//!     unnamed so `relpath` is empty, renders the pinned buffer live in place).
//!   * `GET /<relpath>` (the PINNED file) — [`crate::nvim::render_source_html`] of
//!     the pinned buffer, with an injected auto-reload `<script>` (see
//!     [`reload_script`]) that polls `GET /tick` every second and calls
//!     `location.reload()` when the buffer's `changedtick` advances past the
//!     render-time baseline. This is the ONLY live page.
//!   * `GET /<other.md>` (a linked Markdown file under `root`) — a STATIC snapshot
//!     rendered from DISK via [`crate::markdown::render_html`], with NO reload
//!     script (`/tick` tracks only the pinned buffer).
//!   * `GET /<asset>` (any other existing file under `root`) — the RAW bytes with a
//!     content type guessed from the extension ([`content_type_for`]), so linked
//!     images/CSS/etc. load. The write path is `Vec<u8>` end-to-end so binary
//!     assets are never lossily transcoded.
//!   * `GET /tick` — the pinned buffer's current `changedtick`
//!     ([`crate::render::Snapshot::markdown_ticks`]) as plain text.
//!   * anything else, or any target that escapes `root` (the path-traversal guard,
//!     a hard security requirement) — `404 Not Found`.
//!
//! The pinned target is an [`Arc<Mutex<Target>>`]: each `open_in_browser` STORES a
//! fresh [`Target`] (via [`BrowserPreview::set_target`]) and returns the
//! root-relative URL, so the server re-points on each invocation but does not
//! live-follow between them.

use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::task::AbortHandle;

use crate::input::Nvim;
use crate::nvim::render_source_html;
use crate::render::SharedSnapshot;

/// Cap on the request head we read before parsing (a GET request line + headers
/// is tiny; anything larger is malformed or hostile, so we stop reading).
const MAX_HEAD_BYTES: usize = 8 * 1024;

/// The pinned target the loopback server serves: the source buffer to render
/// live, the canonical root the static site is served from, and the pinned file's
/// absolute path plus its root-relative URL path.
///
/// Rebuilt by [`build_target`] on each `open_in_browser` and swapped in via
/// [`BrowserPreview::set_target`].
#[derive(Clone)]
pub(crate) struct Target {
    /// The PINNED Markdown source buffer the server re-renders live at `/<relpath>`.
    pub bufnr: i64,
    /// The canonical root the static site is served from (the traversal guard's
    /// containment boundary).
    pub root: PathBuf,
    /// The pinned file's canonical absolute path (empty for an unnamed buffer).
    pub pinned_abs: PathBuf,
    /// The pinned file's `/`-joined path relative to `root` (empty when the buffer
    /// is unnamed or not under `root`).
    pub pinned_relpath: String,
}

/// A live loopback preview server: its bound port, the pinned target, and a handle
/// to abort the accept loop.
///
/// Owned by [`crate::api::WolfCore`] so it outlives any single `open_in_browser`
/// call; dropping the core's runtime cancels the accept task.
pub struct BrowserPreview {
    /// The OS-assigned loopback port the server is listening on.
    port: u16,
    /// The PINNED target the server serves. Behind a `Mutex` so `open_in_browser`
    /// can re-point it; the accept loop clones it out under a SHORT lock (never
    /// held across an `.await`).
    target: Arc<Mutex<Target>>,
    /// Aborts the accept loop (cheap idempotent cancellation). The runtime drop
    /// also cancels it; this is the explicit path used by `shutdown`.
    task: AbortHandle,
}

impl BrowserPreview {
    /// The loopback port the server is listening on (the `<port>` of
    /// `http://127.0.0.1:<port>/`).
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Re-points the server at `target` (a short-lived lock; no coordination with
    /// the accept loop needed — each connection clones the current target out).
    pub(crate) fn set_target(&self, target: Target) {
        *self.target.lock() = target;
    }

    /// Aborts the accept loop. Idempotent; also happens implicitly when the core's
    /// runtime is dropped.
    pub fn abort(&self) {
        self.task.abort();
    }
}

/// Starts the loopback preview server on `runtime`, pinned initially at `initial`.
///
/// Binds `127.0.0.1:0` (loopback only, random port), reads the assigned port back
/// from `local_addr()`, and spawns the accept loop on `runtime`. `nvim` is a clone
/// of the captive Neovim RPC handle (see [`crate::nvim::NvimProcess::handle`]) used
/// to re-render the pinned buffer; `snapshot` supplies the live `changedtick`. Bind
/// failure is surfaced as an [`io::Error`] (the caller logs it and returns no URL).
pub(crate) fn spawn(
    runtime: &Runtime,
    nvim: Nvim,
    snapshot: SharedSnapshot,
    initial: Target,
) -> io::Result<BrowserPreview> {
    // Loopback ONLY, random port. NEVER 0.0.0.0.
    let listener = runtime.block_on(async { TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await })?;
    let port = listener.local_addr()?.port();

    let target = Arc::new(Mutex::new(initial));
    let accept_target = target.clone();
    let handle = runtime.spawn(accept_loop(listener, nvim, snapshot, accept_target));
    let task = handle.abort_handle();

    Ok(BrowserPreview { port, target, task })
}

/// Accepts connections forever, spawning a per-connection [`handle_conn`] task.
/// A transient accept error is logged and the loop continues (a single failed
/// accept must not tear down the whole server).
async fn accept_loop(
    listener: TcpListener,
    nvim: Nvim,
    snapshot: SharedSnapshot,
    target: Arc<Mutex<Target>>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let nvim = nvim.clone();
                let snapshot = snapshot.clone();
                let target = target.clone();
                tokio::spawn(handle_conn(stream, nvim, snapshot, target));
            }
            Err(e) => {
                tracing::warn!("browser preview accept failed: {e}");
            }
        }
    }
}

/// Serves one HTTP/1.1 request: reads the head, routes on the parsed target, and
/// writes the response (best-effort; a broken pipe is ignored).
async fn handle_conn(
    mut stream: TcpStream,
    nvim: Nvim,
    snapshot: SharedSnapshot,
    target: Arc<Mutex<Target>>,
) {
    let Some(head) = read_head(&mut stream).await else {
        return;
    };

    let response = match parse_request_target(&head) {
        Some(raw_target) => route(raw_target, &nvim, &snapshot, &target).await,
        None => not_found_response(),
    };

    let _ = stream.write_all(&response).await;
    let _ = stream.flush().await;
}

/// Routes a parsed request `raw_target` to a framed `Vec<u8>` response.
///
/// The pinned [`Target`] is cloned out of the lock and the guard is DROPPED before
/// any `.await` — locking a `parking_lot::Mutex` across an await point can deadlock
/// a concurrent [`BrowserPreview::set_target`] and is flagged by
/// `clippy::await_holding_lock`.
async fn route(
    raw_target: &str,
    nvim: &Nvim,
    snapshot: &SharedSnapshot,
    target: &Arc<Mutex<Target>>,
) -> Vec<u8> {
    // `/tick` reads only the pinned bufnr; keep the lock SHORT and release it here.
    if raw_target == "/tick" {
        let bufnr = target.lock().bufnr;
        let tick = current_tick(snapshot, bufnr);
        return http_response("200 OK", "text/plain; charset=utf-8", &tick.to_string());
    }

    // Clone the target out and drop the guard BEFORE any `.await`.
    let Target {
        bufnr,
        root,
        pinned_abs,
        pinned_relpath,
    } = target.lock().clone();

    // `/` -> render the pinned buffer live when it is unnamed (empty relpath),
    // else redirect to its root-relative URL.
    if raw_target == "/" {
        if pinned_relpath.is_empty() {
            return render_pinned_live(nvim, snapshot, bufnr).await;
        }
        return redirect_response(&format!("/{}", encode_url_path(&pinned_relpath)));
    }

    // The pinned file itself is served live (matched on its stored relpath).
    let rel = percent_decode(raw_target.trim_start_matches('/'));
    if !pinned_relpath.is_empty() && rel == pinned_relpath {
        return render_pinned_live(nvim, snapshot, bufnr).await;
    }

    // Everything else resolves against the root (with the traversal guard).
    let Some(path) = resolve_path(&root, raw_target) else {
        return not_found_response();
    };
    // Defensive: the pinned file reached by any other URL form is still live.
    if !pinned_abs.as_os_str().is_empty() && path == pinned_abs {
        return render_pinned_live(nvim, snapshot, bufnr).await;
    }
    if is_markdown_path(&path) {
        match std::fs::read_to_string(&path) {
            Ok(src) => {
                let html =
                    crate::markdown::render_html(&src, crate::markdown::PreviewTheme::Default);
                http_response("200 OK", "text/html; charset=utf-8", &html)
            }
            Err(_) => not_found_response(),
        }
    } else {
        match std::fs::read(&path) {
            Ok(bytes) => http_response_bytes("200 OK", content_type_for(&path), &bytes),
            Err(_) => not_found_response(),
        }
    }
}

/// Renders the pinned source buffer `bufnr` live (with the injected auto-reload
/// script), falling back to [`fallback_page`] on a render error.
async fn render_pinned_live(nvim: &Nvim, snapshot: &SharedSnapshot, bufnr: i64) -> Vec<u8> {
    let tick = current_tick(snapshot, bufnr);
    let body = match render_source_html(nvim, bufnr).await {
        Ok(html) => render_page(&html, tick),
        Err(e) => {
            tracing::warn!("browser preview render_source_html({bufnr}) failed: {e}");
            fallback_page(tick)
        }
    };
    http_response("200 OK", "text/html; charset=utf-8", &body)
}

/// Reads the request head into a `String`, stopping at the `\r\n\r\n` header
/// terminator, at EOF, or once [`MAX_HEAD_BYTES`] is exceeded. Returns `None` if
/// nothing could be read.
async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() >= MAX_HEAD_BYTES {
            break;
        }
    }
    if buf.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Reads the pinned buffer's live `changedtick` from the snapshot
/// ([`crate::render::Snapshot::markdown_ticks`]), `0` until the buffer is first
/// dirtied. The read guard is dropped before the caller `.await`s a render.
fn current_tick(snapshot: &SharedSnapshot, bufnr: i64) -> u64 {
    snapshot
        .read()
        .markdown_ticks
        .get(&bufnr)
        .copied()
        .unwrap_or(0)
}

/// Extracts the request target from a raw HTTP request head, or `None` for a
/// non-`GET` or malformed request. Parses only the first line
/// (`GET <target> HTTP/1.1`); the target is the second whitespace-delimited field.
fn parse_request_target(request: &str) -> Option<&str> {
    let first_line = request.lines().next()?;
    let mut parts = first_line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    parts.next()
}

/// Injects the auto-reload [`reload_script`] immediately before the single
/// `</body>` of the rendered document (via `replacen(…, 1)`, so a stray later
/// `</body>` in content is untouched). If the document has no `</body>`, the page
/// is returned unchanged.
fn render_page(html: &str, tick: u64) -> String {
    let script = reload_script(tick);
    html.replacen("</body>", &format!("{script}</body>"), 1)
}

/// The injected live-reload `<script>`: polls `GET /tick` every 1000ms with
/// `fetch(..., {cache:'no-store'})`, `parseInt`s the reply, and calls
/// `location.reload()` when it is a number that differs from the render-time
/// `baseline`. A fetch error is swallowed so polling continues.
fn reload_script(tick: u64) -> String {
    format!(
        "<script>\n\
         (function() {{\n\
         var baseline = {tick};\n\
         setInterval(function() {{\n\
         fetch('/tick', {{ cache: 'no-store' }})\n\
         .then(function(r) {{ return r.text(); }})\n\
         .then(function(t) {{\n\
         var n = parseInt(t, 10);\n\
         if (!isNaN(n) && n !== baseline) {{ location.reload(); }}\n\
         }})\n\
         .catch(function() {{}});\n\
         }}, 1000);\n\
         }})();\n\
         </script>"
    )
}

/// A minimal document served when rendering the source buffer fails (e.g. the
/// buffer was unloaded, or the core is tearing down). Still carries the
/// [`reload_script`], so the tab recovers automatically once the buffer renders
/// again.
fn fallback_page(tick: u64) -> String {
    let script = reload_script(tick);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"></head>\
         <body class=\"wolf-markdown\">{script}</body></html>"
    )
}

/// Frames a string `body` as a complete HTTP/1.1 response. Delegates to
/// [`http_response_bytes`] over the body's UTF-8 bytes (the binary-safe write path).
fn http_response(status: &str, content_type: &str, body: &str) -> Vec<u8> {
    http_response_bytes(status, content_type, body.as_bytes())
}

/// Frames `body` bytes as a complete HTTP/1.1 response with `status` (e.g.
/// `"200 OK"`), the given `content_type`, a byte-accurate `Content-Length`, and
/// `Connection: close` (the client reads to EOF).
///
/// The head is ASCII; the raw `body` bytes are appended VERBATIM after the
/// `\r\n\r\n` terminator, so binary assets (images) are never lossily transcoded.
fn http_response_bytes(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n",
        len = body.len()
    );
    let mut response = Vec::with_capacity(head.len() + body.len());
    response.extend_from_slice(head.as_bytes());
    response.extend_from_slice(body);
    response
}

/// A `302 Found` redirect to `location` with an empty body (`Content-Length: 0`).
fn redirect_response(location: &str) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 302 Found\r\n\
         Location: {location}\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\
         \r\n"
    );
    head.into_bytes()
}

/// A `404 Not Found` response for an unknown path.
fn not_found_response() -> Vec<u8> {
    http_response("404 Not Found", "text/plain; charset=utf-8", "not found")
}

/// Builds the [`Target`] the server serves from the pinned `bufnr`, the captive
/// nvim's `cwd` (`getcwd()`, `None` when the query failed), and the pinned file's
/// raw absolute path `pinned_raw` (empty for an unnamed buffer).
///
/// `pinned_abs` is `pinned_raw` canonicalized (empty when `pinned_raw` is empty or
/// canonicalization fails). The `root` is `canonicalize(cwd)` when that succeeds
/// AND the pinned file is under it; otherwise it falls back to
/// `canonicalize(pinned_abs.parent())` (so a file outside the cwd — or a failed
/// cwd query — still gets a usable root). `pinned_relpath` is
/// [`relpath_between`]`(root, pinned_abs)`.
pub(crate) fn build_target(bufnr: i64, cwd: Option<PathBuf>, pinned_raw: &str) -> Target {
    let pinned_abs = if pinned_raw.is_empty() {
        PathBuf::new()
    } else {
        std::fs::canonicalize(pinned_raw).unwrap_or_else(|_| PathBuf::from(pinned_raw))
    };

    let cwd_root = cwd.and_then(|c| std::fs::canonicalize(c).ok());
    let root = match cwd_root {
        // Prefer the cwd root, but only when the pinned file lives under it.
        Some(r) if pinned_abs.as_os_str().is_empty() || pinned_abs.starts_with(&r) => r,
        _ => pinned_abs
            .parent()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .or_else(|| pinned_abs.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| pinned_abs.clone()),
    };

    let pinned_relpath = relpath_between(&root, &pinned_abs);

    Target {
        bufnr,
        root,
        pinned_abs,
        pinned_relpath,
    }
}

/// The `/`-joined path of `pinned_abs` relative to `root`, or the EMPTY string when
/// `pinned_abs` is empty or not under `root`. Pure (no filesystem access): both
/// paths are assumed already canonical.
fn relpath_between(root: &Path, pinned_abs: &Path) -> String {
    if pinned_abs.as_os_str().is_empty() {
        return String::new();
    }
    match pinned_abs.strip_prefix(root) {
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => String::new(),
    }
}

/// Percent-encodes each `/`-split segment of `rel`, PRESERVING the slashes as URL
/// path separators. Unreserved characters (`A-Za-z0-9-_.~`) pass through; every
/// other byte becomes `%XX` (uppercase hex), so `a b.md` -> `a%20b.md`.
pub(crate) fn encode_url_path(rel: &str) -> String {
    rel.split('/')
        .map(|seg| {
            let mut out = String::with_capacity(seg.len());
            for &b in seg.as_bytes() {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                    out.push(b as char);
                } else {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Decodes `%XX` percent-escapes in `s` (byte-wise; e.g. `%20` -> space,
/// `%2e%2e` -> `..`). A malformed or truncated escape is passed through literally.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resolves a URL `url_target` to a canonical filesystem path UNDER `root`, or
/// `None` when it does not exist or ESCAPES the root (the hard path-traversal
/// guard).
///
/// Strips any `?query`/`#fragment`, percent-decodes, trims a leading `/`, joins
/// onto `root`, canonicalizes (resolving `..`/symlinks; a non-existent path yields
/// `None`), and requires the result to `starts_with(root)` — so `root` MUST itself
/// be canonical (on macOS `/var` -> `/private/var`).
fn resolve_path(root: &Path, url_target: &str) -> Option<PathBuf> {
    let no_query = url_target.split(['?', '#']).next().unwrap_or("");
    let rel = percent_decode(no_query);
    let rel = rel.trim_start_matches('/');
    let joined = root.join(rel);
    let canonical = std::fs::canonicalize(joined).ok()?;
    canonical.starts_with(root).then_some(canonical)
}

/// Whether `path` has a recognized Markdown extension (case-insensitive):
/// `md`, `markdown`, `mdown`, `mkd`, `mkdn`, `mdwn`.
fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| {
            matches!(
                ext.as_str(),
                "md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdwn"
            )
        })
}

/// The `Content-Type` for `path` guessed from its extension (case-insensitive),
/// via a hand-written table (no `mime` crate). Unknown or extension-less paths map
/// to `application/octet-stream`.
fn content_type_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// A render-time baseline `changedtick`. Named so the magic-number lint stays
    /// quiet and every assertion can search for the SAME embedded value.
    const BASELINE_TICK: u64 = 42;

    /// A deliberately NON-UTF8 body (a stand-in for image/binary bytes) used to
    /// prove `http_response_bytes` never lossily transcodes the body.
    const NON_UTF8_BODY: [u8; 3] = [0xFF, 0x00, 0xFE];

    /// The HTTP head/body separator, matched byte-wise in the framed response.
    const HEADER_SEP: &[u8] = b"\r\n\r\n";

    /// A well-formed `GET / HTTP/1.1` request line yields the root target.
    #[test]
    fn parse_request_target_root() {
        let request = "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(
            parse_request_target(request),
            Some("/"),
            "a GET / request line must resolve to the \"/\" target"
        );
    }

    /// The `/tick` poll path is parsed out of its request line unchanged.
    #[test]
    fn parse_request_target_tick() {
        let request = "GET /tick HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(
            parse_request_target(request),
            Some("/tick"),
            "a GET /tick request line must resolve to the \"/tick\" target"
        );
    }

    /// An empty request (no first line) has no target.
    #[test]
    fn parse_request_target_empty_is_none() {
        assert_eq!(
            parse_request_target(""),
            None,
            "an empty request must not yield a target"
        );
    }

    /// A malformed first line whose first field is not a method is rejected.
    #[test]
    fn parse_request_target_malformed_is_none() {
        assert_eq!(
            parse_request_target("garbage\r\n\r\n"),
            None,
            "a first line that is not `GET <target> ...` must yield no target"
        );
    }

    /// The parser gates on the `GET` method: a non-GET request (here `POST`) is
    /// rejected even though it is otherwise well formed.
    #[test]
    fn parse_request_target_non_get_is_rejected() {
        let request = "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        assert_eq!(
            parse_request_target(request),
            None,
            "only GET is served; a POST request line must yield no target"
        );
    }

    /// `render_page` injects the reload poller immediately before the document's
    /// single `</body>`: the original body text survives, the poll endpoint and
    /// reload call are present, the baseline tick is embedded, and there is still
    /// EXACTLY ONE `</body>` (the `replacen(…, 1)` guarantee).
    #[test]
    fn render_page_injects_reload_before_single_body_close() {
        let page = render_page("<html><body>X</body></html>", BASELINE_TICK);

        assert!(
            page.contains('X'),
            "the original body text must survive injection: {page:?}"
        );
        assert!(
            page.contains("/tick"),
            "the injected poller must reference the /tick endpoint: {page:?}"
        );
        assert!(
            page.contains("location.reload"),
            "the injected poller must call location.reload(): {page:?}"
        );
        assert!(
            page.contains(&BASELINE_TICK.to_string()),
            "the render-time tick {BASELINE_TICK} must be embedded as the JS baseline: {page:?}"
        );
        assert_eq!(
            page.matches("</body>").count(),
            1,
            "injection must leave EXACTLY one </body> in the document: {page:?}"
        );

        let script_at = page
            .find("<script>")
            .expect("the injected reload <script> must be present");
        let body_close_at = page
            .find("</body>")
            .expect("the document must still have a </body>");
        assert!(
            script_at < body_close_at,
            "the injected <script> must sit BEFORE </body>: {page:?}"
        );
    }

    /// A document with no `</body>` is returned unchanged (the injection point is
    /// absent, so `replacen` is a no-op).
    #[test]
    fn render_page_without_body_close_is_unchanged() {
        let html = "<html><head></head></html>";
        assert_eq!(
            render_page(html, BASELINE_TICK),
            html,
            "a document without </body> must pass through unchanged"
        );
    }

    /// `reload_script` embeds the passed tick as the JS baseline and references the
    /// `/tick` poll endpoint and the `location.reload` refresh call.
    #[test]
    fn reload_script_embeds_tick_and_markers() {
        let script = reload_script(BASELINE_TICK);
        assert!(
            script.contains(&format!("baseline = {BASELINE_TICK}")),
            "the reload script must embed the tick {BASELINE_TICK} as its JS baseline: {script:?}"
        );
        assert!(
            script.contains("/tick"),
            "the reload script must poll the /tick endpoint: {script:?}"
        );
        assert!(
            script.contains("location.reload"),
            "the reload script must call location.reload(): {script:?}"
        );
    }

    /// `fallback_page` (served when a render fails) still carries the reload poller,
    /// so the tab recovers automatically once the buffer renders again.
    #[test]
    fn fallback_page_carries_reload_script() {
        let page = fallback_page(BASELINE_TICK);
        assert!(
            page.contains("/tick"),
            "the fallback page must still poll /tick so it self-recovers: {page:?}"
        );
        assert!(
            page.contains("location.reload"),
            "the fallback page must still call location.reload(): {page:?}"
        );
        assert!(
            page.contains(&BASELINE_TICK.to_string()),
            "the fallback page must embed the baseline tick {BASELINE_TICK}: {page:?}"
        );
    }

    /// `http_response` frames the body with the given status line and content type,
    /// a byte-accurate `Content-Length`, and echoes the body last. Now returns
    /// `Vec<u8>` (the binary-safe write path); a UTF-8 body still frames identically
    /// once viewed lossily.
    #[test]
    fn http_response_frames_status_headers_and_body() {
        let body = "hello";
        let response = http_response("200 OK", "text/plain; charset=utf-8", body);
        let s = String::from_utf8_lossy(&response);
        assert!(
            s.starts_with("HTTP/1.1 200 OK\r\n"),
            "the response must open with the HTTP/1.1 status line: {s:?}"
        );
        assert!(
            s.contains("Content-Type: text/plain; charset=utf-8\r\n"),
            "the response must carry the given content type: {s:?}"
        );
        assert!(
            s.contains(&format!("Content-Length: {}\r\n", body.len())),
            "the response must carry a byte-accurate Content-Length: {s:?}"
        );
        assert!(
            s.ends_with(body),
            "the response must end with the framed body: {s:?}"
        );
    }

    /// `not_found_response` (now `Vec<u8>`) opens with a `404` HTTP status line.
    #[test]
    fn not_found_response_is_404() {
        let response = not_found_response();
        assert!(
            response.starts_with(b"HTTP/1.1 404 Not Found\r\n"),
            "an unknown path must produce a 404 status line: {:?}",
            String::from_utf8_lossy(&response)
        );
    }

    /// Security guard: binding the preview listener the same way [`spawn`] does —
    /// `(Ipv4Addr::LOCALHOST, 0)` — yields a LOOPBACK address, never `0.0.0.0`.
    /// This is the hard requirement stated in the module doc.
    #[tokio::test]
    async fn preview_listener_binds_loopback_only() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("binding 127.0.0.1:0 must succeed");
        let addr = listener
            .local_addr()
            .expect("the bound listener must report a local_addr");
        assert!(
            addr.ip().is_loopback(),
            "the preview server must bind LOOPBACK only (127.0.0.1), never 0.0.0.0; got {addr}"
        );
    }

    /// A non-UTF8 body (an image would look like this) framed by
    /// `http_response_bytes` survives BYTE-EXACT after the `\r\n\r\n` header
    /// terminator and carries a byte-accurate `Content-Length`. This is the
    /// binary-safety regression guard: the body must NOT be routed through
    /// `from_utf8_lossy`, which would mangle `0xFF`/`0xFE`.
    #[test]
    fn http_response_bytes_preserves_non_utf8_body() {
        let response = http_response_bytes("200 OK", "application/octet-stream", &NON_UTF8_BODY);

        // The head is UTF-8 (ASCII); split it off at the first \r\n\r\n and check
        // the trailing bytes equal the body verbatim.
        let sep = response
            .windows(HEADER_SEP.len())
            .position(|w| w == HEADER_SEP)
            .expect("the framed response must contain a \\r\\n\\r\\n header terminator");
        let body = &response[sep + HEADER_SEP.len()..];
        assert_eq!(
            body, &NON_UTF8_BODY,
            "the framed body must equal the non-UTF8 input bytes VERBATIM: {body:?}"
        );

        let head = String::from_utf8_lossy(&response[..sep]);
        assert!(
            head.contains(&format!("Content-Length: {}\r\n", NON_UTF8_BODY.len())),
            "the response must carry a byte-accurate Content-Length: {head:?}"
        );
        assert!(
            head.contains("Content-Type: application/octet-stream\r\n"),
            "the response must carry the given content type: {head:?}"
        );
    }

    /// `redirect_response` frames a `302` with the `Location` header set to the
    /// passed target and an empty (`Content-Length: 0`) body.
    #[test]
    fn redirect_response_302_with_location() {
        let response = redirect_response("/build.md");
        let s = String::from_utf8_lossy(&response);
        assert!(
            s.starts_with("HTTP/1.1 302"),
            "a redirect must open with a 302 status line: {s:?}"
        );
        assert!(
            s.contains("Location: /build.md"),
            "the redirect must point Location at the passed target: {s:?}"
        );
        assert!(
            s.contains("Content-Length: 0\r\n"),
            "the redirect must carry an empty body (Content-Length: 0): {s:?}"
        );
    }

    /// `content_type_for` maps known extensions to their MIME type (matching the
    /// hand-written extension table), is CASE-INSENSITIVE, and falls back to
    /// `application/octet-stream` for unknown or extension-less paths.
    #[test]
    fn content_type_for_maps_known_extensions() {
        let cases = [
            ("pic.png", "image/png"),
            ("pic.PNG", "image/png"),
            ("photo.jpg", "image/jpeg"),
            ("photo.jpeg", "image/jpeg"),
            ("photo.JPEG", "image/jpeg"),
            ("icon.svg", "image/svg+xml"),
            ("style.css", "text/css; charset=utf-8"),
            ("app.js", "text/javascript; charset=utf-8"),
            ("data.unknownext", "application/octet-stream"),
            ("noextension", "application/octet-stream"),
        ];
        for (path, expected) in cases {
            assert_eq!(
                content_type_for(Path::new(path)),
                expected,
                "content_type_for({path:?}) must be {expected:?}"
            );
        }
    }

    /// `is_markdown_path` recognizes the Markdown extensions (case-insensitive) and
    /// rejects non-Markdown and extension-less paths.
    #[test]
    fn is_markdown_path_recognizes_markdown_extensions() {
        for ok in ["README.md", "notes.markdown", "a.mdown", "b.mkd", "C.MD"] {
            assert!(
                is_markdown_path(Path::new(ok)),
                "{ok:?} must be recognized as Markdown"
            );
        }
        for not in ["notes.txt", "pic.png", "plainfile"] {
            assert!(
                !is_markdown_path(Path::new(not)),
                "{not:?} must NOT be recognized as Markdown"
            );
        }
    }

    /// `relpath_between` (pure; no FS access) returns the `/`-joined path of `abs`
    /// relative to `root` for files under the root — direct child or nested subdir —
    /// and an EMPTY string when `abs` is not under `root` or is empty.
    #[test]
    fn relpath_between_computes_url_relative_path() {
        let root = PathBuf::from("/home/user/project");

        assert_eq!(
            relpath_between(&root, &root.join("build.md")),
            "build.md",
            "a direct child must map to its bare filename"
        );
        assert_eq!(
            relpath_between(&root, &root.join("sub").join("nested").join("f.md")),
            "sub/nested/f.md",
            "a nested file must map to a /-joined relative path"
        );
        assert_eq!(
            relpath_between(&root, Path::new("/etc/passwd")),
            "",
            "a path outside the root must map to an empty relpath"
        );
        assert_eq!(
            relpath_between(&root, Path::new("")),
            "",
            "an empty pinned path must map to an empty relpath"
        );
    }

    /// `encode_url_path` percent-encodes each `/`-split segment but PRESERVES the
    /// slashes, so a nested relpath stays a valid multi-segment URL path and a space
    /// becomes `%20`.
    #[test]
    fn encode_url_path_encodes_segments_preserving_slashes() {
        assert_eq!(
            encode_url_path("build.md"),
            "build.md",
            "an unreserved filename must pass through unchanged"
        );
        assert_eq!(
            encode_url_path("sub/f.md"),
            "sub/f.md",
            "the segment separators must be preserved"
        );
        assert_eq!(
            encode_url_path("a b.md"),
            "a%20b.md",
            "a space must be percent-encoded as %20"
        );
    }

    /// `percent_decode` decodes `%XX` escapes (including `%20` -> space and
    /// `%2e%2e` -> `..`) and passes an escape-free string through unchanged.
    #[test]
    fn percent_decode_decodes_escapes() {
        assert_eq!(percent_decode("a%20b"), "a b", "%20 must decode to a space");
        assert_eq!(
            percent_decode("%2e%2e"),
            "..",
            "%2e%2e must decode to .. (the traversal-guard bypass attempt)"
        );
        assert_eq!(
            percent_decode("plain.md"),
            "plain.md",
            "an escape-free string must pass through unchanged"
        );
    }

    /// `resolve_path` maps a URL target to a canonical filesystem path under the
    /// (canonical) root: it decodes percent-escapes, strips a query string, and
    /// REJECTS (`None`) any target that escapes the root or does not exist — the
    /// hard path-traversal security guard.
    #[test]
    fn resolve_path_canonicalizes_and_guards_traversal() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("failed to canonicalize temp root");

        let note = root.join("note.md");
        std::fs::write(&note, b"# note\n").expect("failed to write note.md");
        let spaced = root.join("my note.md");
        std::fs::write(&spaced, b"# spaced\n").expect("failed to write 'my note.md'");

        assert_eq!(
            resolve_path(&root, "/note.md").as_deref(),
            Some(note.as_path()),
            "an existing file must resolve to its canonical path"
        );
        assert_eq!(
            resolve_path(&root, "/my%20note.md").as_deref(),
            Some(spaced.as_path()),
            "a percent-encoded existing name must decode and resolve"
        );
        assert_eq!(
            resolve_path(&root, "/note.md?x=1").as_deref(),
            Some(note.as_path()),
            "a query string must be stripped before resolving"
        );
        assert_eq!(
            resolve_path(&root, "/../../etc/passwd"),
            None,
            "a `..` traversal escaping the root must be rejected"
        );
        assert_eq!(
            resolve_path(&root, "/%2e%2e/%2e%2e/etc/passwd"),
            None,
            "a percent-encoded `..` traversal escaping the root must be rejected"
        );
        assert_eq!(
            resolve_path(&root, "/does-not-exist.md"),
            None,
            "a non-existent target must resolve to None"
        );
    }
}
