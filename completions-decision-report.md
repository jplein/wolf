# Command-line completions: decision report

> **Outcome / what shipped (2026-07).** This report's hybrid recommendation (`ext_cmdline` signal + `getcompletion()` for the list + a picker-style overlay for the menu) was **SUPERSEDED**. What shipped instead is **Option C — fully native, Tab-triggered**: Wolf attaches with `ext_cmdline` + `ext_popupmenu` (via `ext_messages`, which implicitly enables `ext_cmdline` and sets `cmdheight=0`) and lets Neovim own the completion match state + Tab-cycling. There is **no `getcompletion()` call and no `cmdline_completions` core/FFI method**; the captive `init.lua` sets `wildoptions+=pum` so cmdline completion presents as a native vertical popup. Core folds `cmdline_show`/`_pos`/`_hide` into `Snapshot.cmdline`, `popupmenu_show`/`_select`/`_hide` into `Snapshot.pum`, and the front-ends paint the command line at the BOTTOM with the completion popup anchored directly above it (NOT a top-of-window field). The menu is Tab-triggered (Neovim's own completion), not type-ahead on every keystroke — the type-ahead UX in this report's mockup was NOT implemented. This report is retained for its background analysis of Mechanisms A/B; the **Decision** and **Design** sections below are superseded by Option C. This work ALSO added **`ext_messages`** for the message toast (Feature 2 — `msg_show` / `msg_clear` / `msg_history_show` folded into `Snapshot.toast`), which this report did not cover. See `architecture.md` decision 7 and `crates/wolf-core/src/README.md` for the as-built design.

## Goal

Give Wolf a live, as-you-type completion menu for Neovim's command line, matching this flow:

1. In a directory with `File A.md`, `File B.md`, `Something else.md`.
2. User presses `:` in normal mode; a command line appears.
3. User types `e ` (command line shows `:e `); a menu appears below listing all three files.
4. User types `Fi` (`:e Fi`); the menu narrows to `File A.md` and `File B.md`.
5. With an entry selected, Tab (or Enter) commits it into the command line.

The menu must update on **every keystroke**, before Tab is pressed.

## Background: what Neovim gives us

Neovim exposes command-line completion two different ways, and they are not equivalent.

### Mechanism A — `getcompletion()`, a plain RPC call

`getcompletion(pattern, type)` returns the completion list as data, over the same msgpack-RPC channel we already use for `getcwd()` (see `crates/wolf-core/src/nvim.rs:193`, `current_dir`). With `type = "cmdline"` Neovim parses `pattern` _as a command line_ and returns the matches it would offer:

```
nvim_call_function("getcompletion", ["e File", "cmdline"])
  -> ["File A.md", "File B.md", "Something else.md"]

nvim_call_function("getcompletion", ["e Fi", "cmdline"])
  -> ["File A.md", "File B.md"]
```

We pass the text after the colon; Neovim decides from the command what to complete (filenames after `:e`, options after `:set`, highlight groups after `:hi`, and so on). This is a pure query — it changes **no** editor state and requires **no** UI-attach flags.

### Mechanism B — the native UI extensions (`ext_cmdline` + `ext_popupmenu`/`ext_wildmenu`)

This is the "decision 7 deferred" route in `architecture.md`. Adding the flags at attach time (`crates/wolf-core/src/nvim.rs:128`, `ui_attach`) makes Neovim stop drawing the command line / wildmenu into the grid and instead emit structured events:

- `cmdline_show` — the command-line content, prompt, and cursor position.
- `cmdline_pos` / `cmdline_hide` — cursor moves and dismissal.
- `popupmenu_show` / `popupmenu_select` / `popupmenu_hide` — the completion items, the selected index, and an anchor position; Neovim owns the match state and Tab cycling.

## The wrinkle: default Neovim is not live

The mockup shows the menu appearing the instant the user types `:e ` — before any Tab. That is **not** default Neovim behavior. Vanilla Neovim only triggers command-line completion on the wildchar (Tab). Mechanism B by itself therefore gives a Tab-driven wildmenu, not a type-ahead menu.

To get the live behavior, _something_ has to call completion on every keystroke. `getcompletion()` is exactly that something: it lets us compute the list ourselves whenever the command-line text changes, independent of Neovim's Tab trigger.

## Decision (SUPERSEDED — see "Outcome / what shipped" above)

> This hybrid was NOT adopted. Option C (fully native, Tab-triggered `ext_popupmenu`, no `getcompletion()`) shipped instead. The following is the original recommendation, kept for the record.

**Adopt a hybrid: `ext_cmdline` for the command-line signal, `getcompletion()` for the list, and the existing picker-style native overlay for the menu.**

