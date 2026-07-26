# Fix: confirm() prompt text lingers after the dialog is answered

## Problem statement

Steps to reproduce:

1. In the embedded Neovim, run `:lua vim.fn.confirm('Choose', '&A\n&B')`.
2. Type `A` to choose the first option.

Actual behavior:

- The presented options (`[A] (B)`) go away, but the prompt (`Choose`) sticks around.

Expected behavior:

- The prompt AND the options should stay visible until the user chooses something.
- After the user chooses something, the prompt should go away too.

## Root cause (already investigated via a real pinned-nvim event trace)

A Neovim `confirm()` dialog under `ext_messages` + `ext_cmdline` is TWO coupled
surfaces:

- A `msg_show` event with `kind = "confirm"` carrying the prompt text
  (`"Choose"`). This lands in `Snapshot.toast` as a `Message` whose kind is
  `"confirm"` (so `Toast::is_prompt()` returns true).
- A `cmdline_show` event carrying `prompt = "[A], (B): "` (the options line).
  This lands in `Snapshot.cmdline`.

Observed event sequence (pinned nvim 0.12.3), abbreviated:

```
cmdline_show ( ":call confirm(...)" )   # the typed ex command
cmdline_hide                            # command executed
msg_show   kind="confirm" content="Choose" id=1     # -> toast (the prompt)
cmdline_show prompt="[A], (B): "         # -> cmdline (the options)
flush                                    # dialog fully shown

# user types 'A' (a valid accelerator):
cmdline_hide                             # options line disappears
flush
# NOTE: NO msg_clear is ever emitted for the confirm message.
```

Because Neovim never emits `msg_clear` when the confirm dialog resolves, the
`"confirm"` toast message (`"Choose"`) is left in `Snapshot.toast` forever. The
options vanish (via `cmdline_hide`) but the prompt text lingers — exactly the
reported bug.

Note the existing `Toast::is_prompt()` gate in the input pump
(`api.rs`, OD8) deliberately keeps confirm toasts alive across passing input, so
the toast is NOT cleared by the answer key either.

## CORRECTION: the naive "clear on cmdline_hide" fix is INSUFFICIENT

The first coder pass revealed (and a full flush-level real-nvim trace confirmed)
that clearing the toast unconditionally on `cmdline_hide` when `is_prompt()`
BREAKS the existing test `confirm_prompt_is_not_dismissed_by_input`. Reason: an
INVALID keypress (e.g. `z`) produces a `cmdline_hide` batch that is
BYTE-IDENTICAL to the valid-answer `cmdline_hide` batch. The only difference is
temporal — on an invalid key, Neovim RE-PROMPTS by emitting a matching
`cmdline_show` in a LATER, separate flush; on a valid answer, nothing follows.

Definitive flush-level trace (`confirm("Really do it?", "&yes\n&no")`):

```
# INVALID 'z':
BATCH: cmdline_hide[1,false]  mode_change normal  flush     # toast would clear here
BATCH: cmdline_show prompt="[y]es, (n)o: "  mode cmdline_normal  flush   # RE-PROMPT

# VALID 'y':
BATCH: cmdline_hide[1,false]  mode_change normal  flush     # nothing follows
```

There is NO synchronous, in-frame signal distinguishing resolve from re-prompt.

## Revised fix: stash-on-hide, restore-on-matching-reprompt

Add an internal `Snapshot` field `pending_confirm: Option<PendingConfirm>` where
`PendingConfirm { messages: Vec<Message>, prompt: String }`.

- In `cmdline_hide`: capture `snap.cmdline.prompt` BEFORE resetting the cmdline.
  Then reset the cmdline as today. If `snap.toast.is_prompt()`, MOVE the toast
  messages into `pending_confirm` (with the captured prompt), and set
  `toast.visible = false` (so a genuine resolve shows cleared immediately).
- In `cmdline_show`: after parsing `firstc`/`prompt`, if `pending_confirm` is
  `Some`, `take()` it: restore the stashed messages (set `toast.visible = true`)
  IFF the incoming cmdline is prompt-style AND its prompt matches the stashed
  prompt (`firstc.is_empty() && !prompt.is_empty() && prompt == pending.prompt`)
  — i.e. the confirm dialog re-prompted after an invalid key. Otherwise DROP the
  stash (the confirm resolved; this cmdline is unrelated, e.g. a fresh
  `input()`), leaving the toast cleared.
- Also DROP `pending_confirm` at the top of `msg_show` / `msg_history_show` /
  `msg_clear` (a new message supersedes any stashed confirm), for safety.

### Why prompt-matching (not plain restore)

