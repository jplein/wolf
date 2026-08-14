# Unit: composed and input-method text entry for `wolf-gnome`

Branch: `linux-feat/input-method` (cut from `linux`).

## Problem

`wolf-gnome` attaches no `gtk::IMContext`. Verified by grep (no occurrence of
`IMContext`/`im_context`/`IMMulticontext` anywhere under `crates/wolf-gnome/src`)
and by measurement: `gdk::Key::dead_acute.to_unicode()` is `None`, so
`input::gdk_key` returns `None` for it and `window::wire_keys` drops the press.
A dead key, a compose sequence and every input method therefore reach Neovim as
nothing. This is the Linux side of the "macOS Option key composes accents" row.

## Measured evidence gathered before planning

All of it from a throwaway `crates/wolf-gnome/examples/im_probe.rs` run under
`cage` and driven with `wtype` — the same harness the smokes use. **The probe is
deleted before the commit.** The probe attached a `gtk::IMMulticontext` to a
bubble-phase `EventControllerKey` on the window, alongside a capture-phase
controller modelling `shell::install_chord_controller`, and called
`im.filter_keypress()` by hand from inside `key-pressed` (rather than through
`EventControllerKey::set_im_context`).

1. `wtype -k dead_acute` and `wtype -k Multi_key` both work under `cage` with no
   xkb or compose-key configuration in the compositor. `dead_acute` then `e`
   commits `"é"`; `Multi_key` then `a` then `e` commits `"æ"`; `Multi_key s s`
   commits `"ß"`. GTK's own compose handling is client-side, so the harness needs
   nothing added to it.
2. The capture-phase controller sees **every** key first, including `dead_acute`
   and `Multi_key`. The accelerator namespace is untouched by anything below.
3. With no compose sequence pending, the IM filters plain printables only:
   `a`, `A`, `j`, `é` (keyval `eacute`), `ß` (keyval `ssharp`) all arrive as
   `commit` and emit no `key-pressed`. `Ctrl+w`, `Ctrl+Shift+O`, `Escape`,
   `BackSpace`, `Alt+j`, `Left`, `Return` are **not** filtered and still reach
   `key-pressed`.
4. Calling `im.filter_keypress(controller.current_event())` by hand from inside
   `key-pressed` behaves identically to `set_im_context`, and puts the ordering
   in this crate's code rather than in GTK's controller.
5. **The preedit string is empty for every sequence GTK's own compose table
   handles.** `preedit-start` / `preedit-changed` / `preedit-end` all fire around
   a dead key and around a `Multi_key` sequence, and `preedit_string()` returns
   `""` with cursor 0 at every step. Nothing is drawable.
6. While a compose sequence is pending, a `Ctrl+<letter>` press **is** filtered
   and swallowed, and the pending sequence survives it. `Escape` while pending is
   also filtered, and aborts the sequence with no commit.

## Scope decision, made on that evidence

**In:** committed text. Dead keys, compose sequences, `AltGr` levels, and any
input method that commits.

**Out:** preedit display, and `set_cursor_location` with it. Finding 5 is the
ground: GTK's compose path reports an empty preedit string, so a preedit renderer
would draw nothing for every sequence this host or this harness can produce. The
only thing that would exercise it is an ibus or fcitx engine, which cannot run
under `cage` — `cage_command` deliberately removes `DBUS_SESSION_BUS_ADDRESS`,
and the GTK 4 ibus module is not in the flake. Shipping an unexercised paint path
over the grid is what `DECISIONS.md`'s "The pure seams" forbids. This is stated
as a present-tense limitation in `DECISIONS.md` and in `feat-list.md`, not left
for a user to find.

## Constraints this unit must not break

- The accelerator namespace invariant ("no Neovim key becomes unreachable"), held
  by `actions::parse_accel` / `forbidden_reason` over `ACTION_TABLE`.
- The Shift-glyph contract: `shift: false` on every `Key::Char`.
- The overlay guard in `window::wire_keys`: while the picker, palette, buffer
  picker or project search is open, nothing is forwarded to core.
- Pure rulebook, thin glue: the precedence decision lives in a GTK-free seam with
  unit tests, following `nav_ui.rs` and `tabs_ui.rs`.

## Plan