- **`ext_cmdline`** gives us an authoritative, structured "what is on the command line right now" event on every keystroke — cleaner than scraping the message grid — and lets us render the command line as a real widget at the **top** of the window (matching the mockup) instead of the bottom cmdline strip.
- **`getcompletion()`** turns that command-line text into a ranked list on each change, giving the live, type-ahead UX. All Neovim knowledge stays in `wolf-core`; the front-end stays semantically dumb (decisions 6–7).
- **The menu reuses the file picker's overlay** — `NSTableView` on macOS, `ListView` on GTK — so we render, navigate, and commit selections with machinery that already exists.

We explicitly do **not** adopt the pure `ext_popupmenu` wildmenu as the v1 mechanism: on its own it will not produce the type-ahead menu without extra `wildmode`/autocompletion configuration, and it is a larger step into deferred territory. It remains the correct long-term endpoint (see Open questions).

We do **not** enable `ext_popupmenu` in this phase. `ext_cmdline` is additive to the existing `ext_linegrid` + `ext_multigrid` attach and does not disturb the grid-drawn editor content; the popupmenu flag would redirect Neovim's own completion UI and is unnecessary because we drive the menu ourselves.

## Design (SUPERSEDED — Option C shipped, see "Outcome / what shipped" above)

> The design below describes the hybrid that was NOT built (no `cmdline_completions` query, a top-of-window field, per-keystroke type-ahead). The as-built design is the native `Snapshot.cmdline` / `Snapshot.pum` fold with a bottom command line + an anchored Tab-triggered popup. Kept for the record.

### 1. Core: a completions query (`wolf-core`)

Add a thin RPC wrapper alongside `current_dir` in `crates/wolf-core/src/nvim.rs`:

```rust
/// Returns command-line completions for `cmdline` (the text after `:`),
/// via nvim_call_function("getcompletion", [cmdline, "cmdline"]).
pub async fn cmdline_completions(&self, cmdline: &str) -> Result<Vec<String>, NvimError>;
```

Expose it on `WolfCore` (`crates/wolf-core/src/api.rs`) as a synchronous-facing call that the front-end can invoke on each command-line change:

```rust
/// Completions for the current command-line text. Empty vec on error
/// (a failed query must never block typing).
pub fn cmdline_completions(&self, cmdline: &str) -> Vec<String>;
```

It issues the query on the tokio runtime (the same `block_on` shape as `resolve_search_root` in `api.rs`) and returns the list. Ranking/filtering is Neovim's — we surface the order it returns. If we later want fuzzy narrowing beyond Neovim's prefix match, reuse `wolf_core::rank_matches` (the picker's ranker) on top; out of scope for v1.

### 2. Core: the command-line signal (`ext_cmdline`)

- Add `("ext_cmdline", true)` to the attach opts in `ui_attach` (`crates/wolf-core/src/nvim.rs:129`).
- Handle the new redraw events in the redraw folder (`crates/wolf-core/src/redraw.rs`): fold `cmdline_show` / `cmdline_pos` / `cmdline_hide` into a new snapshot field:

```rust
/// The active command line, or None when no command line is open.
pub struct CmdlineState {
    pub firstc: String,   // ":" for Ex, "/" or "?" for search
    pub prompt: String,   // for input()-style prompts
    pub content: String,  // the typed text (concatenated content chunks)
    pub pos: usize,       // cursor byte offset within content
}
Snapshot.cmdline: Option<CmdlineState>
```

`cmdline_show` sets it, `cmdline_pos` updates `pos`, `cmdline_hide` clears it. This fold fires the same redraw wake as everything else; no new wake channel is needed. Because `content` arrives as an array of `[attrs, text]` chunks, concatenate the text parts.

Note: once `ext_cmdline` is on, Neovim no longer draws the command line into grid 1 / the message grid, so the bottom cmdline strip's `:`/`/` text will go blank. The front-end must render the command line from `Snapshot.cmdline` instead — this is the intended move to a top-of-window native command line (see step 4).

### 3. FFI surface (`wolf-ffi`, macOS)

Bridge both the signal and the query in `crates/wolf-ffi/src/lib.rs`, following the existing opaque-handle + accessor pattern:

- On the per-frame handle (or a small dedicated cmdline accessor): `cmdline_active() -> bool`, `cmdline_firstc()`, `cmdline_content()`, `cmdline_pos()`.
- On `WolfCoreHandle`: `cmdline_completions(cmdline: &str) -> Vec<String>` (swift-bridge can return a `Vec<String>` / `RustVec`), so Swift can request the list on each change.

GTK consumes the same `wolf-core` API directly (no FFI).

### 4. Front-end: the command line + menu overlay

Reuse the picker overlay component on each platform.

macOS (`crates/wolf-macos`):

- When `cmdline_active()` becomes true, show a top-of-window command-line field displaying `firstc + content` with the cursor at `pos`. This is display-only — Neovim owns the text; keystrokes still flow to Neovim through the normal key path (`PaneContainerView.keyDown` -> `WolfKeyMap.translate` -> `send_key`), so `:e Fi`, backspace, `<C-w>`, etc. all behave exactly as Neovim defines them.
- On each render wake where the command line is active, call `cmdline_completions(content)` and populate the picker-style `NSTableView` below the field. Empty list -> hide the menu.
- Arrow keys / Tab move the selection **in the menu view** (front-end-owned highlight), the same as the picker. This selection is a front-end concern; it does not go to Neovim until committed.
- Commit (Tab or Enter on a highlighted row): feed the chosen completion back into Neovim's command line. Simplest correct approach — replace the current completion token: send keys that clear the partial word and insert the selected item (e.g. drive Neovim's own completion by sending the text delta, or set the command line via `nvim_input`/`nvim_feedkeys` of the remaining characters). Exact keystroke recipe to be pinned during implementation (see Open questions).
- Esc closes the command line (already a plain key to Neovim, which emits `cmdline_hide`).

