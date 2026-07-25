# Plan: "Preview in Browser" command

## Status: COMPLETE ✅

Nit 1 fixed (api.rs field-doc reworded, rustfmt clean, builds). Docs updated across 10 files by technical-writer; `prettier --write` run by orchestrator (all already clean, `proseWrap: never`). Feature implemented, all tests green, reviewer APPROVED, docs synced. NOT committed (per user instruction).



### Reviewer verdict: APPROVED WITH NITS
All hard requirements verified: loopback-only bind (`Ipv4Addr::LOCALHOST`, browser_preview.rs:96), no snapshot-guard-across-await, bufnr resolved outside the lock, discriminant 21 intact across all 6 mirrors, tokio gained only `net`, wolf-gnome untouched, bindings additive-only, Swift wiring + markdown gating correct.
- Nit 1 (coder): `api.rs:183-184` field doc overstates "never held across block_on" — the bind block_on IS under the lock (benign). Reword to "never held across the nvim RPC block_on".
- Nit 2 (technical-writer): docs pending — `crates/wolf-core/src/README.md`, `crates/wolf-core/README.md`, `crates/wolf-ffi/README.md` (`0..=20`→`0..=21`, add `:WolfPreviewInBrowser`), `architecture.md` §10, `command-lifecycle.md`, wolf-macos READMEs.

### Unit-test gap closed
Test-writer added 12 in-module `#[cfg(test)]` unit tests to `src/browser_preview.rs` (parse/render_page/reload_script/fallback/http_response/404 + a `#[tokio::test]` loopback-bind). Coder ran them: `//crates/wolf-core:wolf_core_lib_test` 130/130 (was 118, +12); integration `browser_preview` passing; `//crates/wolf-core:all //crates/wolf-ffi:all` 52/52. No code changes needed.

### Coder report (done, all green)
- Implemented all of §2–§9, §12. Inline Lua for `current_markdown_bufnr` (no init.lua change). `BrowserPreview::abort()` exists but not wired into shutdown (runtime drop cancels task; avoids lock-across-block_on).
- Results: `browser_preview` 2/2; wolf-core+wolf-ffi 52/52; `WolfKeyMapTests` + `bundle_layout_test` pass; `bazel build //crates/wolf-macos:Wolf` builds; denylist clean (only documented `glibc` false positive); bindings regenerated (adds only `open_in_browser`); rustfmt+clippy clean. `Cargo.Bazel.lock` updated by repin; `Cargo.lock` unchanged (net adds no crate node).
- GAP: plan §3/§10b in-module `#[cfg(test)] mod tests` (pure-helper unit tests) were NOT written (coder correctly leaves test code). Routing to test-writer.

### Test-writer report (done)
- CREATED `crates/wolf-core/tests/browser_preview.rs` — happy path (serve rendered HTML + live `/tick` + reuse + 404 + shutdown) and non-markdown-returns-"" edge case; `http_get` helper on `std::net::TcpStream` only. Buffer mutation uses the `save_open.rs` insert pattern (`ch('i')` + `type_str` + `Key::Esc`), token `freshinserttoken`.
- MODIFIED `command.rs` mod tests (count 22, `PREVIEW_IN_BROWSER_INDEX=21`, migrated `terminal_is_the_last_row`→`terminal_is_at_index_20`, 3 new preview_in_browser_* tests).
- MODIFIED `palette_rank.rs` (`STATIC_COMMAND_COUNT`→21).
- MODIFIED `wolf-ffi/tests/commands.rs` (`CMD_KIND_PREVIEW_IN_BROWSER=21`, guard arrays 0..=21, drain step 3l).
- MODIFIED `CommandSeamTests.swift` (ffi 21→.previewInBrowser, oob probe→22).
- NOTE: test-writer did NOT add the `browser_preview` BUILD.bazel target (that's coder's job, §4e). Assumed API: `open_in_browser(&self) -> String`.

## Problem statement

Add a new user command **"Preview in Browser"** to Wolf that renders the current
Markdown file and opens it in the system default web browser, backed by a
lazily-started local HTTP server owned by the project's captive core.

## Confirmed decisions (from the user)

1. **Live reload.** The server re-renders on change and the already-open browser
   tab auto-refreshes. Reuse the same signal that drives the in-app preview —
   `Snapshot.markdown_ticks` (source buffer `changedtick` via `wolf_markdown_dirty`)
   and the `markdown::render_html` / `render_preview` path. Inject a tiny JS poller
   into the served HTML that watches for a tick bump and reloads. Prefer a simple
   poll endpoint over websockets to keep deps minimal.

