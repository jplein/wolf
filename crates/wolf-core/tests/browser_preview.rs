//! Integration test for `wolf-core`'s BROWSER PREVIEW server: the lazily-started,
//! loopback-only HTTP server behind `WolfCore::open_in_browser`.
//!
//! This drives a REAL, hermetic Neovim (no mocks) via the shared `tests/common`
//! harness — exactly like `theme.rs` / `commands.rs` — AND a REAL loopback TCP
//! socket. The HTTP client is HAND-ROLLED on `std::net::TcpStream` (see
//! [`http_get`]): an HTTP-client crate (reqwest/hyper/ureq) is denylisted from
//! the Bazel test deps, and `std::net` adds zero dependency nodes.
//!
//! The server is a tiny static site rooted at the project root (falling back to
//! the pinned file's parent dir — the case these tests exercise, since the temp
//! `.md` is outside nvim's cwd):
//!   * binds `127.0.0.1:0` (a random OS-assigned port), loopback ONLY;
//!   * serves the PINNED file at its ROOT-relative path (the URL
//!     `open_in_browser` returns) = `markdown::render_html` of the source buffer,
//!     with an injected auto-reload script that polls `GET /tick` and calls
//!     `location.reload()` when the buffer's `changedtick` advances;
//!   * `GET /` 302-redirects to that relpath;
//!   * serves a linked sibling `.md` as a STATIC disk render (no reload script);
//!   * serves a linked asset (e.g. `.png`) as RAW bytes with a guessed content type;
//!   * serves `GET /tick` = the current `changedtick` as text;
//!   * REJECTS path traversal (`..` / `%2e%2e`) and 404s everything else.
//!
//! Because the `wolf_current_file` / `wolf_markdown_dirty` notifications are
//! out-of-band (they wake but do not bump redraw `seq`), the waits here POLL —
//! `is_markdown_active()` for the initial markdown gate, and `GET /tick` for the
//! post-edit tick bump — each bounded by [`FLUSH_TIMEOUT`] so a hang fails loudly.
//!
//! Assumptions:
//!   * `WolfCore::open_in_browser(&self) -> String` returns the loopback URL
//!     pointing at the pinned file's ROOT-relative path
//!     (`http://127.0.0.1:<port>/<relpath>`) or an EMPTY string when the active
//!     buffer is not markdown / on error.
//!   * The rendered page carries the `wolf-markdown` body wrapper (verified in
//!     `markdown::render_html`), the `/tick` poll endpoint, and a `location.reload`
//!     call — the three robust body markers asserted below.
//!   * The buffer is mutated the same way `save_open.rs` does: `i` + typed text +
//!     `<Esc>`, then a flush wait.

mod common;

use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{
    ch, config, key, type_str, wake_channel, FlushWaiter, DEFAULT_COLS, DEFAULT_ROWS, FLUSH_TIMEOUT,
};
use tempfile::TempDir;
use wolf_core::{InputEvent, Key, WolfCore};

/// Gap between polls while waiting for an out-of-band condition (markdown gate,
/// tick bump) to settle. Mirrors `theme.rs` / `commands.rs`.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The markdown document opened for the happy-path test.
const DOC: &str = "# Title\n\nhello world\n";

/// A unique, markdown-inert token typed into the buffer for the live-reload leg,
/// so the post-edit `GET /` body can be searched for it unambiguously.
const EDIT_TOKEN: &str = "freshinserttoken";

/// The exact loopback URL prefix `open_in_browser` must return; also used to
/// carve the port back out of the returned URL.
const URL_PREFIX: &str = "http://127.0.0.1:";

/// HTTP status for a served page / tick.
const HTTP_OK: u16 = 200;
/// HTTP status for a redirect (`GET /` -> the pinned file's relpath).
const HTTP_FOUND: u16 = 302;
/// HTTP status for an unknown path.
const HTTP_NOT_FOUND: u16 = 404;

/// The HTTP head/body separator, matched byte-wise when reading a raw response.
const HEADER_SEP: &[u8] = b"\r\n\r\n";

/// A sibling Markdown file linked from the pinned page: recognizable content so a
/// `GET /build.md` render can be asserted unambiguously.
const BUILD_DOC: &str = "# Building\n\nbuildmarkertoken\n";
/// The unique token embedded in [`BUILD_DOC`], searched for in the served body.
const BUILD_TOKEN: &str = "buildmarkertoken";