Without prompt-matching, a `confirm()` immediately followed by an unrelated
`input("Name: ")` would restore the stale "Really do it?" toast on `input()`'s
prompt-style `cmdline_show`. Matching the exact options-line prompt string means
only the confirm's own identical re-prompt restores it; a different prompt drops
the stash. (An immediately-repeated IDENTICAL confirm is harmless — it emits its
own `msg_show`, which drops the stash first.)

### Tradeoff accepted

On an INVALID key there is a one-flush window where the toast is hidden then
restored. The two batches arrive back-to-back (sub-frame), so this is
effectively imperceptible; the alternative (deferring the clear) cannot fire on
a valid answer because Neovim sends no follow-up event, reintroducing the
original bug.

### Original (insufficient) proposed fix, for the record

Clear the toast in `cmdline_hide` when `Toast::is_prompt()`. Rejected — breaks
the invalid-key case above.

Target: `fn cmdline_hide` in `crates/wolf-core/src/redraw.rs` (currently just
resets `snap.cmdline = Cmdline::default()`).

This is safe because `confirm()` is modal: while a confirm dialog is waiting for
an answer, no other cmdline can be active, so a `cmdline_hide` observed while a
confirm toast is visible always means that confirm dialog resolved (whether the
user chose an option or aborted with `<Esc>`).

## Finalized plan (architect-confirmed, revised design)

### Production changes

**1. `crates/wolf-core/src/render.rs`** — new internal type + field, and update
`clear_toast`.

- Define, right after the `Toast` impl block (before `Snapshot`):

  ```rust
  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub(crate) struct PendingConfirm {
      pub(crate) messages: Vec<Message>,
      pub(crate) prompt: String,
  }
  ```
  (with rustdoc explaining it bridges the one-flush hide→re-prompt window and is
  never rendered / never crossed over FFI.)

- Add to `Snapshot` (near `pub toast: Toast,`), as `pub(crate)` NOT `pub`:
  `pub(crate) pending_confirm: Option<PendingConfirm>,`

- In `clear_toast()` add `self.pending_confirm = None;` (a dismiss/clear
  supersedes any stashed confirm). This also covers `msg_clear` and the OD8
  input-pump dismiss.

FFI SAFETY (architect-verified): `Snapshot` is projected, not bridged — nothing
in `crates/wolf-ffi/src/lib.rs` enumerates Snapshot fields (`FfiToast::from_toast`
reads only `Toast`; `FfiRenderFrame::from_snapshot*` read named fields). A new
internal sibling field needs NO FFI wiring and cannot break packing. `Snapshot`
derives Clone/Debug/Default and there is NO exhaustive `Snapshot { .. }` literal
anywhere, so the field is safe to add.

**2. `crates/wolf-core/src/redraw.rs`** — import `PendingConfirm`; edit four fns.

- `fn cmdline_hide`: capture `let prompt = snap.cmdline.prompt.clone();` BEFORE
  `snap.cmdline = Cmdline::default();`. Then, if `snap.toast.is_prompt()`:
  `let messages = std::mem::take(&mut snap.toast.messages); snap.toast.visible =
  false; snap.pending_confirm = Some(PendingConfirm { messages, prompt });`

- `fn cmdline_show`: AFTER the existing `snap.cmdline = Cmdline { .. }` assignment,
  append (reading firstc/prompt back from `snap.cmdline`):
  ```rust
  if let Some(pending) = snap.pending_confirm.take() {
      let is_reprompt = snap.cmdline.firstc.is_empty()
          && !snap.cmdline.prompt.is_empty()
          && snap.cmdline.prompt == pending.prompt;
      if is_reprompt {
          snap.toast.messages = pending.messages;
          snap.toast.visible = true;
      }
      // else: `pending` drops — stash discarded, toast stays cleared.
  }
  ```

- `fn msg_show`: add `snap.pending_confirm = None;` as the FIRST statement.
- `fn msg_history_show`: add `snap.pending_confirm = None;` as the FIRST statement.
- `fn msg_clear`: NO explicit change (delegates to `clear_toast`, already resets).

SETUP-batch ordering (architect-verified fine): in `[cmdline_show(":call
confirm"), cmdline_hide, msg_show(confirm), cmdline_show(options)]`, at the
`cmdline_hide` the confirm `msg_show` hasn't run yet → `is_prompt()` false → no
stash. Then `msg_show(confirm)` resets `pending_confirm` (None, harmless) and
builds the toast. Then `cmdline_show(options)` sees `pending_confirm == None` → no
spurious restore. Correct.

### Edge cases handled

- `<Esc>` abort / valid answer: `cmdline_hide` stashes + hides; nothing follows →
  toast stays cleared. Bug fixed.