2. **macOS only.** Wire the command through `wolf-core`, `wolf-ffi`, and
   `wolf-macos` (AppKit menu + `CommandDispatcher` + palette). Do NOT wire
   `wolf-gnome`. Follow the append-only discriminant contract: append the new
   `Command` variant LAST (after `Terminal` at 20, so the new one is **21**), add
   the `COMMAND_SPECS` row, bump `EXPECTED_COMMAND_COUNT`, add index/round-trip/spec
   tests, add the `command_kind` constant + `FfiCommand::from_command` arm, extend
   the Swift `CommandKind` `init?(ffi:)` + `CommandSeamTests`, extend
   `crates/wolf-ffi/tests/commands.rs` contiguous `0..=N` guard, add `AppMenu` Tag +
   menu item, add `CommandDispatcher` arm. Gate the menu item's enablement on
   `is_markdown_active` (mirror the `Preview` command precedent). Update
   `STATIC_COMMAND_COUNT` in `crates/wolf-core/tests/palette_rank.rs`.

3. **One server per project** (per captive core). Lazily started — not until the
   user first runs the command. Bind ONLY to `127.0.0.1` on a RANDOM port
   (`127.0.0.1:0`, read back the OS-assigned port). URLs are unique between
   projects because each core's server gets its own random port. The front-end
   opens the returned URL in the default browser (`NSWorkspace.open`).

## Architectural constraints (critical)

- **wolf-core's ONE HARD RULE:** must compile with NO GUI toolkit in its
  dependency tree (denylist + `cargo tree` / `bazel query` checks). Any HTTP
  server dependency MUST be GUI-free, pure-Rust. Prefer minimal (`tiny_http`) OR
  hand-rolled HTTP/1.1 on the existing tokio runtime. RE-VERIFY the denylist stays
  clean and DOCUMENT the new dependency in `crates/wolf-core/README.md` (rusqlite/
  ignore/serde/pulldown precedents). Update `Cargo.lock` + Bazel crate_universe pin
  if a crate is added (macOS builds under Bazel).
- Server serves the RENDERED HTML of the current markdown buffer, reusing
  `markdown::render_html` so browser and in-app preview match.
- Binds `127.0.0.1` ONLY (never `0.0.0.0`). Hard security requirement — verify in
  a test.
- Lazy start; subsequent invocations reuse the same server/port.

## Suggested core API shape (architect to refine)

- New blocking `WolfCore` method e.g. `open_in_browser(&self) -> Option<String>`:
  lazily starts the per-core server (Arc handle stored in `WolfCore`), determines
  the current markdown source buffer, returns the URL (None if not markdown / on
  error). Front-end opens the URL.
- Server serves `GET /` -> `render_html` of current source buffer (re-rendered per
  request) with injected auto-reload script; `GET /tick` -> current changedtick as
  text; injected JS polls `/tick` and reloads on change.

## Testing requirements (CLAUDE.md)

- Prefer end-to-end/integration tests over mocks; start a real Neovim.
- wolf-core integration test: start core, open a markdown buffer, call
  `open_in_browser`, GET the URL over 127.0.0.1, assert rendered HTML returned and
  `/tick` works, assert listener bound to 127.0.0.1 only.

## Documentation requirements (CLAUDE.md)

- Docs are source of truth. Update READMEs in every touched dir (wolf-core
  public-API + "How core embeds Neovim" if relevant, `command-lifecycle.md`
  checklist, `architecture.md` §10, wolf-ffi README, wolf-macos READMEs).
  `prettier --write` every edited Markdown file.
- Regenerate swift-bridge bindings (`bazel run //crates/wolf-ffi:gen_bindings`) and
  commit `crates/wolf-ffi/generated/` after FFI changes.

## Do NOT commit or push unless the user asks.

---

## Architect's plan

### 1. Summary

New command **"Preview in Browser"** (id `preview_in_browser`, ex `:WolfPreviewInBrowser`, discriminant **21**, View menu). Lazily starts a single loopback-only HTTP server per `WolfCore` (bound `127.0.0.1:0`, random port), points it at the markdown buffer active when the command fired, returns `http://127.0.0.1:<port>/`; macOS opens it via `NSWorkspace`. Server re-renders on each `GET /` via `markdown::render_html` and injects a JS poller hitting `GET /tick` every 1s → `location.reload()` when `changedtick` (from `Snapshot.markdown_ticks`) advances. Modeled on the blocking `open_preview`/`render_preview` methods.

