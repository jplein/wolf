//! The lazily-started, loopback-only HTTP server behind
//! [`crate::api::WolfCore::open_in_browser`].
//!
//! One server per [`crate::api::WolfCore`] (per project). It binds
//! `127.0.0.1:0` — a random OS-assigned port, LOOPBACK ONLY, never `0.0.0.0` (a
//! hard security requirement) — and serves the RENDERED HTML of a PINNED Markdown
//! source buffer so the browser tab and the in-app preview match. The server is
//! hand-rolled HTTP/1.1 on the core's existing tokio runtime (only tokio's `net`
//! feature; NO HTTP-server crate), honoring the crate's one hard rule (no GUI
//! toolkit in the dependency tree).
//!
//! Routes:
//!   * `GET /` — [`crate::nvim::render_source_html`] of the pinned buffer, with an
//!     injected auto-reload `<script>` (see [`reload_script`]) that polls
//!     `GET /tick` every second and calls `location.reload()` when the buffer's
//!     `changedtick` advances past the render-time baseline.
//!   * `GET /tick` — the pinned buffer's current `changedtick`
//!     ([`crate::render::Snapshot::markdown_ticks`]) as plain text.
//!   * anything else — `404 Not Found`.
//!
//! The pinned target is an [`AtomicI64`]: each `open_in_browser` STORES the active
//! markdown bufnr (via [`BrowserPreview::set_target`]) and returns the stable URL,
//! so the server re-points on each invocation but does not live-follow between
//! them.

use std::io;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

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

/// A live loopback preview server: its bound port, the pinned source buffer, and
/// a handle to abort the accept loop.
///
/// Owned by [`crate::api::WolfCore`] so it outlives any single `open_in_browser`
/// call; dropping the core's runtime cancels the accept task.
pub struct BrowserPreview {
    /// The OS-assigned loopback port the server is listening on.
    port: u16,
    /// The PINNED Markdown source buffer the server re-renders. Stored atomically
    /// so `open_in_browser` can re-point it without touching the accept loop.
    target: Arc<AtomicI64>,
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

    /// Re-points the server at Markdown source buffer `bufnr` (an atomic store; no
    /// coordination with the accept loop needed).
    pub fn set_target(&self, bufnr: i64) {
        self.target.store(bufnr, Ordering::Relaxed);
    }

    /// Aborts the accept loop. Idempotent; also happens implicitly when the core's
    /// runtime is dropped.
    pub fn abort(&self) {
        self.task.abort();
    }
}

/// Starts the loopback preview server on `runtime`, pinned initially at
/// `initial_bufnr`.
///
/// Binds `127.0.0.1:0` (loopback only, random port), reads the assigned port back
/// from `local_addr()`, and spawns the accept loop on `runtime`. `nvim` is a clone
/// of the captive Neovim RPC handle (see [`crate::nvim::NvimProcess::handle`]) used
/// to re-render on each `GET /`; `snapshot` supplies the live `changedtick`. Bind
/// failure is surfaced as an [`io::Error`] (the caller logs it and returns no URL).
pub(crate) fn spawn(
    runtime: &Runtime,
    nvim: Nvim,
    snapshot: SharedSnapshot,
    initial_bufnr: i64,
) -> io::Result<BrowserPreview> {
    // Loopback ONLY, random port. NEVER 0.0.0.0.
    let listener = runtime.block_on(async { TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await })?;
    let port = listener.local_addr()?.port();

    let target = Arc::new(AtomicI64::new(initial_bufnr));
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
    target: Arc<AtomicI64>,
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
    target: Arc<AtomicI64>,
) {
    let Some(head) = read_head(&mut stream).await else {
        return;
    };

    let response = match parse_request_target(&head) {
        Some("/") => {
            let bufnr = target.load(Ordering::Relaxed);
            let tick = current_tick(&snapshot, bufnr);
            let body = match render_source_html(&nvim, bufnr).await {
                Ok(html) => render_page(&html, tick),
                Err(e) => {
                    tracing::warn!("browser preview render_source_html({bufnr}) failed: {e}");
                    fallback_page(tick)
                }
            };
            http_response("200 OK", "text/html; charset=utf-8", &body)
        }
        Some("/tick") => {
            let bufnr = target.load(Ordering::Relaxed);
            let tick = current_tick(&snapshot, bufnr);
            http_response("200 OK", "text/plain; charset=utf-8", &tick.to_string())
        }
        _ => not_found_response(),
    };

    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
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

/// Frames `body` as a complete HTTP/1.1 response with `status` (e.g. `"200 OK"`),
/// the given `content_type`, a byte-accurate `Content-Length`, and
/// `Connection: close` (the client reads to EOF).
fn http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        len = body.len()
    )
}

/// A `404 Not Found` response for an unknown path.
fn not_found_response() -> String {
    http_response("404 Not Found", "text/plain; charset=utf-8", "not found")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A render-time baseline `changedtick`. Named so the magic-number lint stays
    /// quiet and every assertion can search for the SAME embedded value.
    const BASELINE_TICK: u64 = 42;

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
    /// a byte-accurate `Content-Length`, and echoes the body last.
    #[test]
    fn http_response_frames_status_headers_and_body() {
        let body = "hello";
        let response = http_response("200 OK", "text/plain; charset=utf-8", body);
        assert!(
            response.starts_with("HTTP/1.1 200 OK\r\n"),
            "the response must open with the HTTP/1.1 status line: {response:?}"
        );
        assert!(
            response.contains("Content-Type: text/plain; charset=utf-8\r\n"),
            "the response must carry the given content type: {response:?}"
        );
        assert!(
            response.contains(&format!("Content-Length: {}\r\n", body.len())),
            "the response must carry a byte-accurate Content-Length: {response:?}"
        );
        assert!(
            response.ends_with(body),
            "the response must end with the framed body: {response:?}"
        );
    }

    /// `not_found_response` opens with a `404` HTTP status line.
    #[test]
    fn not_found_response_is_404() {
        let response = not_found_response();
        assert!(
            response.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "an unknown path must produce a 404 status line: {response:?}"
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
}