/// A small, fixed byte blob standing in for a linked binary asset (need NOT be a
/// valid PNG). Served verbatim so the raw-bytes path can be byte-checked.
const PNG_BYTES: [u8; 7] = [0x89, 0x50, 0x4E, 0x47, 0x01, 0x02, 0x03];

/// Minimal HTTP/1.1 GET against `127.0.0.1:<port><path>`, returning
/// `(status_code, body)`. Built ONLY on `std::net::TcpStream` + `std::io` (no HTTP
/// crate): writes a `GET … HTTP/1.1` request with `Connection: close`, reads the
/// whole response to EOF, splits on the FIRST `\r\n\r\n` into head/body, and parses
/// the status code from the first line (`HTTP/1.1 <code> <reason>`).
///
/// Every socket operation is bounded by [`FLUSH_TIMEOUT`] (connect/read/write
/// timeouts) so a misbehaving or absent server fails LOUDLY instead of hanging.
fn http_get(port: u16, path: &str) -> (u16, String) {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&addr, FLUSH_TIMEOUT)
        .unwrap_or_else(|e| panic!("failed to connect to 127.0.0.1:{port}{path}: {e}"));
    stream
        .set_read_timeout(Some(FLUSH_TIMEOUT))
        .expect("failed to set read timeout");
    stream
        .set_write_timeout(Some(FLUSH_TIMEOUT))
        .expect("failed to set write timeout");

    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .unwrap_or_else(|e| panic!("failed to write GET {path}: {e}"));
    stream
        .flush()
        .unwrap_or_else(|e| panic!("failed to flush GET {path}: {e}"));

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .unwrap_or_else(|e| panic!("failed to read response for GET {path}: {e}"));

    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or_else(|| {
        panic!("response for GET {path} has no header/body separator: {text:?}")
    });
    let status_line = head
        .lines()
        .next()
        .unwrap_or_else(|| panic!("empty response head for GET {path}"));
    // e.g. "HTTP/1.1 200 OK" -> the second whitespace-separated field is the code.
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("could not parse status from {status_line:?} for GET {path}"));

    (status, body.to_string())
}

/// Parses the port out of an `http://127.0.0.1:<port>/` URL, panicking loudly if
/// the URL is not the expected loopback shape.
fn port_from_url(url: &str) -> u16 {
    let rest = url
        .strip_prefix(URL_PREFIX)
        .unwrap_or_else(|| panic!("url {url:?} must start with {URL_PREFIX:?}"));
    let port_str = rest
        .split('/')
        .next()
        .unwrap_or_else(|| panic!("url {url:?} has no port segment"));
    port_str
        .parse::<u16>()
        .unwrap_or_else(|e| panic!("port {port_str:?} in url {url:?} is not a u16: {e}"))
}

/// Extracts the URL PATH (everything from the first `/` that follows
/// `127.0.0.1:<port>`) out of an `http://127.0.0.1:<port>/<relpath>` URL, so a
/// test can GET the exact path `open_in_browser` handed back rather than assuming
/// `/`. Panics loudly if the URL is not the expected loopback shape.
fn path_from_url(url: &str) -> String {
    let rest = url
        .strip_prefix(URL_PREFIX)
        .unwrap_or_else(|| panic!("url {url:?} must start with {URL_PREFIX:?}"));
    let slash = rest
        .find('/')
        .unwrap_or_else(|| panic!("url {url:?} has no path segment after the port"));
    rest[slash..].to_string()
}

/// Like [`http_get`], but returns the RAW response body as `Vec<u8>` (binary-safe,
/// for asset bytes that are not valid UTF-8) plus the parsed `Content-Type` header
/// value. Built ONLY on `std::net::TcpStream` + `std::io` (no HTTP crate): splits
/// the response at the FIRST `\r\n\r\n` into head/body on the BYTE buffer (never
/// lossily transcoding the body), parses the status code from the first line, and
/// reads `Content-Type` with a case-insensitive header-name match.
///
/// Every socket operation is bounded by [`FLUSH_TIMEOUT`] so an absent or wedged
/// server fails LOUDLY instead of hanging.
fn http_get_bytes(port: u16, path: &str) -> (u16, Vec<u8>, String) {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&addr, FLUSH_TIMEOUT)
        .unwrap_or_else(|e| panic!("failed to connect to 127.0.0.1:{port}{path}: {e}"));
    stream
        .set_read_timeout(Some(FLUSH_TIMEOUT))
        .expect("failed to set read timeout");
    stream
        .set_write_timeout(Some(FLUSH_TIMEOUT))
        .expect("failed to set write timeout");

    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .unwrap_or_else(|e| panic!("failed to write GET {path}: {e}"));
    stream
        .flush()
        .unwrap_or_else(|e| panic!("failed to flush GET {path}: {e}"));

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .unwrap_or_else(|e| panic!("failed to read response for GET {path}: {e}"));

    let sep = raw
        .windows(HEADER_SEP.len())
        .position(|w| w == HEADER_SEP)
        .unwrap_or_else(|| panic!("response for GET {path} has no header/body separator"));
    let body = raw[sep + HEADER_SEP.len()..].to_vec();
    let head = String::from_utf8_lossy(&raw[..sep]).into_owned();

    let status_line = head
        .lines()
        .next()
        .unwrap_or_else(|| panic!("empty response head for GET {path}"));
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("could not parse status from {status_line:?} for GET {path}"));

    let content_type = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-type")
                .then(|| value.trim().to_string())
        })
        .unwrap_or_default();

    (status, body, content_type)
}