### 2. Design decisions

- **2a. HAND-ROLLED HTTP/1.1 server on tokio (no crate).** Enable tokio's `net` feature only. `mio`/`socket2`/`libc` already in `Cargo.lock` (via tokio `process`), so `net` adds NO new crate node — denylist stays clean. Cargo.toml line 14 → `features = ["rt-multi-thread", "macros", "io-util", "process", "sync", "net", "time"]`. Serves only `GET /`, `GET /tick`, 404. Re-run BOTH denylist checks; repin crate_universe (`CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core`) since features changed; commit lockfiles.
- **2b. PINNED-per-invocation target** via `Arc<AtomicI64>`. Each `open_in_browser` resolves the current active markdown bufnr and STORES it as the new target, returns the stable URL. Server does not live-follow between invocations.
- **2c. Tick source** = `Snapshot.markdown_ticks.get(&target).copied().unwrap_or(0)`, fed by `wolf_markdown_dirty` (fires for any `filetype==markdown` buffer, not gated on a preview window). `0` until first edit; page embeds render-time tick as JS baseline to avoid a spurious first-poll reload.
- **2d. Test HTTP client = `std::net::TcpStream`** + hand-written GET (no crate). Bazel `deps()` query is not `-e no-dev`-scoped and wolf-core test targets pull `normal_dev` deps, so a dev-dep client risks the Bazel denylist; `std::net` adds zero deps.

### 3. Files to CREATE

**`crates/wolf-core/src/browser_preview.rs`**:
- `pub struct BrowserPreview { port: u16, target: Arc<AtomicI64>, task: AbortHandle }` — `port()`, `set_target(bufnr)` (atomic store), optional `abort()`.
- `pub(crate) fn spawn(runtime, nvim, snapshot: SharedSnapshot, initial_bufnr) -> io::Result<BrowserPreview>`: `runtime.block_on(TcpListener::bind((Ipv4Addr::LOCALHOST, 0)))`, read `local_addr().port()`, `runtime.spawn(accept_loop(...))`, capture `abort_handle()`.
- `async fn accept_loop(listener, nvim, snapshot, target)`: loop accept → `tokio::spawn(handle_conn(...))`, log+continue on error.
- `async fn handle_conn(stream, nvim, snapshot, target)`: read head until `\r\n\r\n` (cap ~8 KiB), `parse_request_target`, route:
  - `/` → `bufnr = target.load()`; `render_source_html(&nvim, bufnr).await`; read tick; `render_page(html, tick)`; 200 `text/html`. On Err → `fallback_page(tick)` (still carries reload script).
  - `/tick` → tick as text, 200 `text/plain`.
  - else → 404.
- Pure helpers (module `#[cfg(test)]`): `parse_request_target`, `render_page` (inject reload script before the single `</body>` via `replacen(…,1)`), `reload_script(tick)`, `fallback_page(tick)`, `http_response`, `not_found_response`.
- **Injected reload JS**: poll `/tick` every **1000ms** with `fetch('/tick',{cache:'no-store'})`; `parseInt`; if `!isNaN && n !== baseline` → `location.reload()`; `.catch` keeps polling. Baseline = render-time tick.
- Module unit tests: `parse_request_target` (valid `/`,`/tick`, malformed, non-GET); `render_page` (contains body + `location.reload` + `/tick` + baseline, exactly one `</body>`); `reload_script` embeds tick; `not_found_response` = 404; bind test asserts `local_addr().ip().is_loopback()`.

**`crates/wolf-core/tests/browser_preview.rs`** — real-nvim + real loopback socket (see §10).

### 4. Files to MODIFY (wolf-core)

