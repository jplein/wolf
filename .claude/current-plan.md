# Plan: Browser preview — follow links to other files

## Status: COMPLETE ✅

Reviewer APPROVED WITH NITS (traversal guard + binary safety verified correct). Nits resolved: FFI source doc-comments (`wolf-ffi/src/lib.rs:531,2041`) + 3 README mentions updated to `/<relpath>`; rustfmt clean; docs prettier-clean. Coder re-verified: `wolf_core_lib_test` pass, `wolf-ffi:all` 30/30, `wolf_ffi` builds. `browser_preview` 6/6, full 52 sweep green, clippy + denylist clean, no new crate. NOT committed (per user instruction).

Accepted non-blocking: rare non-canonical-root edge (only over-rejects, never escapes); no integration test for the unnamed-buffer empty-relpath live-at-`/` branch (unit-covered).



### Coder report (done, all green)
Implemented nvim.rs `buffer_path`, browser_preview.rs `Target`/helpers/`route`/`Vec<u8>` refactor, api.rs `open_in_browser` return-URL. Results: `wolf_core_lib_test` pass; `browser_preview` 6/6 (incl. traversal rejection, raw asset bytes, linked markdown static, missing-404); `//crates/wolf-core:all //crates/wolf-ffi:all` 52 pass; clippy clean (no await_holding_lock); denylist clean (only glibc false positive); no new crate. All frozen signatures/content-types matched.
- OPEN: 2 rustfmt diffs inside the frozen test module (`browser_preview.rs:777-778`, `:919`) — coder left them (no-test-edit rule); test-writer to run rustfmt.

### Test-writer report (done)
- `src/browser_preview.rs` mod tests: updated `http_response_*`/`not_found_*` to `Vec<u8>`; added `http_response_bytes` (non-UTF8 byte-exact), `redirect_response`, `content_type_for`, `is_markdown_path`, `relpath_between`, `encode_url_path`, `percent_decode`, `resolve_path` (tempdir traversal guard).
- `tests/browser_preview.rs`: added `path_from_url`, `http_get_bytes`, local `start_core_in_dir` (dedicated tempdir so siblings resolve as server root); modified happy-path to GET the relpath + `GET /`→302; added `linked_markdown_sibling_renders_static`, `path_traversal_is_rejected`, `linked_asset_serves_raw_bytes`, `missing_markdown_target_is_404`.
- **Coder MUST match these assumed signatures** (tests are frozen): `content_type_for(&Path) -> &'static str`, `is_markdown_path(&Path) -> bool`, `relpath_between(&Path, &Path) -> String`, `resolve_path(&Path, &str) -> Option<PathBuf>`, `http_response_bytes(&str, &str, &[u8]) -> Vec<u8>`, `redirect_response(&str) -> Vec<u8>`, `encode_url_path(&str)`, `percent_decode(&str)`. Content-type strings must be EXACTLY: `image/png`, `image/jpeg`, `image/svg+xml`, `text/css; charset=utf-8`, `text/javascript; charset=utf-8`, unknown→`application/octet-stream`.
- Test-writer wished for a `common::start_core_in_dir` helper (used a local one instead) — coder may optionally add it to `common/mod.rs` but not required.

## Problem statement

The "Preview in Browser" feature (already shipped) serves only the pinned buffer
at `GET /` and returns 404 for anything else. So clicking a relative link in the
rendered Markdown — e.g. the README's "Building Wolf" → `build.md` — navigates the
browser to `http://127.0.0.1:<port>/build.md`, which 404s ("Not found"). The
in-app pane preview follows the same link fine (it intercepts the WebKit `file://`
navigation and calls `preview_file(win, absolute_path)`).

**Goal:** make relative links in the browser preview work — clicking a link to
another Markdown file renders that file in the browser; linked assets (images,
etc.) also load — so the browser preview reaches parity with the in-app preview.

## Grounding (verified in code)

- Server: `crates/wolf-core/src/browser_preview.rs` — routes `/` (render pinned
  buffer + reload script), `/tick` (pinned buffer changedtick), else 404. Pinned
  target is `Arc<AtomicI64>` (bufnr). Hand-rolled HTTP/1.1 on tokio.
- `crates/wolf-core/src/nvim.rs`: `render_source_html(nvim, bufnr)` (pure render
  path shared with the in-app preview), `current_markdown_bufnr()`, a `getcwd()`
  helper (~line 195), `nvim_buf_get_name` usage, and `preview_file(win, path)`
  (the in-app link-follow — repoints a preview window at an ABSOLUTE path).
- `crates/wolf-core/src/markdown.rs::render_html` passes relative link hrefs
  through unchanged (its event rewriter only touches markers/headings/math), so
  `[Building Wolf](build.md)` → `<a href="build.md">` — no rewriting needed; the
  browser resolves it relative to the current page URL.