(architect's plan lands here)
# Implementation plan: composed and input-method text entry for `wolf-gnome`

## 1. Summary

`wolf-gnome` attaches no `gtk::IMContext`, so `input::gdk_key` returns `None` for `dead_acute` and `Multi_key` and `window::wire_keys` drops the press: dead keys, compose sequences, AltGr levels and every input method reach Neovim as nothing. This unit attaches one `gtk::IMMulticontext` per window, routes each window-level key press through it before the `gdk_key` translation, and turns each `commit` string into `wolf_core::KeyEvent`s. The precedence (overlay beats input method beats translation) and the commit rulebook (one `Key::Char` per `char`, `shift: false`, control characters dropped, a commit arriving behind an open overlay refused) live in a new pure seam, `crates/wolf-gnome/src/im_ui.rs`, unit-tested the way `nav_ui.rs` is; `window.rs` holds only the glue. Preedit display is out of scope, on the measured ground that GTK's compose path reports an empty preedit string for every sequence this host or harness can produce.

**The most important consequence of the measurements, and the thing that shapes the whole risk profile:** measurement 3 says the input method filters *plain printables* — `a`, `A`, `j` — with no sequence pending. After this change, every ordinary keystroke, including every normal-mode command key and every character of a typed `:e` command line, travels the `commit` path rather than the `gdk_key` path. This is not an accent feature bolted beside typing; it is a rerouting of typing. That is why the existing smoke suite is the regression evidence, and why the commit rule must be exactly right about ordering, control characters and `InputEvent::Key`-versus-`Paste`.

---

## 2. Files to create

### 2.1 `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/src/im_ui.rs`

The name: keep `im_ui.rs`. Its content is a precedence-and-refusal rulebook consumed by widget glue, which is the `nav_ui.rs` / `tabs_ui.rs` / `clipboard_ui.rs` species; `input.rs` and `mouse.rs` are the *translation* species (platform event → core value), and `commit_keys` is subordinate to the routing rule rather than the file's subject. `im_ui.rs` sorts next to `input.rs`'s neighbours in `src/README.md` either way.

Pure, GTK-free, `wolf_core`-free except for `Key` and `KeyEvent`. No `WolfCore`, no widget type, no display.

**Module doc comment** (nav_ui.rs's shape — sections, not a wall):

- What it owns: the precedence for one window-level key press, and the rule turning one `commit` string into zero or more `KeyEvent`s.
- **Why the ordering is a value rather than prose.** Offering a key to the input method is not a query: `filter_keypress` has a side effect, since a `dead_acute` it accepts is *swallowed into a pending compose state* that commits on a later key. So a key offered to the IM while an overlay is open cannot be un-offered; the overlay guard must short-circuit ahead of the offer, and that is what the enum makes executable.
- **Why the seam cannot be one call.** The IM's answer is not known until it has been asked, and asking is the side effect above. Hence the `Option<bool>` staging described below: the glue asks once with `None`, and only an answer of `AskInputMethod` licenses it to call `filter_keypress` and ask again.
- **Why a control character must never become a `Key::Char`.** `wolf_core::key_to_notation` renders a modifier-free `Key::Char(c)` as the bare character (`key_base` returns `c.to_string()`), so `Key::Char('\u{1b}')` sends a raw ESC byte through `nvim_input` and leaves insert mode; `'\r'` and `'\t'` are Enter and Tab by the same route. Those keys already have a route — `input::gdk_key`'s `named_key` maps `Return`/`Escape`/`Tab` to `Key::Enter`/`Key::Esc`/`Key::Tab`, which `key_to_notation` renders as `<CR>`/`<Esc>`/`<Tab>` and which measurement 3 says the IM does not filter. A control character in a commit would be a second, unnamed route to the same effect with no `<lt>`-style escaping and no modifier handling. `gdk_key` already refuses control characters at its own entry point; this is the identical rule at the second entry point, and two entry points that disagree is the bug.
- **The Shift-glyph contract**, cross-referenced to `input.rs` rather than restated: the committed text already carries the glyph, so every emitted `KeyEvent` sets `shift: false` — and `ctrl`, `alt`, `meta` false too, because a commit is a *result*, not a chord.
- **Totality is the contract**, pointing at the two `_`-free matches below.

**Public API:**

```
pub enum KeyPhase { Press, Release }
```
Derive `Debug, Clone, Copy, PartialEq, Eq`. Two variants only. Doc: which of the two `gtk::EventControllerKey` signals is being routed.

```
pub enum KeyRoute { Overlay, AskInputMethod, Consumed, Translate, Ignore }
```
Derive `Debug, Clone, Copy, PartialEq, Eq`. Per-variant docs stating the glue's obligation:
- `Overlay` — forward nothing, offer nothing, `glib::Propagation::Proceed`.
- `AskInputMethod` — call `filter_keypress` with the controller's current event, then re-ask with `Some(answer)`.
- `Consumed` — the IM claimed it; `Propagation::Stop`, translate nothing, count nothing.
- `Translate` — hand the keyval and modifier state to `crate::gdk_key` (press only).
- `Ignore` — a release the IM declined: `Propagation::Proceed` and nothing else. **Never `Translate`**: translating a release sends a second copy of every key.

```
#[must_use]
pub fn key_route(phase: KeyPhase, overlay_open: bool, im_filtered: Option<bool>) -> KeyRoute
```
Body shape, which is itself the precedence and must be written in this order:

1. `if overlay_open { return KeyRoute::Overlay; }` — the early return *is* the statement that an overlay wins, and it holds whatever `im_filtered` says, so a caller that asked the IM anyway is told to discard the answer.
2. `match im_filtered` with **no `_` arm**: `None => AskInputMethod`, `Some(true) => Consumed`, `Some(false) => match phase { Press => Translate, Release => Ignore }` — the inner match over `KeyPhase` carries **no `_` arm** either, so a future variant is an `E0004` here until classified. Document both, the way `nav_ui::nav_press` documents its `_`-free match.

```
#[must_use]
pub fn commit_keys(text: &str) -> Vec<KeyEvent>
```
One `KeyEvent { key: Key::Char(c), ctrl: false, shift: false, alt: false, meta: false }` per `char` of `text`, in order, skipping every `c.is_control()`. Empty input yields an empty vec. Public (not private) so its rule can be asserted on its own, following `nav_ui::nav_query_win`'s precedent.

```
pub enum CommitDecision { Send(Vec<KeyEvent>), Refuse(&'static str) }
```
Derive `Debug, Clone, PartialEq, Eq` (no `Copy` — it holds a `Vec`). `Refuse`'s payload is a mid-sentence clause, the `nav_ui::NavPress::Refuse` convention.

```
#[must_use]
pub fn commit_decision(overlay_open: bool, text: &str) -> CommitDecision
```
`Refuse` when an overlay is open, whatever the text; otherwise `Send(commit_keys(text))`, which may be an empty vec.

**Why an empty commit is `Send(vec![])` and not a refusal**, stated in the doc: a refusal reason is printed by the caller, and an empty or control-only commit is a routine event rather than a hazard. Only the overlay case is a hazard worth a printed line.

**Why the refusal arm is not dead code**, stated in the doc: `commit` fires synchronously from inside `filter_keypress`, which the glue calls only when no overlay is open — so *this glue* cannot reach it. An out-of-process input method (ibus, fcitx) commits asynchronously, on a bus reply, at an arbitrary main-loop turn, exactly as `gdk::Clipboard::read_text_async` resolves on a later turn. The guard is for that, and the plan should say so rather than leave a reader to delete it as unreachable.

### 2.2 `/home/jplein/dev/jplein/wolf/crates/wolf-gnome/tests/im_commit_nvim.rs`

A new file rather than new cases in `roundtrip_nvim.rs`. Justification: `roundtrip_nvim.rs`'s stated subject, in its `tests/README.md` entry, is one sentence — "the full input seam: `gdk_key` to `InputEvent::Key` to `WolfCore` to snapshot … the shift-glyph contract end to end". A commit case is a different subject with a different failure diagnosis (a wrong glyph, a reordered word, a fired Escape — not a missing word), and folding it in would require that entry to be rewritten to cover two claims, which is what makes a red run ambiguous. This crate is Cargo-only with no Bazel target (`tests/README.md` line 3), so `cargo test` auto-discovers the file and CLAUDE.md's wiring obligation is discharged by that fact — but the `tests/README.md` entry is mandatory. Cost is one more test binary spawning one more Neovim, roughly a second.

### 2.3 No other new files.

---

## 3. Files to modify

### 3.1 `crates/wolf-gnome/src/lib.rs`

- Add `pub mod im_ui;` to the module list, alphabetically between `frame_stats` and `init_lua`… in fact between `glyph_style` and `init_lua`.
- Add a bullet to the crate-level `//!` seam inventory, in the established voice, naming `im_ui::key_route`, `im_ui::commit_keys`, `im_ui::commit_decision`, `KeyRoute`, `KeyPhase`, `CommitDecision`, and stating in one clause that the precedence is a value because offering a key to the IM is a side effect.
- **No re-export** in the `pub use` block. Nothing outside the crate needs these (the tier-2 test uses `wolf_gnome::im_ui::…`, which works because the module is `pub`), and the block's stated rule is that a re-export exists for a concrete consumer (`cell_at`, `restart_button_rect`).

### 3.2 `crates/wolf-gnome/src/window.rs`

**`wire_keys` — signature.** Change to return the context:

`fn wire_keys(...) -> gtk::IMMulticontext` — same six parameters, no new ones. The context is constructed inside because nothing above needs to supply it, and this keeps the parameter count at six, below `clippy::too_many_arguments`.

**`wire_keys` — body, in this order:**

1. `let im = gtk::IMMulticontext::new();`
2. `im.set_client_widget(Some(window));` — before `focus_in`, so the context has a surface when it becomes focused.
3. `im.connect_commit(...)` — the closure captures clones of `core` and the four overlay handles, plus nothing else. Body:
   - Compute `overlay_open` from the same four `is_open()` reads the press guard uses.
   - `match im_ui::commit_decision(overlay_open, text)`:
     - `Refuse(reason)` → one `eprintln!("wolf-gnome: commit not forwarded: {reason}")`, following `nav_ui`'s printed-refusal convention, and return.
     - `Send(events)` → `if let Some(core) = core.borrow().as_ref() { for ev in events { core.send_input(InputEvent::Key(ev)); frame_metrics::note_keystroke(); } }`.
4. `im.focus_in();`
5. `controller.connect_key_pressed(...)` — the closure now binds `controller` (not `_controller`) and captures `im.clone()`. Body:
   - `let overlay_open = picker.is_open() || palette.is_open() || buffers.is_open() || search.is_open();`
   - `match im_ui::key_route(KeyPhase::Press, overlay_open, None)`:
     - `Overlay` → `Propagation::Proceed`.
     - `AskInputMethod` → `let filtered = controller.current_event().is_some_and(|ev| im.filter_keypress(&ev));` then re-enter with `im_ui::key_route(KeyPhase::Press, overlay_open, Some(filtered))`.
     - the other three arms are unreachable on the first ask and must be handled by the same `match` as the second ask — write it as a small helper closure or a second `match` on the re-ask result, but **do not** collapse the two asks into one.
   - On `Translate` → the existing `crate::gdk_key(keyval, state)` block, unchanged, including `note_keystroke()`.
   - On `Consumed` → `Propagation::Stop`, no translation, no counter.
   - On `Ignore` → `Propagation::Proceed` (unreachable on a press; the match stays total).
   - **`current_event()` answering `None` must be treated as "the IM declined" (`false`), never as consumed.** A missing event is a GTK anomaly; falling through to `gdk_key` loses at worst the composition, while treating it as consumed loses the keystroke outright. Write the reason in a comment.
6. `controller.connect_key_released(...)` — new. Same overlay guard, `KeyPhase::Release`, `Consumed` → `Stop`, everything else → `Proceed`, and **no call to `gdk_key` on any path**.
7. `window.add_controller(controller);`
8. `im`

**Why forward releases at all** (write this into the doc comment): the whole design rests on measurement 4, that a hand-rolled `filter_keypress` from inside `key-pressed` behaves identically to `EventControllerKey::set_im_context`. `set_im_context` forwards *both* press and release; a press-only forward silently diverges from the thing the measurement licensed. Measurement 6 records that the IM does claim some events while a sequence is pending. Nothing else in this crate consumes key releases — `install_chord_controller` connects `key_pressed` only — so the forward costs nothing today and closes a class of wedged-state-machine failures that would be silent.

**Why not `EventControllerKey::set_im_context`** (a "do not simplify" note): GTK would then offer the key to the IM before this crate's overlay guard ran, moving the ordering out of this crate's code and making the overlay-versus-IM precedence unstatable and untestable. Measurement 4 is what makes the hand-rolled call free.

**`wire_keys` doc comment** — rewrite. The sentence "Otherwise every key is translated via the pure [`crate::gdk_key`] seam into core input" becomes false and must be replaced, not appended to: every press that survives the overlay guard is offered to this window's `gtk::IMMulticontext` first, and only a press the IM declines is translated. Keep the existing capture-phase paragraph verbatim — it is still true and still load-bearing.

**`build`** — line 804 becomes `let im = wire_keys(&window, &core, &picker, &palette, &buffers, &search);` and the `CloseParts` literal gains `im: &im`.

**`CloseParts`** — add `im: &'a gtk::IMMulticontext`, with a field doc naming what the handler does with it.

**`wire_close`** — clone the context into the closure and, as the **last** statement of the handler, after the core has been taken and shut down:
```
im.focus_out();
im.set_client_widget(None::<&gtk::Widget>);
```
Two reasons, both written down. First, `gtk_im_context_set_client_widget` stores the widget in the context, which the controller closures own and which the window owns — a cycle that keeps a closed window's whole widget tree alive, and this application closes windows routinely. Second, `focus_out` on a context with a pending sequence may commit it; running the teardown **after** `core.borrow_mut().take()` means such a commit finds `None` and performs nothing, rather than pushing into a core that is being shut down. The ordering *within* the handler is not load-bearing beyond that, but the existing rule — `coordinator.on_window_closing(id)` is the first statement — is untouched and must stay first.

**Module doc comment** — the input paragraph, if present, gains one clause. Do not restate `im_ui`'s rules here.

### 3.3 `crates/wolf-gnome/src/shell.rs`

**Untouched. Deliberately, and this is a claim rather than an omission.**

`install_chord_controller` sets `PropagationPhase::Capture` on its controller and the IM lives on `window.rs`'s phase-less (bubble) controller. GTK runs every capture-phase controller root→target before any bubble-phase controller runs, and measurement 2 confirms the capture controller sees every key first, `dead_acute` and `Multi_key` included. So a chord this crate binds *and enables* is matched and `Propagation::Stop`ped before the input method is ever offered the key: the accelerator namespace keeps first refusal by construction, not by cooperation. Attaching the IM to the chord controller, or moving `wire_keys`' controller into the capture phase, would invert that and swallow every bound chord — worth a sentence in `wire_keys`' doc comment so nobody "unifies" the two controllers.

### 3.4 `crates/wolf-gnome/DECISIONS.md`

Four edits. Sentences quoted exactly.

**(a) "Window structure", the input paragraph.** This becomes false as written:

> Input has two regimes. Keys are global: a capture-phase controller on the window matches this crate's own chords and stops propagation, and a bubble-phase controller routes everything else through the key seam into Neovim's current window, so a bound chord is never also forwarded.

Rewrite the middle clause: the bubble-phase controller offers everything else to the window's own input method and routes only what the input method declines through the key seam. Keep "so a bound chord is never also forwarded" — still true, and now for a second reason worth one clause: a bound chord never reaches the input method either.

**(b) "The accelerator namespace" — one sentence becomes conditionally false, and it is the sharpest finding of this review.**

> One consequence is worth knowing: a disabled action's accelerator falls through to Neovim, which is what a real GTK accelerator does and what the chord controller deliberately reproduces, so in the empty state `Ctrl+Shift+O` reaches Neovim as `<C-o>`, `Ctrl+Shift+F` as `<C-f>` and `Ctrl+Shift+B` as `<C-b>`.

and, carrying more weight:

> Plain Ctrl+C and Ctrl+V carry no shift, and `action_for_key` compares its three modifiers exactly, so neither matches a bound chord and both still reach Neovim.

Measurement 6 says that **while a compose sequence is pending, a `Ctrl+<letter>` press is filtered and swallowed**. A bound, enabled chord is immune (the capture controller took it first), but a *disabled* action's fall-through and every unbound `Ctrl+<letter>` — `<C-c>`, `<C-v>`, `<C-w>`, plain `Ctrl+P` — are not: between a dead-key press and the next key they reach the input method and are eaten. Both sentences need a qualifying clause naming that window, and the "Present limitations" bullet below must state it as a live limitation rather than burying it here.

Do **not** fix it in this unit. The obvious repair — never offer a press carrying `CONTROL_MASK` to the input method — is a one-arm change to `key_route` (it would take the modifier state as a fourth argument), but no measurement licenses it: nobody has measured that skipping the offer leaves the pending sequence intact and lets the chord through. Name it as a follow-up with the measurement it owes, and note that the seam's shape makes it a one-row flip with its own red run.

**(c) "Present limitations".** Delete this bullet, which becomes wrong:

> - No input-method composition support.

Replace it with two present-tense bullets:

- A composed sequence draws nothing while it is pending: GTK's own compose path reports an empty preedit string at every step (`preedit-start`, `preedit-changed` and `preedit-end` all fire and `preedit_string()` answers `""` with cursor 0), so a dead key and a `Multi_key` sequence are invisible until they commit, and this front-end sets no `set_cursor_location`. An engine that does produce a preedit string cannot run under the harness at all — `cage_command` removes `DBUS_SESSION_BUS_ADDRESS` and no GTK 4 ibus module is in the flake — so a preedit renderer would be an unexercised paint path over the grid, which "The pure seams" forbids.
- While a compose sequence is pending, a `Ctrl+<letter>` press is claimed by the input method and does not reach Neovim, and the sequence survives it. Bound, enabled chords are unaffected, because the capture-phase chord controller matches them first; what this costs is a disabled action's fall-through and every `Ctrl+<letter>` Neovim wants for itself, `<C-c>` and `<C-v>` included, inside the one-keystroke window between a dead key and the key that resolves it.
- A window's input method is focused in once at build and focused out at close, and nothing tracks window activation, so in a session with several windows every window's context reports itself focused. This crate wires no window-activation handler at all (it also sends no `InputEvent::SetUiFocus`); adding one is a unit of its own. GTK's built-in simple context is entirely client-side and per instance, so nothing crosses between windows today.

**(d) The `wakes/key` paragraph under "Frame-time instrumentation"** — no edit needed, and say so in the commit message rather than in the file. See §7.

### 3.5 `crates/wolf-gnome/src/README.md`

**New `#### im_ui.rs` entry**, alphabetically between `glyph_style.rs` and `init_lua.rs`. Under 200 words. Purpose; Public API (the three functions and the three types, one line each); Decisions — the early-return-is-the-precedence rule, the `_`-free matches, why control characters are dropped (one clause, pointing at `key_to_notation`), why an empty commit is not a refusal; Gotchas — the overlay refusal exists for an asynchronous out-of-process commit, not for this glue.

**Edit `#### input.rs` in place.** Its Purpose stays true. Add one Gotcha: a printable key normally reaches Neovim through the input method's `commit` rather than through this function, since the IM filters plain printables with no sequence pending; `gdk_key` is what handles the named keys, the modifier chords and anything the IM declines. Do not restate `im_ui`'s rules — link.

**Edit `#### window.rs`'s Gotcha in place.** This becomes incomplete:

> - The key controller carries no propagation phase, so it runs after [`shell.rs`](#shellrs)'s capture-phase chord controller, which stops a bound chord also reaching Neovim.

Rewrite it to add that the same controller offers each press to the window's `gtk::IMMulticontext` before translating, and that the capture-phase controller's first refusal is what keeps a bound chord out of the IM as well as out of Neovim. Add one Decision bullet: the close handler focuses the context out and clears its client widget as its last statement, after the core is taken, so a teardown-time commit finds no core and so the context does not hold the window alive.

**`#### shell.rs`** — no edit. Its Decisions are all still true.

### 3.6 `crates/wolf-gnome/tests/README.md`

- **New `#### im_commit_nvim.rs` entry** with Covers and Gotchas (see §4.2 for the content).
- **Edit `#### smoke_headless.rs` in place.** The counts move: "Fifty-eight end-to-end smokes … thirty-seven OCR a `grim` screenshot and twenty-one call no OCR helper at all" becomes fifty-nine / thirty-seven / twenty-two, and "the target reports `67 passed`, not `58`" becomes `68 passed`, not `59`. Grep the whole file and the `smoke_headless.rs` module doc comment for every count phrase rather than editing the two you happen to remember; the module comment carries several ("fifty-six", "Five of the fifty-six read RAW PIXELS", the smoke-family enumeration at the top).
- **"What is still unproven"** gains three entries, each in the established voice:
  - The overlay guard on the commit path. `commit_decision`'s `Refuse` arm is unreachable from this glue, since GTK's built-in context commits synchronously inside `filter_keypress` and that call is made only with no overlay open. Reaching it needs an out-of-process engine, which the harness cannot run. Table-tested only — the same shape as the existing `window::wire_keys`' buffer-picker guard entry.
  - A commit arriving after the close handler has taken the core out. Same reason; the `if let Some(core)` arm is executed by nothing.
  - The release forward. Nothing observes that the input method is ever offered a release, or that one it claims is stopped: measurement 6 is the only evidence, taken from a deleted probe, and no compose sequence in the smoke straddles a release the IM claims in a way any observable distinguishes. What a *wrong* release path would do is loudly visible (§8, hazard 3), so the gap is that the right one is unproven, not that a wrong one would hide.
- **Optionally** add a sentence to "Smoke conventions" only if the compose smoke needs a new convention — it does not; it is an ordinary member of the non-OCR readback family the SAVE smokes' block already describes.

### 3.7 `crates/wolf-gnome/tests/smoke_headless.rs`

See §4.3 for the smoke. Beyond it:

- **`cage_command`** gains `.env("GTK_IM_MODULE", SMOKE_GTK_IM_MODULE)` with `const SMOKE_GTK_IM_MODULE: &str = "gtk-im-context-simple";`. See §6.
- **`cage_command`'s doc comment**: "the five environment variables EVERY smoke must have" → six, and a sixth bullet.
- **`cage_is_only_spawned_through_the_isolating_helper`'s doc comment**: "would silently lose ALL FIVE variables" → six; "which is the only thing that sets `WOLF_STATE_DIR`, `HOME` and `XDG_CACHE_HOME`" gains the new name; add a sixth numbered cost paragraph. Its **assertion message** enumerates the variables too ("without `WOLF_STATE_DIR`, `WOLF_CONFIG_DIR`, `HOME` or `XDG_CACHE_HOME`, and WITH whatever `DBUS_SESSION_BUS_ADDRESS`…") and must gain the new name with its cost clause.
- **`packaged_cage_command`'s doc comment**: "it carries `WOLF_STATE_DIR`, `WOLF_CONFIG_DIR`, `HOME`, `XDG_CACHE_HOME` and the `DBUS_SESSION_BUS_ADDRESS` removal. None of those five is a devShell export and none of them may be scrubbed" → six, with `GTK_IM_MODULE` classified the same way `WOLF_FONT_PT` is: harness isolation, set by `cage_command` rather than by the `shellHook`, and therefore never scrubbed.
- **`RELAUNCH_FIXTURE_NAMES`** gains the compose smoke's fixture name and its two readback names, per `CD_READBACK_NAME`'s stated reason (their absolute paths are echoed into the typed `:call writefile(...)` command line). Run `restore_fixture_names_cannot_satisfy_their_own_assertions` after adding them; if it carries a count or an exemption-block count, update it rather than guessing.

### 3.8 `feat-list.md`

- Row 49, `macOS Option key composes accents (option-as-option)`: the Linux box `[ ]` → `[x]`, and the branch column gains `linux-feat/input-method`.
- Delete item 2 of "Suggested order of work for Linux" ("**Composed and input-method text entry.** …") and renumber items 3 onward. Its premise — "this front-end attaches no `gtk::IMContext` at all" — becomes false.
- If the surrounding prose names the divergences that remain, add one sentence naming the two limitations from §3.4(c) rather than claiming parity.

### 3.9 Not touched, and each with its reason

- **`architecture.md`** — untouched. Its input contract ("The front-end pushes `InputEvent`s onto an ordered FIFO channel … fuses a run of consecutive `Key` events into one `nvim_input` call") is exactly what a multi-character commit uses, unchanged. Apply CLAUDE.md's test: delete `wolf-gnome` and the IM precedence rule vanishes with it — it is a crate-local constraint, so it belongs in `crates/wolf-gnome/DECISIONS.md`. The counter-argument, that "translate OS input into core's abstract input events" is a cross-crate boundary sentence, is already satisfied by the existing text: nothing at the boundary changes shape.
- **`build.md`** — untouched. `GTK_IM_MODULE` is set inside `cage_command`, so no developer types it and no workflow changes. Argued the other way: build.md owns "environment", and this is an environment variable — but the variable belongs to one test target's harness, and what a given test target covers belongs in that directory's README, which is where the pin's reason goes.
- **`crates/wolf-gnome/README.md`** — untouched. Its "Four environment variables matter" sentence is about *running the application*, and this unit adds no variable the application reads.
- **`crates/wolf-gnome/src/frame_metrics.rs`** — untouched; see §7.

---

## 4. Test plan

### 4.1 Tier 1 — seam unit tests, in `src/im_ui.rs`'s `#[cfg(test)] mod tests`

Follow `nav_ui.rs`'s conventions exactly: domain constants at the top, a table with a row-count assertion written as **independent literals** (the `NAMED_BUTTON_CASES` precedent — never `CASES.len()` compared to a product of the same arrays), rules stated over the whole domain where a per-row failure would quote an index instead of naming the rule, and a non-vacuity assertion beside every implication.

Domain constants: `ALL_PHASES: [KeyPhase; 2]`, `ALL_OVERLAY: [bool; 2]`, `ALL_FILTER: [Option<bool>; 3]`, and `ROUTE_CASES: usize = 2 * 2 * 3` written as literals.

| # | Test | What it pins | Smallest source edit that falsifies it |
|---|---|---|---|
| 1 | `key_route_table` | All twelve `(phase, overlay_open, im_filtered)` rows, plus the count assertion | In `key_route`, change `Some(false) => match phase { Release => Ignore }` to `Translate` |
| 2 | `an_open_overlay_wins_over_the_input_method` | Sweep: `overlay_open == true` ⇒ `Overlay`, for every phase and every filter answer, including `Some(true)` and `Some(false)`. Non-vacuity: at least one input answers something else | Move the `if overlay_open` early return below the `match im_filtered`, i.e. make `(true, _, None)` answer `AskInputMethod` |
| 3 | `a_release_is_never_translated` | Sweep: no `Release` input answers `Translate`. Non-vacuity: a `Press` input does | The same `Release => Translate` edit as #1, but the failure message names the rule rather than a row |
| 4 | `the_input_method_is_asked_exactly_once_before_it_answers` | `AskInputMethod` is the answer for `im_filtered == None` and for nothing else, over the whole domain — so a glue loop cannot be told to ask twice | `None => Consumed` (or `Translate`), which would make the glue never call `filter_keypress` at all |
| 5 | `commit_keys_table` | Rows: `""` → empty; `"é"`, `"æ"`, `"ß"` → one event each carrying that exact char; a multi-character commit (`"the"`) → three events in source order; `"a\u{1b}b"` → two events, `a` then `b`; `"\n"`, `"\t"`, `"\u{1b}"`, `"\u{7f}"` → empty; `"<"` → one `Key::Char('<')` | Delete the `is_control` filter (the four control rows and the mixed row go red); `text.chars().rev()`; `text.chars().next()` (the multi-character row goes red) |
| 6 | `no_committed_key_carries_a_modifier` | Over every fixture string in #5's corpus, every emitted event has `ctrl`, `shift`, `alt`, `meta` all false. Message names the Shift-glyph contract and `wolf_core::KeyEvent`'s doc | `shift: true` in `commit_keys` |
| 7 | `a_committed_key_renders_as_itself_in_neovim_notation` | Cross-check against `wolf_core::key_to_notation`: for every event #5 produces, the notation is the character itself, except `'<'` which is `"<lt>"`; and the notation is never a bare control byte and never an angle-bracket named key | Delete the `is_control` filter — `"\u{1b}"` then produces the notation `"\u{1b}"`, a raw ESC into `nvim_input`. This test is the executable form of the "why" paragraph |
| 8 | `commit_decision_table` | `(overlay_open, text)` over the same corpus: `Refuse` for every text with an overlay open, including `""`; `Send` otherwise, carrying exactly `commit_keys(text)` | Drop the `overlay_open` parameter from the decision |
| 9 | `an_empty_commit_is_not_a_refusal` | `commit_decision(false, "")` is `Send` with an empty vec | Make empty text a `Refuse` — which would print a reason on a routine event |
| 10 | `the_refusal_reason_is_non_empty_and_reads_as_a_clause` | Non-empty, and not sentence-cased, using `nav_ui::tests::is_sentence_cased` re-declared locally (the file's own precedent — do not import across modules) | Empty the reason string |

Note for the coder: tests #2 and #3 are the two that must survive as *sweeps*, not tables — a table row failure quotes an index, and the rules these state are the two whose violation is catastrophic and easy to reintroduce.

### 4.2 Tier 2 — `crates/wolf-gnome/tests/im_commit_nvim.rs`

Shape: `mod common;` plus `common::{require_nvim_bin, config, wake_channel, FLUSH_TIMEOUT}`, a local `key_from_char` (the file-local helper the `tests/README.md` "The `key_from_char` helper" section describes — re-declare it, do not import), a `SETTLE` constant and the drain/sleep/drain treatment `roundtrip_nvim.rs`'s README entry requires of any case that reads a snapshot.

Every case routes through the seam: `im_ui::commit_decision(overlay_open, text)`, and on `Send(events)` sends each as `InputEvent::Key`. Insert mode is entered through `gdk_key(key_from_char('i'), ModifierType::empty())`, so the two seams compose exactly as `wire_keys` composes them.

| # | Test | Behaviour | Falsifying source edit |
|---|---|---|---|
| A | `a_committed_character_reaches_the_buffer` | Insert mode, commit `"é"`, Escape, settle; row 0 contains `"é"` | `commit_keys` returning an empty vec, or dropping non-ASCII |
| B | `a_multi_character_commit_arrives_in_order` | Insert mode, commit a multi-character string (`"ærø"`), Escape, settle; row 0 contains it **in that order** | `text.chars().rev()`, or `text.chars().next()` only |
| C | `a_control_character_in_a_commit_never_reaches_neovim` | Insert mode, commit `"a\u{1b}b"`, Escape, settle; row 0 is exactly `"ab"` (trimmed). This is the discriminating case: without the filter, the raw ESC leaves insert mode, the following `b` becomes a normal-mode word motion, and row 0 reads `"a"` | Delete the `is_control` filter in `commit_keys` |
| D | `a_commit_behind_an_open_overlay_sends_nothing` | Type a marker in insert mode, Escape; then `commit_decision(true, "é")` and send only on `Send`; settle; row 0 is unchanged and contains no `"é"` | Drop the `overlay_open` parameter from `commit_decision` |
| E | `a_normal_mode_commit_is_a_command_and_not_text` | Buffer with two known lines; in **normal** mode commit `"dd"` through the seam; settle; the first line is gone and the second is now row 0 | None in `im_ui` — this pins the *transport*. Recorded honestly in the README entry: what it discriminates is that the seam's output is delivered as `InputEvent::Key` rather than `InputEvent::Paste`, and the test's own loop is what chooses that. The glue's choice is covered by the smokes (§8), because after this unit every normal-mode key in every smoke travels the commit path |

`tests/README.md` entry — Covers: the commit rulebook against a real Neovim, including the control-character drop, whose failure mode is a fired Escape rather than a stray glyph. Gotchas: needs `WOLF_NVIM_BIN`, `common::require_nvim_bin` panics rather than skipping; leaving insert mode emits follow-up flushes so every case drains and sleeps a settle window; case E pins the transport rather than the seam, and its choice of `InputEvent::Key` is the test's own.

### 4.3 Tier 3 — one smoke in `crates/wolf-gnome/tests/smoke_headless.rs`

**Template: `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage` and its `SAVE_SCENARIO`.** That is the file's non-OCR readback family: keyboard-only, no `wolf-pointer-inject`, no `grim`, no `tesseract`, a `wait_file` poll for a file the captive Neovim writes, an exit-code vocabulary (11 for a dead app, a distinct code per missing readback), an assertion-0 precondition that the fixture's own bytes do not pre-satisfy the discriminator, and an assertion table in the doc comment with a `revert` and a `measured` column. Mirror all of it. `typed_cd_reroots_the_sidebar_under_cage`'s `CD_SCENARIO` is the secondary reference for the `wait_file` helper body and the `FILE_TRIES` knob.

**Name:** `composed_text_reaches_neovim_under_cage`.

**New constants:**
- `COMPOSE_FIXTURE_FILE` — e.g. `"cmpjuliet.md"`, deliberately unlike every asserted term.
- `COMPOSE_FIXTURE_HEADING` — e.g. `"# CMPFIXTUREHEAD"`, the file's whole content going in.
- `COMPOSE_EXPECTED: &str = "éæß"` — the exact line the three sequences must produce.
- `COMPOSE_BEFORE_NAME`, `COMPOSE_READBACK_NAME` — both under `XDG_RUNTIME_DIR`, both registered in `RELAUNCH_FIXTURE_NAMES`.
- `COMPOSE_STEP` inside the scenario — the pause between two keys of one compose sequence. A timing knob; no assertion depends on it.

**What the scenario shell script does**, step by step:

1. `set -eu`; `WAIT_TRIES=75`, `WAIT_INTERVAL=0.2`, `SETTLE=1.0`, `FILE_TRIES=100`, `COMPOSE_STEP=0.1` — the shared values, with `SETTLE`'s comment pointing back at `SCENARIO`'s canonical measured table.
2. `cleanup()` trap killing `app_pid`, installed before anything starts.
3. Launch `"$WOLF_BIN" &`; poll `kill -0` / `wtype ""` with `exit 11` on an early exit; `sleep "$SETTLE"`.
4. `wait_file()` helper, the `SAVE_SCENARIO` body verbatim.
5. `wtype -- ":e $COMPOSE_FILE"`, `wtype -k Return`, `sleep "$SETTLE"` — a named buffer with known content.
6. Precondition readback: `wtype -- ":call writefile([bufname('%')], '$BEFORE')"`, `wtype -k Return`, `wait_file "$BEFORE" || exit 24`. This proves keys reach Neovim at all, so a failure of the discriminator is attributable to the input method rather than to a lost keystroke — which matters more here than in any other smoke, because a broken commit path breaks *all* typing and every other symptom would be a red herring.
7. Park at the end and open a fresh empty line in insert mode: `wtype -- G`, `wtype -- o`, `sleep "$COMPOSE_STEP"`.
8. The three sequences, **one `wtype` invocation per key**, each followed by `sleep "$COMPOSE_STEP"` (each invocation uploads a fresh single-keycode keymap, so separate invocations are what keep the compose state machine's input ordered and unbatched):
   - `wtype -k dead_acute` then `wtype -- e` → `é`
   - `wtype -k Multi_key`, `wtype -- a`, `wtype -- e` → `æ`
   - `wtype -k Multi_key`, `wtype -- s`, `wtype -- s` → `ß`
9. `wtype -k Escape`; `sleep "$SETTLE"`.
10. The discriminator readback: `wtype -- ":call writefile([getline('$')], '$COMPOSE_READBACK')"`, `wtype -k Return`, `wait_file "$COMPOSE_READBACK" || exit 25`.
11. `exit 0`. **No `grim`, no `tesseract`, no `:w`, no cursor parking** — nothing here is a claim about pixels, and the scenario must contain no second route from the buffer to a file.

**Rust-side assertions:**

| # | Assertion | Kind | Revert |
|---|---|---|---|
| 0 | The fixture on disk contains none of `é`, `æ`, `ß` before the launch | precondition — the evidence is not pre-satisfied by the fixture | none |
| 1 | `status.success()`, with an exit-code decoder naming 11 (launch), 24 (no keys reach Neovim at all — check the keyboard-only smokes first) and 25 (the buffer readback never appeared) | precondition | none |
| 2 | `$BEFORE` names `COMPOSE_FIXTURE_FILE` | precondition — the `:e` took effect | none |
| 3 | the readback line **equals** `COMPOSE_EXPECTED` | **the discriminator** | delete the `im.filter_keypress(...)` call in `wire_keys`, so every press goes straight to `Translate`. `gdk_key` answers `None` for `dead_acute` and `Multi_key`, so the line reads `"eaess"` — a specific, unambiguous red |
| 4 | the readback line contains no ASCII character | corroboration — any ASCII means one compose member leaked through uncomposed, which distinguishes a partly-working compose table from a wholly-broken one | none of its own; green under assertion 3's revert only in the sense that 3 fires first |

Equality rather than `contains` in assertion 3 is load-bearing: it is what makes a doubled commit (`"ééææßß"`) and a reordered one visible, which a `contains` would not catch.

A second revert worth running and recording in the `measured` column: replace the `connect_commit` handler's send loop with `{}`. The readback then holds an empty line, assertion 3 reds with an empty `got`, and the pair of reverts distinguishes "the IM was never offered the key" from "the commit never reached core".

**Regression evidence — the smokes that constitute proof that ordinary typing, the bound chords, and Neovim's `<C-w>` chords still work.** These are not new tests; they are the existing ones that must stay green, and after this unit they are all exercising the commit path for the first time. Name them in the commit message and in the smoke's doc comment:

- **Ordinary typing:** `window_renders_typed_text_under_cage` (types `ihello` + Escape and OCRs `hello` — a doubled or dropped commit reads as `hheelllloo` or as an empty page); `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage` (types `zzsavedelta` and asserts those exact bytes on disk — the sharpest of the set, since it is an exact string rather than an OCR `contains`); `new_untitled_buffer_replaces_open_file_under_cage`; and, less directly, every smoke that types a `:e …` command line, which is all of them.
- **The bound chords:** `picker_opens_and_loads_selected_file_under_cage` (Ctrl+Shift+O), `command_palette_chord_runs_the_selected_command_under_cage` (Ctrl+Shift+P), `buffer_picker_chord_switches_to_the_queried_buffer_under_cage` and `buffer_picker_bare_enter_switches_to_the_previous_buffer_under_cage` (Ctrl+Shift+E), `project_search_chord_opens_the_matched_line_under_cage` (Ctrl+Shift+F), `sidebar_toggle_chord_hides_the_file_tree_under_cage` (Ctrl+Shift+B), `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage` (Ctrl+Shift+S), `native_tabs_open_switch_and_close_under_cage` (Ctrl+Shift+T), `copy_and_paste_round_trip_through_the_clipboard_under_cage` (Ctrl+Shift+C and Ctrl+Shift+V), `close_pane_chord_closes_focused_pane_under_cage` (Ctrl+Shift+W), `alt_left_navigates_back_to_the_previous_file_under_cage` (Alt+Left), `two_windows_two_neovims_and_two_saved_sessions_under_cage` (Ctrl+Shift+Q and Ctrl+Shift+N via the palette).
- **Neovim's `<C-w>` chords:** `vertical_split_shows_two_panes_under_cage` (`<C-w>v`), `horizontal_split_shows_two_panes_under_cage` (`<C-w>s`), `focus_move_returns_to_first_pane_under_cage` (`<C-w>v` then `<C-w>h`), `close_pane_leaves_one_under_cage` (`<C-w>c`), `three_panes_keep_all_files_visible_under_cage`, `terminal_drawer_opens_and_is_not_persisted_under_cage` and `terminal_drawer_defaults_to_full_width_under_cage` (`<C-w>k` / `<C-w>j`). Measurement 3 says a `Ctrl+w` press with nothing pending is not filtered, so these are the evidence that the measurement holds under the shipped glue rather than only under the probe.

---

## 5. Implementation order

1. `src/im_ui.rs` — the seam and its `#[cfg(test)] mod tests`. It depends on nothing but `wolf_core::{Key, KeyEvent}`, so it compiles and its tests run before any widget code changes. Run `cargo test -p wolf-gnome --lib im_ui` here.
2. `src/lib.rs` — `pub mod im_ui;` plus the crate-doc bullet. (Step 1 will not compile into the crate until this lands; do them together if the compiler complains about an unreferenced file.)
3. `tests/im_commit_nvim.rs` — the headless tier. It needs only step 1 and a real Neovim, so it lands and goes green before any GTK glue exists.
4. `src/window.rs` — `wire_keys`' new body and return type, `CloseParts`' new field, `wire_close`'s teardown, and `build`'s call site, in that order (the return type change forces the call site, which forces the struct field).
5. `tests/smoke_headless.rs` — `SMOKE_GTK_IM_MODULE` and the `cage_command` line first, then the three doc-comment and one assertion-message count updates, then the compose smoke's constants, scenario and test, then the `RELAUNCH_FIXTURE_NAMES` entries. Run `cage_is_only_spawned_through_the_isolating_helper` and `restore_fixture_names_cannot_satisfy_their_own_assertions` immediately after, since both are pure and fail in microseconds.
6. Run the smoke suite once whole (`cargo test -p wolf-gnome --test smoke_headless`) before touching any documentation — this is where a broken commit path shows up as a mass failure across the OCR smokes rather than as one red test, and it is the step that must be seen green.
7. Take the two red runs for the compose smoke's assertion 3 and record them in the `measured` column.
8. Documentation, last, in this order: `crates/wolf-gnome/DECISIONS.md`, `crates/wolf-gnome/src/README.md`, `crates/wolf-gnome/tests/README.md`, `feat-list.md`. Then `prettier --write` on each, then `./tools/lint/markdown_check.sh` on the four.

---

## 6. Dependencies, and the `GTK_IM_MODULE` question

**No new external packages.** `gtk::IMMulticontext`, `IMContextExt` (`set_client_widget`, `focus_in`, `focus_out`, `filter_keypress`, `connect_commit`) and `EventControllerExt::current_event` are all in the pinned `gtk4` 0.11.x, and `window.rs` already has `use adw::prelude::*;`, whose re-export of the gtk prelude carries every one of those traits — so no new `use` line either. No `flake.nix` change: nothing new is linked, and deliberately no GTK 4 ibus module is added.

**Pin `GTK_IM_MODULE` in `cage_command`, to `gtk-im-context-simple`. Yes.**

The reason is not tidiness. `gtk::IMMulticontext` selects its slave from `GtkSettings:gtk-im-module`, which GTK initializes from the `GTK_IM_MODULE` environment variable, and `cage_command` inherits the developer's environment wholesale. On this host that variable reads `ibus`, the GTK 4 ibus module is not installed, and GTK falls back to its built-in simple context — which is exactly what the probe measured. That fallback is a property of *this host*, not of the harness. Before this unit the variable did nothing, because no context was attached. After it, the variable decides which context every smoke's typing goes through, so on a host that does have the GTK 4 ibus module — a different NixOS configuration, or a future flake that pulls ibus in for some other reason — the entire suite's typing would silently reroute through an engine that cannot reach its daemon, since the harness removes `DBUS_SESSION_BUS_ADDRESS`. **This unit is what makes the smokes' typing host-dependent for the first time, and the pin is what takes that back.**

Value: `gtk-im-context-simple`, named explicitly, rather than removing the variable. Removing it lets GTK apply its own default selection, which on Wayland consults the compositor for `zwp_text_input_manager_v3` — a `cage`/wlroots-dependent branch that would make the choice depend on the compositor build rather than on anything this repo pins. `gtk-im-context-simple` names the context the probe measured, on every host and every compositor.

What the pin does *not* buy, and must be said in the doc comment rather than implied: it does not make the smokes evidence about any out-of-process input method. They cannot be — the harness removes the session bus and the flake ships no engine — and that is already recorded as the ground for the scope decision. The pin documents that constraint instead of leaving it to the ambient environment.

**Guard-test obligations.** `cage_is_only_spawned_through_the_isolating_helper` is the only guard that counts `cage_command`'s variables, and it counts them **in prose** (its doc comment's "ALL FIVE" and its five numbered cost paragraphs) and **in its assertion message**, not in an executable list — so nothing goes red automatically and all three must be edited by hand. `packaged_cage_command`'s doc comment carries a fourth "those five". No other guard enumerates them: `every_shellhook_export_is_scrubbed_or_deliberately_kept` and `the_shellhook_still_stages_the_vendored_lua_trees_for_the_ffi_tests` both scan `flake.nix`'s `linuxShell.shellHook` exports, and `GTK_IM_MODULE` is not one — it is set by the harness, like `WOLF_FONT_PT` — so those two are untouched and `PACKAGED_SCRUB_ALWAYS` must **not** gain it.

---

## 7. `frame_metrics::note_keystroke`, and the absent core

**Count a committed character.** One `note_keystroke()` per `InputEvent::Key` the commit handler actually sends, so a multi-character commit counts N.

The reason is the counter's own documented contract — `frame_metrics::note_keystroke`'s doc comment reads "Counts one key actually delivered to Neovim", and `wire_keys`' existing comment says it is counted "where the key is actually DELIVERED, so a key the overlay guard above swallowed and a key typed at a window with no core are both correctly absent from the denominator". A committed character *is* a key delivered to Neovim, and after this change it is how nearly every key is delivered. Not counting it would silently empty the denominator of `wakes/key` for ordinary typing.

Read against `DECISIONS.md`'s `wakes/key` paragraph — "an upper bound on the wakes attributable to typing, never an attribution … Mouse, scroll, resize and out-of-band core notifications all inflate it" — the two answers differ in kind. Leaving commits uncounted would still satisfy "upper bound" (an uncounted delivery only raises the ratio), which is precisely the trap: it would keep the stated property technically true while destroying the instrument, since under the harness there is no pointer and no resize and the ratio is meant to sit close to attribution. Counting keeps the invariant that the counter equals the number of `InputEvent::Key`s the window sent. **No documentation change is needed** — that paragraph and `frame_metrics.rs`'s doc comment both stay exactly true.

The one thing that is *not* counted, correctly: a `dead_acute` press the IM claims. It delivers nothing to core and produces no wake, so both sides of the ratio are untouched.

**An absent core.** `if let Some(core) = core.borrow().as_ref()` — the same shape `wire_keys`' press path already uses, with the borrow taken and released in one statement per `DECISIONS.md`'s rule. A commit that finds `None` performs nothing and prints nothing: the close handler has already taken the core out, and the window is tearing down. Deliberately **no liveness gate**: `send_input` is fire-and-forget on the ordered channel and blocks on nothing, so `nav_ui::nav_press`'s argument does not apply, and gating commits while the existing `gdk_key` path is ungated would make typing `e` and typing `é` behave differently at a dead core. This arm is reachable only through an asynchronous out-of-process commit, so it is unobservable here and goes in `tests/README.md`'s "What is still unproven".

---

## 8. Every place this could silently break ordinary typing, and what catches each

| # | Failure | Why it is reachable | What catches it |
|---|---|---|---|
| 1 | `connect_commit` never wired, or its handler sends nothing | Measurement 3: the IM filters plain printables, so **all** typing goes through `commit` — a missing handler is total input death, not a lost accent | Mass smoke failure. First and loudest: `window_renders_typed_text_under_cage`; `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage` reports it as an exact-bytes miss. Unit: `commit_decision`'s `Send` rows |
| 2 | The IM claims a press **and** the glue also translates it | Handling `Consumed` as a fall-through, or asking the IM after translating | `ctrl_shift_s_…` (the file would read `zzzzssaavveeddeellttaa`), `window_renders_typed_text_under_cage` (`hheelllloo` fails `contains("hello")`). Unit: `key_route_table`'s `(Press, false, Some(true)) => Consumed` row |
| 3 | A release routed to `gdk_key` | The new `connect_key_released` handler is the only place this can happen, and it is one line away | Same two smokes — every key doubles. Unit: `a_release_is_never_translated` |
| 4 | The overlay guard placed below the IM offer | A "simplification" that computes `filtered` first and then checks overlays | Weakly. The four overlay smokes very likely stay green, because a focused `gtk::Entry` consumes printables in the bubble phase before the window controller runs — the same hole `tests/README.md` already records for `window::wire_keys`' buffer-picker guard. Unit: `an_open_overlay_wins_over_the_input_method`. Record it as unproven, with that entry as precedent |
| 5 | The chord controller loses first refusal (the IM moved onto the capture controller, or `set_im_context` used) | Every bound chord swallowed | The eleven chord smokes in §4.3, all at once |
| 6 | `current_event()` answering `None` treated as consumed | A `?` or an `unwrap_or(true)` where `unwrap_or(false)` belongs | Nothing, if it never fires; total input loss, if it does. Closed by writing the rule and its reason at the call site — this is a "state it, do not test it" case, and it should be named in `window.rs`'s comment |
| 7 | A multi-character commit sent as `InputEvent::Paste` | It looks equivalent in insert mode | Every smoke that drives normal mode: `nvim_paste` in normal mode inserts text where `nvim_input` runs a command, and after this unit every normal-mode key travels the commit path, so `close_pane_leaves_one_under_cage`, the split smokes and every `:e` line break at once. Headless: case E |
| 8 | `GTK_IM_MODULE` inherited from the developer | A host with the GTK 4 ibus module reroutes every smoke's typing through an engine with no session bus | Nothing, without the pin. Closed by §6 |
| 9 | A commit arriving after the core is taken out | Only an asynchronous engine can produce it | Nothing. `if let Some(core)`; recorded as unproven |
| 10 | The IM begins filtering named keys (Escape, Return, arrows) | Would follow from a different slave context being selected | Every smoke — each drives `wtype -k Escape` and `wtype -k Return`. This is the other thing §6's pin protects |

---

## 9. Answers to the remaining questions

**Multi-character commits, and ordering against `send_input`.** Yes, the path must handle them, and not only in theory: an out-of-process engine commits whole words and phrases, and `gtk_im_context_reset` can commit a partial sequence in one string. The rule is one `InputEvent::Key` per `char`, pushed in `text.chars()` order in a single loop, with the core borrow taken once for the loop (matching `wire_keys`' existing shape, and safe because `send_input` is a channel push that cannot re-enter the close handler). Ordering is then free and already contracted: `wolf_core::input::apply_batch` fuses a run of consecutive `Key` events into one `nvim_input` call, so a two-character commit reaches Neovim as one call and one redraw flush — exactly what a burst of keystrokes already does, which is why `architecture.md` needs no edit. What must not happen is `InputEvent::Paste`: `nvim_paste` bypasses `nvim_input`, so a normal-mode commit of `dd` would insert the text instead of deleting a line, and it is one undo block that runs no mappings. Given measurement 3, normal-mode commits are the common case, not an exotic one.

**Per window, per tab, or per pane?** Per window, and the reasoning is structural rather than aesthetic. Per pane is not merely wrong but impossible: `DECISIONS.md`'s "GTK keyboard focus never lands on a pane area" means a pane could never be an IM client, and "Every widget under the pane area is destroyed and rebuilt on each layout change" means a per-pane context would be destroyed mid-sequence on any split or divider drag, dropping a pending compose state with nothing reporting it. Per tab is coherent but buys nothing and costs something: there is exactly one key controller per window and one core per window, so the routing decision has no per-tab input to read, while N contexts per window would give N compose states in a UI that shows one — and the tab switch would then need a `reset()` policy nothing measures. Per window matches the one thing that is already per window: the controller the IM is driven from. The honest cost is recorded as a limitation — with several windows open, each window's context reports itself focused in, because this crate wires no window-activation handler at all.

**Anything in `DECISIONS.md` or `src/README.md` that becomes false**, quoted exactly:

1. `DECISIONS.md`, "Present limitations": `- No input-method composition support.` — wholly wrong; deleted and replaced (§3.4c).
2. `DECISIONS.md`, "Window structure": `a bubble-phase controller routes everything else through the key seam into Neovim's current window` — false as written; the controller offers to the input method first (§3.4a).
3. `DECISIONS.md`, "The accelerator namespace": `so in the empty state \`Ctrl+Shift+O\` reaches Neovim as \`<C-o>\`, \`Ctrl+Shift+F\` as \`<C-f>\` and \`Ctrl+Shift+B\` as \`<C-b>\`` — conditionally false: not while a compose sequence is pending (§3.4b).
4. `DECISIONS.md`, "The accelerator namespace": `Plain Ctrl+C and Ctrl+V carry no shift, and \`action_for_key\` compares its three modifiers exactly, so neither matches a bound chord and both still reach Neovim.` — conditionally false for the same window, and this is the one that carries weight, since `<C-c>` interrupts and `<C-v>` is visual-block and insert-mode literal-next (§3.4b).
5. `src/README.md`, `window.rs` Gotcha: `The key controller carries no propagation phase, so it runs after \`shell.rs\`'s capture-phase chord controller, which stops a bound chord also reaching Neovim.` — still true but incomplete; rewritten in place (§3.5).
6. `src/README.md`, `input.rs`: nothing false, but the entry is now misleading by omission — a printable key normally reaches Neovim through `commit`, not through `gdk_key`. One Gotcha added (§3.5).
7. `window.rs`'s `wire_keys` doc comment (source, not a README): `Otherwise every key is translated via the pure \`crate::gdk_key\` seam into core input.` — false; rewritten (§3.2).
8. `feat-list.md`: `this front-end attaches no \`gtk::IMContext\` at all, so a dead key, a compose sequence and every input method reach Neovim as nothing` — false; that whole work item is deleted (§3.8).

`DECISIONS.md`'s "The accelerator namespace" invariant sentence itself — `**The invariant is that no Neovim key becomes unreachable.**` — stays true and is not weakened by this unit: the swallow in finding 6 is a transient state, not a permanently unreachable key, and it is recorded as a limitation with a named follow-up rather than as an exception to the invariant.

---

## 10. Lint considerations

No `#[allow]` directives should be needed anywhere, and the plan is arranged so none is. The gate is `cargo clippy --workspace --all-targets -- -D warnings` with default lints only (no pedantic), per `build.md`.

Two places where one would be tempting, and why each is avoided:

- `clippy::too_many_arguments` on `wire_keys`. It stands at six parameters and the default threshold is seven; constructing the `gtk::IMMulticontext` inside and *returning* it, rather than passing it in, keeps it at six. If a later change does push it over, follow the `ShellParts` / `CloseParts` / `crate::persistence::StateParts` precedent and take a struct — do not add an allow.
- `clippy::needless_pass_by_value` and `clippy::must_use_candidate` are pedantic and not enabled; add `#[must_use]` to `key_route`, `commit_keys` and `commit_decision` anyway, for consistency with every other seam in this crate.

Two mechanical items: derive `Debug, Clone, Copy, PartialEq, Eq` on `KeyPhase` and `KeyRoute` and `Debug, Clone, PartialEq, Eq` on `CommitDecision` (it holds a `Vec`, so no `Copy`) — the seam tests need `assert_eq!` and `matches!` over all three. And `--all-targets` means clippy lints the new `#[cfg(test)] mod tests` and `tests/im_commit_nvim.rs` too, so the usual test-code rules apply: no `unwrap()` where a message helps, and every assertion carries the failure message the house style expects.

---

# Orchestrator amendments to the plan above

These override the architect where they differ.

1. **Delete `crates/wolf-gnome/examples/im_probe.rs`.** It is the throwaway probe
   that produced the measurements at the top of this file. It is in the working
   tree right now and must not be committed. Deleting it is part of the coder's
   job, not a follow-up.
2. **Verify every count before editing it.** The architect's numbers for
   `smoke_headless.rs` ("fifty-eight", "sixty-seven passed", "five of the
   fifty-six") are read from the file, not computed. Grep for every count phrase
   in `tests/smoke_headless.rs` and `tests/README.md`, and re-derive each from the
   test binary's own reported pass count after the suite runs. Do not trust a
   number in this plan.
3. **Verify `packaged_cage_command` before editing it.** Check whether it delegates
   to `cage_command` or builds its own `Command`. If it delegates, it inherits
   `GTK_IM_MODULE` automatically and only its doc comment changes.
4. **The seam's file name is `src/im_ui.rs`.** Settled.
5. **The double-`match` shape in §3.2 step 5 is a description, not a prescription.**
   Implement the two-stage ask however reads best, provided the overlay guard
   short-circuits strictly before any `filter_keypress` call, and the routing
   comes from `im_ui::key_route` rather than from inline booleans.

## Orchestrator ruling on plan test #4 (§4.1 row 4)

The plan states `the_input_method_is_asked_exactly_once_before_it_answers` as a
biconditional over the whole domain — "`AskInputMethod` is the answer for
`im_filtered == None` and for nothing else". That is false against a correct
implementation and contradicts row 2, because `key_route(Press, true, None)`
answers `Overlay`: the overlay early return precedes the match, which is the very
thing row 2 demands.

**Assert two one-directional statements instead**, both over the whole domain:

1. Every input answering `AskInputMethod` has `im_filtered == None`.
2. Every input with `im_filtered == None` **and** `overlay_open == false` answers
   `AskInputMethod`.

Each needs its own non-vacuity assertion. The rule the test is protecting is that
the glue's two-stage ask cannot loop, and both directions above are needed to say
it.

---

# Outcome

Shipped on `linux-feat/input-method`. What differs from the plan above, and why:

1. **The release handler classifies but cannot act.** GTK 4's `key-released` is a
   void signal — `connect_key_released` takes no `Propagation` return — so plan
   §3.2 step 6's "`Consumed` → `Stop`" is not expressible. The handler makes the
   offer, which is the thing the measurement licensed, and its route match is a
   total `_`-free classification with no action in any arm.
2. **Plan test #4 was a biconditional that contradicted plan test #2.** Replaced
   by the two one-directional statements recorded above.
3. **Plan §4.2 case D would have been vacuous as written.** It attempted the
   refused commit in normal mode, where a leaked `é` changes no buffer. It is
   attempted in insert mode instead.
4. **The compose smoke's revert (b) does not produce the predicted empty line.**
   It reds one assertion earlier, at exit 24, because a commit handler that sends
   nothing breaks *all* typing — the `:e` and the `writefile` command lines
   included, so no readback is ever written. The pair still discriminates, by exit
   code rather than by content.
5. **`smoke_headless.rs`'s aggregate counts were already stale by two before this
   unit touched them**, and the two Save smokes were the missing pair. Repaired
   along with the enumerations that back them.
6. **One smoke beyond the plan.**
   `a_control_chord_while_composing_is_claimed_by_the_input_method_under_cage`.
   `DECISIONS.md` states the `Ctrl+<letter>`-while-composing limitation as
   measured, and that measurement came from the probe deleted at the top of this
   file — so the claim rested on nothing in the tree. It reproduces: control arm
   two panes, subject arm one.

Not built, and stated as present-tense limitations rather than left to be found:
preedit display and `set_cursor_location`; the `Ctrl+<letter>` swallow inside a
pending sequence, whose repair owes a measurement first.