- **`command.rs`**: enum doc + append `PreviewInBrowser` variant after `Terminal`; append `COMMAND_SPECS` row (`preview_in_browser`/`WolfPreviewInBrowser`/"Preview in Browser"/View). Tests: `EXPECTED_COMMAND_COUNT` → **22**; add `PREVIEW_IN_BROWSER_INDEX = 21`; migrate `terminal_is_the_last_row` → fixed-index `terminal_is_at_index_20` (drop `==len()-1`); update `TERMINAL_INDEX` doc ("no longer last"); add `preview_in_browser_id_round_trips`, `preview_in_browser_spec_fields`, `preview_in_browser_is_the_last_row` (index 21 AND `==len()-1`); update count message.
- **`nvim.rs`**: extract `render_preview` body into `pub(crate) async fn render_source_html(nvim, source_bufnr) -> Result<String, NvimError>` (buf_get_lines + join + `markdown::render_html`); `render_preview` calls it. Add `pub async fn current_markdown_bufnr(&self) -> Result<i64, NvimError>` (inline `nvim_exec_lua`: `(vim.bo.filetype=='markdown') and nvim_get_current_buf() or 0`). If coder prefers a named `wolf_*` helper, also update `init.lua` + `runtime/nvim/README.md`.
- **`api.rs`**: `use crate::browser_preview::BrowserPreview;`; add field `browser_preview: Mutex<Option<BrowserPreview>>` (init `Mutex::new(None)` in `start`). Add blocking `pub fn open_in_browser(&self) -> String`: resolve bufnr via `runtime.block_on(nvim.current_markdown_bufnr())` (outside the lock); if `<=0` return ""; lock slot; if Some reuse+`set_target`; if None `browser_preview::spawn(...)` (Err→log+""); `format!("http://127.0.0.1:{port}/")`. Optional explicit `abort()` in `shutdown` (do NOT hold the lock across `block_on`).
- **`lib.rs`**: `pub mod browser_preview;` + module doc bullet; `pub use browser_preview::BrowserPreview;`.
- **`BUILD.bazel`**: add `browser_preview` `rust_test` target (copy of `commands`: srcs `tests/browser_preview.rs` + `tests/common/mod.rs`, `no-sandbox`, same data/env, `normal_dev` deps).

### 5. Files to MODIFY (wolf-ffi) — `src/lib.rs`

- `mod command_kind`: `pub const PREVIEW_IN_BROWSER: u8 = 21;`.
- Bridge decl in `extern "Rust"`: `fn open_in_browser(&self) -> String;` (after preview methods).
- `WolfCoreHandle` impl: `pub fn open_in_browser(&self) -> String { self.core.borrow().as_ref().map_or_else(String::new, WolfCore::open_in_browser) }`.
- `FfiCommand::from_command`: arm `Command::PreviewInBrowser => command_kind::PREVIEW_IN_BROWSER,` (keeps match exhaustive).
- Doc ranges (`FfiCommand::kind` doc, module doc) extend ".. TERMINAL" → ".. PREVIEW_IN_BROWSER".
- **`tests/commands.rs`**: header/contract comment → `PREVIEW_IN_BROWSER = 21`, "contiguous 0..=21"; `const CMD_KIND_PREVIEW_IN_BROWSER: u8 = 21;`; add to both guard arrays (`21`); drain step `3l` `:WolfPreviewInBrowser` → `poll_for_command_kind(..., CMD_KIND_PREVIEW_IN_BROWSER, ...)`. **NOTE for test-writer**: this only proves the discriminant round-trips; it does NOT start the server (no front-end). Assert no HTTP here.

### 6. Files to MODIFY (wolf-macos)

- **`CommandSeam.swift`**: `case previewInBrowser` after `terminal`; `init?(ffi:)` `case 21: self = .previewInBrowser`; header doc `previewInBrowser == 21`.
- **`CommandSeamTests.swift`**: add `XCTAssertEqual(CommandKind(ffi: 21), .previewInBrowser, ...)`; bump out-of-range probe `21` → **`22`**; header doc range 0..21.
- **`CommandDispatcher.swift`**: `case .previewInBrowser: controller?.openInBrowser()`.
- **`WolfWindowController.swift`**: add `func openInBrowser()`: `guard let handle else { return }; let url = handle.open_in_browser().toString(); guard !url.isEmpty, let u = URL(string: url) else { return }; NSWorkspace.shared.open(u)`.
- **`AppMenu.swift`**: `static let previewInBrowser = 21`; View-menu item after Terminal, before separator: `commandItem(title: "Preview in Browser", tag: Tag.previewInBrowser, keyEquivalent: "", target: target)` (**NO accelerator** — matches Preview/Edit Source).
- **`AppDelegate.swift`**: `validateMenuItem` case `.previewInBrowser: return keyController?.isMarkdownActive ?? false` (mirror `.preview`).

### 7. `crates/wolf-core/tests/palette_rank.rs`
`STATIC_COMMAND_COUNT` 20 → **21**; update doc (22 registered, `PreviewInBrowser` at 21, palette gates nothing so it is a static row like Preview/Edit/Terminal).

