# Plan: bound the synchronous nvim RPCs with a timeout

## Problem

The macOS app sporadically beach-balls: after a while, invoking a command (open a file, preview) hangs the whole UI until force-quit. We reproduced it deterministically by wedging the captive Neovim (`:lua while true do end`) and captured a symbolicated hang report.

The main-thread stack is unambiguous:

```
@objc AppDelegate.handleCommandMenuItem(_:)
 -> CommandDispatcher.dispatch(...)
  -> WolfWindowController.openFilePicker()
   -> PickerView.open()
    -> WolfCoreHandle::start_file_search            (FFI boundary)
     -> wolf_core::api::WolfCore::start_file_search
      -> wolf_core::api::WolfCore::resolve_search_root
       -> tokio Runtime::block_on
        -> parking_lot Condvar::wait -> __psynch_cvwait   (parked 19/19 samples, ~24s)
```

The captive nvim was simultaneously pegged at 100% CPU inside `ex_lua -> lua_pcall` (the busy loop), so it could not service the RPC.

## Root cause

`WolfCoreHandle` is main-thread-only by design (`WolfCore` is `!Send`), so every bridge method runs on the AppKit main thread. Several of those methods issue a synchronous `runtime.block_on(self.nvim.<rpc>())` against the captive nvim. These RPCs have **no timeout** (the only `tokio::time::timeout` in core is on the shutdown path, `nvim.rs`). When nvim is slow or wedged and never replies, `block_on` parks the main thread forever, which macOS renders as the spinner.

The reproduced trigger was `WolfCore::resolve_search_root` (`api.rs:946`), reached from the file picker. Note it is reached via `start_file_search`, whose FFI surface is documented "infallible across the bridge" and was **not** advertised as a blocking method — so the blocking surface is broader than the methods whose docs say "blocks briefly on the captive nvim." Any bridge method reaching a `runtime.block_on(self.nvim.…)` is exposed. This is why the fix is a single central chokepoint rather than a per-method patch.

## Goal and non-goals

Goal: convert an unbounded main-thread hang into a bounded stall that degrades to each method's existing fallback, and emit a log line naming which RPC timed out (which will also surface the root cause of the real, non-synthetic 30-minute hang the next time it fires).

Non-goals (explicitly out of scope for this change):

- Making the calls non-blocking / moving them off the main thread. A timeout bounds the worst case (infinite -> `RPC_TIMEOUT`); it does not make the picker/preview open instantly while nvim is busy. Moving the hot paths to a background queue with results delivered via the existing wake -> `DispatchQueue.main` path is a larger, separate follow-up.
- The startup `block_on` (`api.rs:251`) and `shutdown` (`api.rs:933`): startup already fails through `WolfCore::start`'s error path, and shutdown already has `SHUTDOWN_TIMEOUT`.
- The loopback `TcpListener::bind` in `browser_preview.rs:130`: a local socket bind, not an nvim RPC, so it cannot wedge on nvim.

## Design

### 1. Add a `Timeout` variant to `NvimError`

In `crates/wolf-core/src/nvim.rs`:

```rust
#[error("nvim RPC '{0}' timed out after {1:?}")]
Timeout(&'static str, Duration),
```

Every current call site already matches `Ok(_) => …, Err(_) => <fallback>`, so a new `Err` variant needs no per-site branch change — the timeout falls straight into the existing fallback arm.

### 2. Add a central `block_on_rpc` helper on `WolfCore`

In `crates/wolf-core/src/api.rs`:

```rust
const RPC_TIMEOUT_DEFAULT: Duration = Duration::from_secs(5);

fn block_on_rpc<F, T>(&self, what: &'static str, fut: F) -> Result<T, NvimError>
where
    F: std::future::Future<Output = Result<T, NvimError>>,
{
    match self.runtime.block_on(tokio::time::timeout(self.rpc_timeout, fut)) {
        Ok(inner) => inner,
        Err(_elapsed) => {
            tracing::warn!("nvim RPC '{what}' timed out after {:?}", self.rpc_timeout);
            Err(NvimError::Timeout(what, self.rpc_timeout))
        }
    }
}
```

The timer is available because the runtime is built with `.enable_all()`. `self.rpc_timeout` is resolved once at `WolfCore::start` from an env override (`WOLF_RPC_TIMEOUT_MS`) falling back to `RPC_TIMEOUT_DEFAULT` — this lets the integration test drive a small timeout deterministically and gives operators an escape hatch. Store it as a `Duration` field on `WolfCore`.

### 3. Route the interactive RPC sites through the helper

Replace `self.runtime.block_on(self.nvim.X())` with `self.block_on_rpc("X", self.nvim.X())` at each interactive site. The existing `Ok`/`Err` handling stays; the timeout maps onto the method's documented fallback:

| Site (`api.rs`) | Method | RPC | Fallback on timeout |
| --- | --- | --- | --- |
| 947 | `resolve_search_root` | `current_dir` (getcwd) | configured `self.cwd` (the repro path) |
| 377 | `copy_visual_selection` | `yank_visual_selection` | `None` |
| 414 | `list_buffers` | `list_buffers` | empty `Vec` |
| 437 | `render_preview` | `render_preview` | empty `String` |
| 471 | `open_in_browser` | `current_markdown_bufnr` | empty `String` (whole cmd) |
| 484 | `open_in_browser` | `buffer_path` | empty `String` (whole cmd) |
| 486 | `open_in_browser` | `current_dir` | already `.ok()` -> `None` root |
| 539 | `open_preview` | `open_preview` | `0` |
| 565 | `edit_source` | `edit_source` | `0` |
| 587 | `preview_file` | `preview_file` | `0` |
| 630 | `create_tabpage` | `create_tabpage` | `0` |
| 654 | `cwd_for_tabpage` | `tab_cwd` | `None` |
| 672 | `current_tabpage` | `current_tabpage` | `0` |

`start_file_search` (the reproduced path) is covered transitively by fixing `resolve_search_root`.

## Tests

Per the repo's test policy (real nvim, no mocks for nvim-communication behavior), two layers:

1. **Unit test of the wrapper** (fast, deterministic): call `block_on_rpc` with a future that never resolves (`std::future::pending()` or a long `tokio::time::sleep`) under a short `rpc_timeout`; assert it returns `Err(NvimError::Timeout(..))` within a small bound of the configured timeout. Pins the mechanism without a live nvim.

2. **Integration test, real nvim** (new `crates/wolf-core/tests/rpc_timeout.rs`): start a `WolfCore` with `WOLF_RPC_TIMEOUT_MS` set small (e.g. 200 ms), wedge the captive nvim by feeding `:lua vim.uv.sleep(<3x timeout>)<CR>` through the input pump (the same key-feeding path `commands.rs` uses for `:lua require('wolf')…`), then call an interactive method (e.g. `start_file_search` -> `resolve_search_root`, and at least one of `open_preview` / `current_tabpage`). Assert it returns its fallback within ~`RPC_TIMEOUT` rather than hanging, and record elapsed to prove the bound. Use a **finite** `vim.uv.sleep` (not `while true do end`) so nvim recovers and teardown/`shutdown` do not block the test.

## Docs to update

- `crates/wolf-ffi/README.md`: note that bridge methods reaching nvim are bounded by `RPC_TIMEOUT` and return their documented fallback on timeout; correct the impression that `start_file_search` is entirely non-blocking (it resolves the search root via a bounded `getcwd()` RPC).
- `crates/wolf-core/src/README.md`: document `block_on_rpc`, `RPC_TIMEOUT_DEFAULT`, the `WOLF_RPC_TIMEOUT_MS` override, and the `NvimError::Timeout` variant.

Reformat any edited Markdown with `prettier --write`.

## Risks and tradeoffs

- A timeout bounds the hang to `RPC_TIMEOUT` and degrades gracefully; it does **not** make a busy-nvim command feel instant. A up-to-5s stall is still a stall. If that is unacceptable for the hot picker/preview paths, the off-main-thread follow-up is the real fix. This plan is the safety net and the diagnostic (the warn log names the wedged RPC).
- Timeout value choice: too short cuts off a legitimately slow-but-alive nvim (e.g. a large `render_preview`); too long keeps the UI dead longer. Default proposed: 5s (matches `SHUTDOWN_TIMEOUT`), tunable via `WOLF_RPC_TIMEOUT_MS`. Confirm the value during orchestration.
- Fallbacks are already the shut-down-core behavior for each method, so a timeout produces a state the UI already handles (empty frame / no-op / configured cwd) — low blast radius.

## Blocking-input prompts (swap files, confirm dialogs, hit-enter)

A related but distinct failure mode reaches the same "nvim is not answering" state without a CPU busy loop: nvim prompts that block the main loop waiting for a keystroke — the swap-file ATTENTION prompt (E325), `hit-enter` prompts, `:confirm` / `confirm()` dialogs, `getchar()`, `input()`. While nvim is parked in such a prompt it defers normal work, so the UI stalls until the prompt is answered — and if the front-end cannot show or answer it, the user is stuck.

Observed behavior today (via `:lua vim.fn.confirm('Choose one', '&A\n&B')` in the embedded nvim): it is NOT a hard hang — `do_dialog` pumps the event loop, so e.g. the `⌘O` picker still opens — but the experience is broken:

- The dialog body ("Choose one") shows as a message above the command line, but the `[A], (B):` choice/answer line is not visible, so the user has no idea nvim is waiting for a keystroke.
- Actions taken meanwhile appear to do nothing: after picking a file in the picker, the buffer does not render until `Esc` is pressed (which cancels the dialog and unblocks nvim, after which the queued open applies).

So this is worse-than-terminal UX rather than a beach ball, but it shares a root with the hang: an nvim main loop waiting on input the front-end does not surface. The goal is a Neovim-ish (not necessarily macOS-native) experience: show the whole prompt and let the user answer it inline.

### 1. `SwapExists` autocmd — make swap prompts impossible (root-cause fix for the swap case)