- Invalid key: stash + hide (one-flush blink), then matching re-prompt
  `cmdline_show` restores. `confirm_prompt_is_not_dismissed_by_input` still
  passes (its `sleep(SETTLE)` elapses past both batches; asserts final restored
  state).
- Fresh unrelated `input("Name: ")` after a valid answer: prompt mismatch → stash
  dropped, no stale restore. A new `echomsg`/message also drops the stash.
- OD8 input-pump gate (api.rs:327-333) unchanged/complementary; `clear_toast`
  reset also tidies a stash lingering after answer+movement.

### Tests

**Inline unit tests (`redraw.rs` `mod tests`):**

1. Already written, re-verified PASS: `cmdline_hide_clears_a_confirm_prompt_toast`
   (after hide: `!visible`, `!is_prompt`, `messages.is_empty()` — messages moved
   into the stash, invisible to these asserts) and
   `cmdline_hide_leaves_a_non_prompt_toast_intact`.
2. NEW — add a `cmdline_show_params(prompt)` helper next to `msg_show_params`
   (tuple order `[content, pos, firstc, prompt, indent, level]`: empty content,
   pos 0, firstc "", the prompt, indent 0, level 1). Then:
   - `cmdline_show_matching_reprompt_restores_stashed_confirm`: cmdline_show a
     prompt "[y]es, (n)o: ", msg_show(confirm "Choose"), cmdline_hide (assert
     `!visible` and `pending_confirm.is_some()`), cmdline_show the SAME prompt →
     assert `visible`, `is_prompt()`, `pending_confirm.is_none()`, message text
     "Choose".
   - `cmdline_show_nonmatching_prompt_drops_stashed_confirm` (staleness guard):
     same setup + hide, then cmdline_show a DIFFERENT prompt "Name: " → assert
     `!visible`, `!is_prompt()`, `pending_confirm.is_none()`, `messages.is_empty()`.

**Real-nvim tests (`tests/messages.rs`):**

3. `confirm_prompt_cleared_when_answered` (headline regression guard): drive
   `confirm("Really do it?", "&yes\n&no")`, `poll_until(is_prompt())`, send VALID
   `ch('y')`, `poll_until(!toast.visible)`, assert `!toast.visible` AND
   `!toast.is_prompt()`. Reuse `send_ex`, `poll_until`, `ch`, `key`,
   `start_core_sized`. No trailing `<Esc>`.
4. `confirm_answer_then_message_does_not_resurrect_prompt` (staleness, robust via
   the `msg_show`-drop path — less timing-sensitive than driving `input()`):
   answer the confirm as in #3, then `send_ex(&core, r#"echomsg "after confirm
   zzz""#)`, `poll_until` the toast shows "after confirm zzz" and `!is_prompt()`,
   assert `toast_text` contains "after confirm zzz", `!is_prompt()`, and does NOT
   contain "Really do it?". (Driving `input()` directly is feasible but delicate;
   the cmdline-mismatch path is already covered deterministically by the unit
   test above — the `input()` real-nvim test is OPTIONAL, only if reliable.)

### Existing tests that must still pass

- `confirm_prompt_is_not_dismissed_by_input` (re-verified PASS under new design).
- All other `messages.rs` and inline `redraw::tests::msg_show_*` tests.

### Documentation to update (technical-writer)

- `crates/wolf-core/src/render.rs` — rustdoc on `PendingConfirm`, the
  `pending_confirm` field, and the updated `clear_toast` doc (now also resets it).
- `crates/wolf-core/src/README.md` — `cmdline_hide` bullet (stash-on-hide),
  `msg_show`/`msg_clear`/`msg_history_show` fold bullet (drop the stash), a short
  paragraph on restore-on-matching-reprompt in `cmdline_show` and WHY
  (byte-identical hide batches, distinguished only temporally), and the Snapshot
  field list note that `pending_confirm` is internal (not rendered, not FFI).
- `crates/wolf-core/tests/README.md` — sentences for
  `confirm_prompt_cleared_when_answered` and
  `confirm_answer_then_message_does_not_resurrect_prompt`, plus the inline
  restore/drop unit coverage.
- `crates/wolf-ffi/src/lib.rs` — NO change (verified).
- Reformat edited Markdown with `prettier --write`.

## Status

- [x] Architect plan
- [x] Tests written
- [x] Implementation + tests passing (wolf_core_lib_test + messages both PASS; clippy clean)
- [x] Review — APPROVED (correctness, FFI safety, tests all verified; only docs outstanding)
- [x] Docs updated (src/README.md + tests/README.md; prettier clean)

## DONE