### 8. swift-bridge regen (REQUIRED)
`bazel run //crates/wolf-ffi:gen_bindings`; commit `crates/wolf-ffi/generated/`.

### 9. Dependencies
No new crate. Only tokio `net` feature. Repin crate_universe; commit lockfiles.

### 10. Test plan (for test-writer)

**`tests/browser_preview.rs`** (real nvim + `std::net::TcpStream`; model on `tests/theme.rs`/`tabpages.rs`; `mod common;`; SKIP when `WOLF_NVIM_BIN` unset). `fn http_get(port, path) -> (status, body)` helper.
1. Start core; write `test.md` (`# Title\n\nhello world\n`); `Open`; poll `is_markdown_active()` true.
2. `url = open_in_browser()`; non-empty, `starts_with("http://127.0.0.1:")`; parse port.
3. `GET /` → 200; body contains `<body class="wolf-markdown"` and `hello` AND `/tick` + `location.reload`.
4. `GET /tick` → 200; body parses as integer.
5. Edit buffer so `TextChanged` fires; poll `/tick` until it differs from tick₁.
6. After edit, `GET /` body contains the newly typed text.
7. `GET /nope` → 404.
8. Second `open_in_browser()` → same port (reuse).
9. `shutdown()` → Ok.
Edge: non-markdown buffer → `open_in_browser()` returns ""; tick 0 before first edit still serves.

**Module `#[cfg(test)]` unit tests** (no nvim/socket): as listed in §3.

**Existing contract guards updated**: `command.rs` (§4), `commands.rs` (§5), `CommandSeamTests.swift` (§6), `palette_rank.rs` (§7).

### 11. Documentation (for technical-writer)
`command-lifecycle.md` (enum list 21, command_kind list, Swift map, note the hybrid "dispatcher opens a URL from a blocking core call" flavor); `architecture.md` §10 (command list + loopback preview server paragraph); `crates/wolf-core/README.md` (`open_in_browser` bullet, "Browser preview server" note, Commands 22 rows, dependency/denylist paragraph = no crate, only tokio `net`); `crates/wolf-core/src/README.md` (`browser_preview` module); `crates/wolf-ffi/README.md` + `src/README.md` (bridge method + `PREVIEW_IN_BROWSER=21`); `crates/wolf-ffi/tests/README.md` (drains 0..=21); `crates/wolf-macos/README.md` + `Sources/README.md` + `Tests/README.md` (CommandKind case, dispatcher arm, `openInBrowser()`, menu item + gating, tests 0..21/oob-22); `crates/wolf-core/tests/README.md` (new integration test + no-sandbox target); `runtime/nvim/README.md` only if a named Lua helper is used. `prettier --write` every edited MD; no hard line breaks in paragraphs.

### 12. Implementation order
1. `command.rs` registry+tests. 2. `Cargo.toml` tokio `net`. 3. `nvim.rs` refactor+`current_markdown_bufnr`. 4. `browser_preview.rs`+`lib.rs`. 5. `api.rs` field+method. 6. `wolf-ffi` const/arm/bridge/handle. 7. regen bindings. 8. Swift files. 9. contract-guard tests. 10. integration test+BUILD. 11. repin+docs+prettier.

**Verification**: denylist (both checks) → `cargo test -p wolf-core` → `cargo test -p wolf-ffi` → `CARGO_BAZEL_REPIN=1 bazel build //crates/wolf-core:wolf_core` → `bazel run //crates/wolf-ffi:gen_bindings` → `bazel test //crates/wolf-core:browser_preview //crates/wolf-core:commands //crates/wolf-core:palette_rank //crates/wolf-ffi:commands //crates/wolf-macos/...`.

### 13. Risks (coder must heed)
tokio `net` required (compile error if missing) + repin. NEVER hold a lock across `.await` (drop snapshot guard before render await; resolve bufnr before taking `browser_preview` lock). Blocking `open_in_browser` uses `runtime.block_on` (runs on AppKit main thread). Loopback-only bind `(Ipv4Addr::LOCALHOST, 0)` — never `0.0.0.0`. Server stored on `WolfCore` (outlives call); shutdown drops runtime→cancels task (fallback page during teardown ok). Idempotent lazy start via `Mutex<Option<..>>`. `render_page` uses `replacen(…,1)`. Tick baseline avoids spurious first reload. Append-only discriminant contract — all mirrors move in lockstep (incl. oob probe → 22). swift-bridge regen mandatory before Swift links. Do NOT wire wolf-gnome.