/// Starts a `WolfCore` on a `test.md` inside a FRESH, DEDICATED temp dir (returned
/// so the caller can write sibling files that resolve under the server root), waits
/// for the initial attach flush, opens the file, and waits for its render flush.
///
/// A dedicated dir (rather than the shared OS temp dir used by
/// `common::start_core_with_temp_md`) is required here so sibling names like
/// `build.md` / `pic.png` cannot collide with other tests running in parallel and
/// so the server root (the pinned file's parent, per the parent-dir fallback) holds
/// exactly the files this test wrote.
fn start_core_in_dir(content: &str) -> (WolfCore, FlushWaiter, TempDir) {
    let dir = tempfile::tempdir().expect("failed to create dedicated temp dir");
    let path: PathBuf = dir.path().join("test.md");
    std::fs::write(&path, content).expect("failed to write test.md");

    let (wake, waiter) = wake_channel();
    let core = WolfCore::start(config(DEFAULT_COLS, DEFAULT_ROWS), wake)
        .expect("WolfCore::start failed (is WOLF_NVIM_BIN a valid nvim?)");
    waiter.wait_for_initial_flush(&core, FLUSH_TIMEOUT);

    core.send_input(InputEvent::Open { path });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    (core, waiter, dir)
}

/// Polls `core.is_markdown_active()` until it is `true`, draining wakes each
/// iteration, or panics after [`FLUSH_TIMEOUT`]. The markdown gate is fed by the
/// out-of-band `wolf_current_file` notification, so it may lag the open flush.
fn wait_for_markdown_active(core: &WolfCore, waiter: &FlushWaiter) {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        if core.is_markdown_active() {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for is_markdown_active() to \
                 become true after opening a .md buffer"
            );
        }
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Fetches `GET /tick` and parses the body as a `u64` changedtick, panicking with
/// context if the body does not trim-and-parse as an integer.
fn fetch_tick(port: u16) -> u64 {
    let (status, body) = http_get(port, "/tick");
    assert_eq!(status, HTTP_OK, "GET /tick must return 200; body: {body:?}");
    body.trim()
        .parse::<u64>()
        .unwrap_or_else(|e| panic!("GET /tick body {body:?} must parse as u64: {e}"))
}

