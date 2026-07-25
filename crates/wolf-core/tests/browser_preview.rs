//! Integration test for `wolf-core`'s BROWSER PREVIEW server: the lazily-started,
//! loopback-only HTTP server behind `WolfCore::open_in_browser`.
//!
//! This drives a REAL, hermetic Neovim (no mocks) via the shared `tests/common`
//! harness — exactly like `theme.rs` / `commands.rs` — AND a REAL loopback TCP
//! socket. The HTTP client is HAND-ROLLED on `std::net::TcpStream` (see
//! [`http_get`]): the plan's dependency denylist forbids an HTTP-client crate
//! (reqwest/hyper/ureq) reaching the Bazel test deps, and `std::net` adds zero
//! dependency nodes.
//!
//! The server (per the plan):
//!   * binds `127.0.0.1:0` (a random OS-assigned port), loopback ONLY;
//!   * serves `GET /` = `markdown::render_html` of the pinned source buffer, with
//!     an injected auto-reload script that polls `GET /tick` and calls
//!     `location.reload()` when the buffer's `changedtick` advances;
//!   * serves `GET /tick` = the current `changedtick` as text;
//!   * 404s everything else.
//!
//! Because the `wolf_current_file` / `wolf_markdown_dirty` notifications are
//! out-of-band (they wake but do not bump redraw `seq`), the waits here POLL —
//! `is_markdown_active()` for the initial markdown gate, and `GET /tick` for the
//! post-edit tick bump — each bounded by [`FLUSH_TIMEOUT`] so a hang fails loudly.
//!
//! NOTE to coder / ASSUMPTIONS (flagged for alignment):
//!   * `WolfCore::open_in_browser(&self) -> String` returns the loopback URL
//!     (`http://127.0.0.1:<port>/`) or an EMPTY string when the active buffer is
//!     not markdown / on error (plan §4 `api.rs`).
//!   * The rendered page carries the `wolf-markdown` body wrapper (verified in
//!     `markdown::render_html`), the `/tick` poll endpoint, and a `location.reload`
//!     call — the three robust body markers asserted below.
//!   * The buffer is mutated the same way `save_open.rs` does: `i` + typed text +
//!     `<Esc>`, then a flush wait.

mod common;

use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use common::{ch, key, type_str, FlushWaiter, FLUSH_TIMEOUT};
use wolf_core::{Key, WolfCore};

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
/// HTTP status for an unknown path.
const HTTP_NOT_FOUND: u16 = 404;

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
        "open_in_browser() must return a http://127.0.0.1:<port>/ url; got {url:?}"
    );
    let port = port_from_url(&url);

    // (3) GET / -> 200 with the rendered markdown body + the two injected reload
    // markers. `wolf-markdown` is the render_html body wrapper; `hello` is the
    // source text; `/tick` + `location.reload` are the injected poller.
    let (status, body) = http_get(port, "/");
    assert_eq!(status, HTTP_OK, "GET / must return 200; body: {body:?}");
    assert!(
        body.contains(r#"class="wolf-markdown""#),
        "GET / body must carry the wolf-markdown body wrapper: {body:?}"
    );
    assert!(
        body.contains("hello"),
        "GET / body must contain the rendered source text 'hello': {body:?}"
    );
    assert!(
        body.contains("/tick"),
        "GET / body must reference the /tick poll endpoint: {body:?}"
    );
    assert!(
        body.contains("location.reload"),
        "GET / body must inject a location.reload() auto-refresh: {body:?}"
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

    // (6) After the edit, a fresh GET / re-render contains the newly typed text.
    let (status, body) = http_get(port, "/");
    assert_eq!(
        status, HTTP_OK,
        "GET / after the edit must still return 200; body: {body:?}"
    );
    assert!(
        body.contains(EDIT_TOKEN),
        "GET / after the edit must contain the newly typed text {EDIT_TOKEN:?}: {body:?}"
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

/// Edge case: with a NON-markdown buffer active, `open_in_browser` returns an
/// EMPTY string (there is no markdown source to preview). A bare core's initial
/// scratch buffer has an empty filetype, so `is_markdown_active()` is false.
#[test]
fn non_markdown_buffer_returns_empty_url() {
    let (core, _waiter) = common::start_core_sized(common::DEFAULT_COLS, common::DEFAULT_ROWS);

    assert!(
        !core.is_markdown_active(),
        "a bare core's scratch buffer must not be markdown"
    );

    let url = core.open_in_browser();
    assert!(
        url.is_empty(),
        "open_in_browser() on a non-markdown buffer must return an empty string; got {url:?}"
    );

    core.shutdown().expect("shutdown should be Ok");
}