GTK (`crates/wolf-gnome`): the same, using the picker's `ListView`.

Both front-ends keep the fallback: if `Snapshot.cmdline` is `None`, no command line UI is shown.

### 5. Data flow

```
key ':'  -> send_key -> nvim opens cmdline
  nvim -> cmdline_show -> redraw fold -> Snapshot.cmdline = Some{ firstc: ":", content: "" } -> wake
    front-end: show command-line field
key 'e',' ','F','i' -> send_key -> nvim updates cmdline
  nvim -> cmdline_show (content "e Fi") -> Snapshot.cmdline updated -> wake
    front-end: cmdline_completions("e Fi") -> ["File A.md","File B.md"] -> render menu
Tab/Enter on a row -> front-end commits chosen item into nvim cmdline
Enter (execute) / Esc -> nvim -> cmdline_hide -> Snapshot.cmdline = None -> wake
    front-end: hide field + menu
```

## Testing

Per CLAUDE.md, prefer real-Neovim integration tests over mocks.

- **Core, real nvim:** a test in `crates/wolf-core` (or the `wolf-ffi` real-nvim suite, e.g. `bridge_smoke`) that creates temp files `File A.md` / `File B.md` / `Something else.md`, `:cd`s into the temp dir, and asserts `cmdline_completions("e File")` returns all three and `cmdline_completions("e Fi")` returns exactly the two `File *` entries. This is the load-bearing test — it proves the round-trip.
- **Core, ext_cmdline fold:** drive `:` + `e Fi` via `send_key` against a real nvim and assert `Snapshot.cmdline` reflects `firstc == ":"`, `content == "e Fi"`, and clears on `<Esc>`.
- **FFI:** extend the macOS real-nvim FFI test to call `cmdline_completions` across the bridge and assert the same lists.
- **Front-end:** menu navigation/commit is compile-verified + manual smoke (same status as the picker's AppKit wiring today).

## Scope and phasing

1. **Prototype (prove the round-trip):** `cmdline_completions` in core + the real-nvim test. No UI. Confirms `getcompletion("...","cmdline")` returns what we expect for `:e`.
2. **Signal:** add `ext_cmdline`, the `Snapshot.cmdline` fold, and its test. Point the existing cmdline strip (or a new top field) at `Snapshot.cmdline`.
3. **Menu:** wire the picker overlay to `cmdline_completions` on each change; render live.
4. **Commit:** pin the keystroke recipe for accepting a selection; add Tab/Enter/Esc handling.

Each step is independently testable and independently useful.

## Open questions

- **Commit recipe.** Exactly which keys to send to insert a chosen completion and keep Neovim's cmdline state consistent (send the remaining characters vs. drive Neovim's own wildmenu vs. `setcmdline()`). Neovim's `setcmdline()` (0.9+) can set the command-line text and cursor directly and is the cleanest candidate; verify it against the pinned 0.12.3 during step 4.
- **Fuzzy vs. prefix.** `getcompletion()` gives Neovim's prefix/glob semantics. If we want the picker's fuzzy narrowing on the command line too, layer `wolf_core::rank_matches` on top. Deferred.
- **Full native endpoint.** Whether to eventually move to `ext_popupmenu` (Neovim-owned menu state) once the native command line is fully realized (decision 7). The hybrid here does not block that — it is a superset of the signal that path needs.
- **`architecture.md` decision 7.** When this lands, update decision 7 and the "Deferred / open" list: `ext_cmdline` will be partially realized (command line + completions), while `ext_messages` / `ext_popupmenu` remain deferred.