- `WolfCore::open_in_browser() -> String` (api.rs) returns `http://127.0.0.1:<port>/`.
- The macOS front-end just opens the returned URL via `NSWorkspace`; NO front-end
  change is needed for link-following (the browser handles navigation; the server
  serves the target). This stays macOS-agnostic / server-side.

## Design direction (architect to finalize)

Turn the server into a tiny static site rooted at the PROJECT ROOT (nvim
`getcwd()`), so relative links (including `../`) resolve correctly in URL space:

- Root the server at `ROOT` = nvim `getcwd()` (canonicalized). Fallback to the
  pinned file's parent directory when the file is not under `ROOT` or cwd query
  fails; handle an unnamed/unsaved buffer (empty path) gracefully.
- The pinned file is served at its ROOT-relative URL. RECOMMENDATION:
  `open_in_browser` returns `http://127.0.0.1:<port>/<relpath>` DIRECTLY (cleanest;
  the browser address bar shows the real path and relative links resolve against
  it). Alternative: keep returning `/` and 302-redirect `/` → `/<relpath>`.
  Architect picks one and justifies; note the integration-test impact either way.
- `GET /<relpath>`: canonicalize `ROOT/<url-path>` (URL-decode, strip query/frag),
  and REJECT (404) any path that escapes `ROOT` (path-traversal guard — hard
  requirement, even though loopback-only). Then:
  - If the resolved path is the PINNED file → render live from its buffer via
    `render_source_html` + inject the reload script (the live-editing page).
  - Else if it is a Markdown file (`.md`/`.markdown`) that exists → render from
    DISK via `markdown::render_html` (a static snapshot; linked pages are not
    live — document this). No reload script (or a benign one).
  - Else if it exists (image/css/other asset) → serve RAW bytes with a content
    type guessed from the extension (so `![img](pic.png)` and any linked asset
    load).
  - Else → 404.
- `/tick` unchanged (pinned buffer). Live reload continues to apply to the pinned
  file only; navigating to a linked page is static. Justify this scope.
- Keep the module GUI-free, hand-rolled on tokio, loopback-only. No new crate
  (content-type gufrom extension is a tiny match; do NOT add a mime crate).

## Constraints

- wolf-core ONE HARD RULE: no GUI toolkit dep; no new crate (extension→content-type
  is a hand-written match). Re-verify the denylist if anything is added.
- Loopback-only bind stays; the traversal guard is a NEW hard security requirement
  (a loopback client must not read files outside ROOT).
- Prefer end-to-end/integration tests (real nvim + real loopback socket).
- Docs are source of truth: update `browser_preview.rs` module doc + the READMEs
  that describe the routes / behavior; `prettier --write` edited Markdown.

## Do NOT commit or push unless the user asks.

---

## Architect's plan