/// Polls `GET /tick` until it DIFFERS from `baseline`, or panics after
/// [`FLUSH_TIMEOUT`]. Proves the live-reload signal (`markdown_ticks` for the
/// pinned source buffer) advanced after the edit.
fn wait_for_tick_change(port: u16, baseline: u64) -> u64 {
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    loop {
        let tick = fetch_tick(port);
        if tick != baseline {
            return tick;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out after {FLUSH_TIMEOUT:?} waiting for GET /tick to advance past \
                 the baseline {baseline} following a buffer edit"
            );
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// End-to-end: open a markdown buffer, start the loopback preview server via
/// `open_in_browser`, and exercise every route — `GET /` (rendered HTML + reload
/// markers), `GET /tick` (integer changedtick), the live-reload tick bump after an
/// edit, the freshly-typed text surfacing in a re-render, a 404 for an unknown
/// path, and server REUSE (a second `open_in_browser` returns the same port).
#[test]
fn open_in_browser_serves_rendered_markdown_and_live_tick() {
    // (1) Start core, open `# Title\n\nhello world\n`, wait for the markdown gate.
    let (core, waiter, _file) = common::start_core_with_temp_md(DOC);
    wait_for_markdown_active(&core, &waiter);

    // (2) open_in_browser returns a non-empty loopback URL; carve out the port.
    let url = core.open_in_browser();
    assert!(
        url.starts_with(URL_PREFIX),
        "open_in_browser() must return a http://127.0.0.1:<port>/<relpath> url; got {url:?}"
    );
    let port = port_from_url(&url);
    // The pinned file is now served at its ROOT-relative path (not `/`); derive it
    // from the returned URL rather than hardcoding.
    let path = path_from_url(&url);
    assert_ne!(
        path, "/",
        "the pinned file must be served at its relpath, not the bare root: {url:?}"
    );

    // (3) GET <relpath> -> 200 with the rendered body: `wolf-markdown` is render_html's body
    // wrapper, `hello` the source text, and `/tick` + `location.reload` the injected reload poller.
    let (status, body) = http_get(port, &path);
    assert_eq!(
        status, HTTP_OK,
        "GET {path} must return 200; body: {body:?}"
    );
    assert!(
        body.contains(r#"class="wolf-markdown""#),
        "GET {path} body must carry the wolf-markdown body wrapper: {body:?}"
    );
    assert!(
        body.contains("hello"),
        "GET {path} body must contain the rendered source text 'hello': {body:?}"
    );
    assert!(
        body.contains("/tick"),
        "GET {path} body must reference the /tick poll endpoint: {body:?}"
    );
    assert!(
        body.contains("location.reload"),
        "GET {path} body must inject a location.reload() auto-refresh: {body:?}"
    );

    // (3b) GET / now 302-redirects to the pinned relpath (do NOT follow it here).
    let (redirect_status, _redirect_body) = http_get(port, "/");
    assert_eq!(
        redirect_status, HTTP_FOUND,
        "GET / must 302-redirect to the pinned file's relpath"
    );

    // (4) GET /tick -> 200, body trims-and-parses as a u64.
    let tick_before = fetch_tick(port);

    // (5) Edit the buffer so TextChanged fires and markdown_ticks advances. Same
    // mutation mechanism as save_open.rs: `i` + typed text + <Esc>, then a flush.
    core.send_input(ch('i'));
    type_str(&core, EDIT_TOKEN);
    core.send_input(key(Key::Esc));
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // Poll /tick until it differs from the pre-edit value (bounded, fails loudly).
    let tick_after = wait_for_tick_change(port, tick_before);
    assert_ne!(
        tick_after, tick_before,
        "the changedtick must advance after editing the source buffer"
    );

    // (6) After the edit, a fresh GET <relpath> re-render contains the newly typed
    // text (the pinned file is served live at its relpath).
    let (status, body) = http_get(port, &path);
    assert_eq!(
        status, HTTP_OK,
        "GET {path} after the edit must still return 200; body: {body:?}"
    );
    assert!(
        body.contains(EDIT_TOKEN),
        "GET {path} after the edit must contain the newly typed text {EDIT_TOKEN:?}: {body:?}"
    );

    // (7) GET /nope -> 404.
    let (status, body) = http_get(port, "/nope");
    assert_eq!(
        status, HTTP_NOT_FOUND,
        "GET /nope must return 404; body: {body:?}"
    );

    // (8) A second open_in_browser reuses the SAME server (same port).
    let url_again = core.open_in_browser();
    assert!(
        url_again.starts_with(URL_PREFIX),
        "the second open_in_browser() must also return a loopback url; got {url_again:?}"
    );
    assert_eq!(
        port_from_url(&url_again),
        port,
        "the second open_in_browser() must REUSE the lazily-started server (same port)"
    );

    // (9) Clean teardown.
    core.shutdown().expect("shutdown should be Ok");
}

/// A sibling markdown file the pinned page links to renders as a STATIC snapshot
/// from disk: `GET /build.md` returns 200 with the render_html body wrapper and the
/// file's text, and does NOT carry the live-reload script (only the pinned file is
/// live; linked pages are static).
#[test]
fn linked_markdown_sibling_renders_static() {
    let (core, waiter, dir) = start_core_in_dir(DOC);
    wait_for_markdown_active(&core, &waiter);
    std::fs::write(dir.path().join("build.md"), BUILD_DOC).expect("failed to write build.md");

    let url = core.open_in_browser();
    assert!(
        url.starts_with(URL_PREFIX),
        "open_in_browser() must return a loopback url; got {url:?}"
    );
    let port = port_from_url(&url);

    let (status, body) = http_get(port, "/build.md");
    assert_eq!(
        status, HTTP_OK,
        "GET /build.md must render the sibling file; body: {body:?}"
    );
    assert!(
        body.contains(r#"class="wolf-markdown""#),
        "GET /build.md body must carry the wolf-markdown wrapper: {body:?}"
    );
    assert!(
        body.contains(BUILD_TOKEN),
        "GET /build.md body must contain the sibling's text {BUILD_TOKEN:?}: {body:?}"
    );
    assert!(
        !body.contains("location.reload"),
        "a linked (static) page must NOT carry the live-reload script: {body:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Security: the path-traversal guard REJECTS any target that escapes the server
/// root, both as literal `..` and percent-encoded `%2e%2e` — each yields a 404 and
/// NEVER leaks the contents of a file outside the root (e.g. `/etc/passwd`).
#[test]
fn path_traversal_is_rejected() {
    let (core, waiter, _dir) = start_core_in_dir(DOC);
    wait_for_markdown_active(&core, &waiter);

    let url = core.open_in_browser();
    let port = port_from_url(&url);

    for target in ["/../../etc/passwd", "/%2e%2e/%2e%2e/etc/passwd"] {
        let (status, body) = http_get(port, target);
        assert_eq!(
            status, HTTP_NOT_FOUND,
            "GET {target} must be rejected as 404 (path-traversal guard); body: {body:?}"
        );
        assert!(
            !body.contains("root:"),
            "GET {target} must NOT leak file contents outside the root: {body:?}"
        );
    }

    core.shutdown().expect("shutdown should be Ok");
}

/// A linked binary ASSET is served RAW: `GET /pic.png` returns 200 with an
/// image content type and the exact bytes written to disk (binary-safe path — the
/// body is never lossily transcoded).
#[test]
fn linked_asset_serves_raw_bytes() {
    let (core, waiter, dir) = start_core_in_dir(DOC);
    wait_for_markdown_active(&core, &waiter);
    std::fs::write(dir.path().join("pic.png"), PNG_BYTES).expect("failed to write pic.png");

    let url = core.open_in_browser();
    let port = port_from_url(&url);

    let (status, body, content_type) = http_get_bytes(port, "/pic.png");
    assert_eq!(
        status, HTTP_OK,
        "GET /pic.png must serve the asset; content_type: {content_type:?}"
    );
    assert!(
        content_type.contains("image/png"),
        "GET /pic.png must carry an image/png content type; got {content_type:?}"
    );
    assert_eq!(
        body, PNG_BYTES,
        "GET /pic.png body must equal the written bytes VERBATIM: {body:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// A link to a non-existent markdown file 404s (the target resolves to no file
/// under the root).
#[test]
fn missing_markdown_target_is_404() {
    let (core, waiter, _dir) = start_core_in_dir(DOC);
    wait_for_markdown_active(&core, &waiter);

    let url = core.open_in_browser();
    let port = port_from_url(&url);

    let (status, body) = http_get(port, "/does-not-exist.md");
    assert_eq!(
        status, HTTP_NOT_FOUND,
        "GET /does-not-exist.md must return 404; body: {body:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}

/// Edge case: with a NON-markdown buffer active, `open_in_browser` returns an
/// EMPTY string (there is no markdown source to preview).
///
/// The startup buffer is markdown — the anonymous-buffer feature names it
/// `Untitled.md` and sets `filetype=markdown` — so this drives a genuinely
/// NON-markdown buffer by opening a real `.txt` file and waiting for the markdown
/// gate to drop to false.
#[test]
fn non_markdown_buffer_returns_empty_url() {
    let (core, waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    // Open a real plain-text file so the active buffer is a genuinely NON-markdown
    // document (empty/`text` filetype), not the markdown-named startup `Untitled.md`.
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let txt = dir.path().join("notes.txt");
    std::fs::write(&txt, "plain text, not markdown\n").expect("failed to write notes.txt");
    core.send_input(InputEvent::Open { path: txt });
    waiter.wait_for_flush(&core, FLUSH_TIMEOUT);

    // The markdown gate is fed by the out-of-band `wolf_current_file` notification,
    // so poll until it drops to false after the `.txt` open lands.
    let deadline = Instant::now() + FLUSH_TIMEOUT;
    while core.is_markdown_active() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for is_markdown_active() to become false after opening a .txt file"
        );
        waiter.drain();
        std::thread::sleep(POLL_INTERVAL);
    }

    assert!(
        !core.is_markdown_active(),
        "an opened .txt buffer must not be markdown"
    );

    let url = core.open_in_browser();
    assert!(
        url.is_empty(),
        "open_in_browser() on a non-markdown buffer must return an empty string; got {url:?}"
    );

    // Keep the temp dir alive until the assertions above have run.
    drop(dir);
    core.shutdown().expect("shutdown should be Ok");
}