Wolf's captive nvim already sets `swapfile = false` (`runtime/nvim/init.lua:25`), which is why the swap prompt does not normally appear from the embedded editor — with `'swapfile'` off, nvim neither creates swap files nor runs the "already exists" check on a normal open. The one real-world sighting was almost certainly a stale swap file in the shared global dir (`~/.local/state/nvim/swap`, left by a terminal nvim or a crash) hit under an edge condition where the check still ran (a modeline/`exrc`/plugin momentarily re-enabling `swapfile`, or a recover path).

Add a `SwapExists` autocmd to `init.lua` that sets `vim.v.swapchoice = "e"` (edit anyway) so a stale swap **never** blocks — the file just opens. Wolf is hermetic and does not manage recovery, so "edit anyway, ignore the stale swap" is the correct policy; this makes the swap prompt impossible rather than merely bounded. Test: create a stale swap file for a fixture (with `set swapfile` in a throwaway nvim, or by hand), open that file through the captive nvim, and assert it opens without a pending `"confirm"` toast and without blocking.

### 2. RPC timeout — the general backstop (see the main plan above)

The timeout from the main plan is the catch-all for this whole class: any OTHER prompt a plugin or command raises still parks nvim, and any front-end RPC issued during a HARD block (e.g. a busy-loop, or a prompt path that does not pump the loop) will hang. The timeout converts that from an infinite hang into a bounded stall plus a `tracing::warn!` naming the wedged RPC. No new work beyond the main plan — this subsection just records that prompts are one of the real-world triggers the timeout is defending against.

### 3. Render the cmdline `prompt` field (the actual fix for the UX)

Diagnosis is settled — probed with a real `confirm('Choose one', '&A\n&B')` driven through the core. A dialog arrives over TWO channels, and core captures BOTH:

- The dialog body (`Choose one`) → `ext_messages` `msg_show` kind `"confirm"` → `Snapshot.toast`. Already rendered by `ToastView`.
- The choice/answer line (`[A], (B): `) → `ext_cmdline` `cmdline_show` → `Snapshot.cmdline`, as the `prompt` field, with `active: true`, `firstc: ""`, `content: [""]`. Observed snapshot: `Cmdline { active: true, firstc: "", prompt: "[A], (B): ", content: [""], pos: 0, level: 1 }`.

So this is NOT a toast-truncation bug and NOT a core-capture gap — the `prompt` string is present in `Snapshot.cmdline` and already crosses the bridge (`FfiCmdline::prompt()`). The gap is purely in the macOS renderer: `CmdlineView` (`crates/wolf-macos/Sources/WolfMacOS/CmdlineView.swift`) reads only `firstc()` + `text()` in `update()` (lines 74-75) and draws `let line = firstc + text` in `draw()` (line 99), so it never reads or paints `prompt()`. With `firstc` and `text` both empty, the cmdline band is shown (`isHidden = !active` -> visible) but blank — exactly the observed "`Choose one` shows, `[A], (B):` does not."

The fix is small and front-end-only:

- **Render `prompt`.** In `CmdlineView.update()`, read `cmdline.prompt()`; in `draw()`, prepend it to the drawn line (`prompt + firstc + text`), and include its width in the caret-offset math (the `prefix` computed at line 106). No core, FFI, or toast changes.
- **Answering already works.** Keys flow via `nvim_input`; `Esc` already cancels and a single-key `A`/`B`/`o`/`e` already answers once the prompt is visible. The remaining task is presentation, not input plumbing — confirm the answering key is not swallowed and the prompt clears on answer.
- **Scope.** Neovim-ish is acceptable: the prompt text painted inline in the existing cmdline band, not a native `NSAlert`.

Test: a Swift `CmdlineViewTests` unit test that feeds an `FfiCmdline`-shaped value with `active=true`, empty `firstc`/`text`, non-empty `prompt` and asserts the rendered line contains the prompt (the pure-view analog of the existing seam tests). Optionally a core-side assertion (already true today) that `confirm(...)` yields `cmdline.prompt == "[A], (B): "` to pin the contract the view depends on.

## Execution

Run through `/orchestrate` (architect plan -> tests first -> implement -> verify against a real nvim -> reviewer -> technical-writer for the READMEs), since this touches core behavior and the FFI contract docs. The three efforts are separable and can land in order: (a) the RPC timeout, (b) the `SwapExists` autocmd, (c) the inline prompt render/answer. (a) and (b) are small and high-value; (c) is a small feature.

## Open decisions

1. `RPC_TIMEOUT_DEFAULT` value (proposed 5s).
2. Whether to also add the env-override escape hatch now (proposed yes — it is what makes the integration test deterministic).
3. Whether to schedule the off-main-thread follow-up for the hot picker/preview paths in the same effort or as a separate one (proposed separate).
4. `SwapExists` policy: `"e"` (edit anyway, proposed) vs `"o"` (open read-only). Proposed `"e"` since Wolf is hermetic and does not manage recovery.
5. Whether the cmdline-`prompt` render (section 3) ships with the crash fix or as a separate follow-up (proposed separate — it is a small front-end fix, not a hang fix). Note: the probe showed no `FfiToast::is_prompt()` accessor is needed for it — the choice line comes from `Snapshot.cmdline.prompt` (already exposed via `FfiCmdline::prompt()`), not the toast.