**Scope: wolf-core only** (no FFI/Swift/discriminant change — the front-end opens whatever URL string it's handed).

### Decisions
- **D1 Root** = `canonicalize(getcwd())`. Fallback (cwd fails, or pinned file not under cwd, or unnamed buffer) = `canonicalize(pinned_abs.parent())`. NOTE: the existing integration test's temp `.md` is OUTSIDE nvim cwd → exercises the parent-dir fallback (root = temp dir, relpath = filename).
- **D2 Return value**: `open_in_browser` returns `http://127.0.0.1:<port>/<url-encoded relpath>` (or `.../` when unnamed). `GET /` **302-redirects** to `/<relpath>` (or serves pinned live when relpath empty). URL-encode each `/`-split segment (unreserved `A-Za-z0-9-_.~` kept; `build.md`→`build.md`, `a b.md`→`a%20b.md`).
- **D3 Path→FS (traversal guard, HARD security req)**: `resolve_path(root, url_target)`: strip `?`/`#`, percent-decode, `trim_start_matches('/')`, `root.join(rel)`, `std::fs::canonicalize` (non-existent → None → 404), `starts_with(root)` else None. `root` must be canonical (macOS `/var`→`/private/var`). Canonicalize BEFORE starts_with (symlinks/`..` normalized).
- **D4 Live vs static**: pinned file → live from buffer via `render_source_html`+`render_page` (reload script). Other markdown → static from DISK via `markdown::render_html(.., PreviewTheme::Default)`, NO reload script (`/tick` tracks only pinned). "Is pinned?" = decoded rel == stored `pinned_relpath` (also defensive canonical `pinned_abs` compare).
- **D5 Assets + binary-safe**: non-markdown existing files → raw bytes via `std::fs::read` + `content_type_for(path)` hand-written match (no mime crate). REFACTOR the whole write path to `Vec<u8>`: new `http_response_bytes(status, content_type, &[u8]) -> Vec<u8>`; `http_response(..&str)` delegates. Images MUST NOT pass through `from_utf8_lossy`.
- **D6 Markdown exts** (case-insensitive): `md`, `markdown`, `mdown`, `mkd`, `mkdn`, `mdwn`. File I/O = blocking `std::fs` (no tokio `fs` feature; tiny local files, multi-thread runtime).

### Files to modify
**`browser_preview.rs`**: new `pub(crate) struct Target { bufnr, root: PathBuf, pinned_abs: PathBuf, pinned_relpath: String }` (Clone). Change `BrowserPreview.target` `Arc<AtomicI64>`→`Arc<parking_lot::Mutex<Target>>`; `set_target(Target)`; `spawn(.., initial: Target)`; thread through `accept_loop`/`handle_conn`. New helpers: `build_target(bufnr, cwd: Option<PathBuf>, pinned_raw) -> Target` (FS canonicalize), pure `relpath_between(root, abs) -> String`, `encode_url_path(rel)`, `percent_decode(s)`, `resolve_path(root, url_target) -> Option<PathBuf>`, `is_markdown_path(path)`, `content_type_for(path) -> &'static str`, `http_response_bytes`, `redirect_response(location)`. Refactor `http_response`/`not_found_response`→`Vec<u8>`; `handle_conn` writes `&Vec<u8>`. Extract `async fn route(..) -> Vec<u8>`: `/tick` unchanged; clone `Target` out of the lock and DROP guard before any `.await`; `/`→ pinned-live-if-empty else 302; if rel==pinned_relpath → pinned live; else `resolve_path` → None=404 / ==pinned_abs=live / markdown=disk-render / else=raw bytes. Rewrite module doc Routes section.
**`nvim.rs`**: add `pub async fn buffer_path(&self, bufnr: i64) -> Result<String, NvimError>` (`nvim_buf_get_name`, empty when unnamed). Reuse `render_source_html`, `current_dir` (getcwd ~line 202).
**`api.rs`**: rewrite `open_in_browser` — block_on `current_markdown_bufnr` (≤0→""), `buffer_path(bufnr)`, `current_dir()` — ALL before the lock; `build_target`; lock reuse-or-spawn; build URL per D2. Update doc comments.

### Tests (for test-writer) §5
**Unit (`browser_preview.rs` mod tests)**: CHANGE `http_response_frames_*` + `not_found_response_is_404` to `Vec<u8>` (assert via `from_utf8_lossy` or byte slices). ADD: `content_type_for` (png/jpg/jpeg/svg/css/js/unknown, case-insensitive), `is_markdown_path`, `relpath_between` (pure), `encode_url_path`, `percent_decode`, `resolve_path` (tempdir: valid/percent/query-strip/`..`-escape/`%2e%2e`-escape/non-existent), `http_response_bytes` (non-UTF8 body `[0xFF,0x00,0xFE]` byte-exact + Content-Length), `redirect_response` (302 + Location + CL 0). Keep `render_page_*`/`reload_script_*`/`fallback_page_*`/`parse_request_target_*`/`preview_listener_binds_loopback_only`.
**Integration (`tests/browser_preview.rs`)**: add `path_from_url(url)` helper + `http_get_bytes(port, path) -> (u16, Vec<u8>, content_type)`. MODIFY happy-path test: GET the returned relpath (`path_from_url`) not `/`; optional `GET /`→302 assert (don't follow). ADD (each in a dedicated `tempdir`, open `<dir>/test.md`, write siblings there): (a) `<dir>/build.md` → `GET /build.md` 200, contains `wolf-markdown`+build text, NOT `location.reload`; (b) `GET /../../etc/passwd` and `/%2e%2e/%2e%2e/etc/passwd` → 404, body lacks `root:`; (c) `<dir>/pic.png` known bytes → `GET /pic.png` 200, `Content-Type: image/png`, exact bytes; (d) `GET /missing.md` → 404. Keep `non_markdown_buffer_returns_empty_url`.

### Docs (technical-writer) §8
`browser_preview.rs` module doc; `crates/wolf-core/README.md` "browser preview server" (return value + routes + traversal guard + Target); `crates/wolf-core/src/README.md` (browser_preview section + `buffer_path` bullet); `crates/wolf-core/tests/README.md` (new cases + bytes helper); grep `command-lifecycle.md`/`architecture.md` for the returned-URL/`127.0.0.1` and update only if they state the old `/` route. `prettier --write` each.

### Order: nvim.rs `buffer_path` → browser_preview.rs (Target+helpers+route, Vec<u8> refactor) → api.rs → unit tests → integration → docs.
### Risks: binary safety (Vec<u8> path, never lossy an image); traversal guard (canonicalize before starts_with, canonical root for macOS /var symlink); lock-across-await (clone Target out, drop guard before await → clippy::await_holding_lock); no new crate (hand-written encode/decode/mime); loopback bind unchanged; return-URL ripple into the integration test.
