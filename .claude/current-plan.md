# Unit: Save and Save As for `wolf-gnome`

Branch: `linux-feat/save-dialogs` (cut from `linux`). Merge target: the `linux`
worktree at `.worktrees/linux`, via `--no-ff`. Do not push.

## Problem statement

`feat-list.md`'s "Navigation and file access" table carries one unticked Linux row,
`Native file dialogs (open / save / save-as)`. `wolf-gnome` has native Open File and
Open Folder; saving is reachable only as `:w` typed inside Neovim. This unit adds
Save and Save As so all three dialogs are genuinely reachable, then ticks that row,
rewrites the paragraph that explains the partial coverage, and removes the unit from
the "Suggested order of work for Linux" list (renumbering the survivors; item 1, the
preview-render coalesce, stays).

## Facts established before planning (orchestrator, measured on this host)

These are measurements, not assumptions. Do not re-derive them; do not contradict
them without a new measurement.

1. **`wolf-core` already carries the save primitives.** `InputEvent::Save` applies
   `write`; `InputEvent::SaveAs { path }` applies `saveas <fnameescape(path)>`
   (`crates/wolf-core/src/input.rs:445-448`). Both are fire-and-forget on the ordered
   input channel — **no blocking RPC**. `crates/wolf-core/tests/save_open.rs` already
   drives both against a real Neovim. No front-end has ever constructed either.

2. **There is no `Command::Save` / `Command::SaveAs` in the core registry, and this
   unit does not add one.** Adding a variant is a cross-platform change: the Swift
   `CommandKind` port (`crates/wolf-macos/Sources/WolfKeyMap/CommandSeam.swift`) and
   `CommandDispatcher.swift`'s `default`-free switch would both have to grow arms, and
   macOS has no save UI at all to point them at (measured: no `NSSavePanel` anywhere in
   the repo; the macOS File menu has no Save item; ⌘S reaches Neovim as an unmapped
   `<D-s>`). None of that can be compiled or verified on this Linux host. The
   consequence is that Save and Save As are **front-end-only rows** —
   `command: None`, resolved through `command_action::ui_action_for_front_end_action`,
   exactly as `win.about`, `win.history-back` and `win.history-forward` are — so they
   reach the user from the primary menu and an accelerator, and **not** from the command
   palette, whose rows come from `wolf_core::COMMAND_SPECS`. That gap is a documented
   present limitation and a decision for the human user; adding the core commands later
   is purely additive over this unit's work.

3. **An `Untitled N.md` buffer has a real path.** `runtime/nvim/init.lua`'s
   `wolf_name_untitled` names a fresh anonymous buffer `<cwd>/Untitled.md` (or
   `Untitled N.md`) without writing it, so `:w` on such a buffer succeeds and creates
   that file. "Never saved" therefore means "named but not on disk", not "unnamed".

4. **`gtk::FileDialog` cannot present under `cage` while the portal route is live, and
   the failure is worse than invisible.** Measured with a throwaway GTK probe:
   - `FileDialog::save()`/`open()` construct a `GtkFileChooserDialog` that stays
     `visible=false mapped=false realized=false` forever, and the callback never fires.
   - Root cause, read out of the GTK 4.22.4 source: `gdk_display_should_use_portal`
     answers true because GDBus reaches the developer's **real** session bus at
     `$XDG_RUNTIME_DIR/bus` even though the harness removes
     `DBUS_SESSION_BUS_ADDRESS`; `gtk_file_chooser_native_portal_show` then exports a
     window handle, `cage` implements no `zxdg_exporter_v2`, and GTK proceeds to
     `show_portal_file_chooser(self, NULL)`. The request goes to the host's real
     `org.freedesktop.portal.Desktop`, i.e. **a smoke that triggers a native dialog
     opens a modal file dialog on the developer's desktop** and hangs until it is
     answered or the app is killed.
   - `GDK_DEBUG=no-portals` forces GTK's in-process fallback. Measured under `cage`:
     the dialog presents (`visible=true mapped=true realized=true`, 881x326), `wtype`
     of a name plus `Return` returns `Ok(<cwd>/typedname.md)` to the callback. This is
     the lever that makes a Save As smoke possible, and it is per-smoke scenario
     environment, never production behaviour.
   - The existing Open File / Open Folder dialogs have exactly the same property, which
     is why no smoke drives them today.

5. **Accelerator namespace.** `Ctrl+Shift+S` is currently held, unbound, by the
   `win.sidebar-show-project` reserved row, whose command `wolf-gnome` classifies
   `Deferred` ("no panel switcher"). macOS spells Save ⌘S and Save As ⇧⌘S, which both
   translate to the same `Ctrl+Shift+S`, so at most one of the two can carry a chord —
   the `app.new-window` precedent (macOS ⇧⌘N, menu-only because `Ctrl+Shift+N` is
   spent).

6. **Verification commands available here:** `nix develop --command cargo test -p
   wolf-gnome`, `... -p wolf-core`, `cargo clippy --workspace --all-targets`. The
   `wolf-gnome` crate has **no** `BUILD.bazel` by design, so a new test file needs no
   Bazel wiring — but a new `wolf-core` test file would.

## Plan (architect, accepted by the orchestrator)

# Plan: Save and Save As for `wolf-gnome` (`linux-feat/save-dialogs`)

## 1. Summary

`wolf-gnome` gains **Save** (`Ctrl+Shift+S`, primary menu) and **Save As** (primary menu only), built as a new pure decision seam `crates/wolf-gnome/src/save_ui.rs` plus thin `gtk::FileDialog` glue in `shell.rs` shaped exactly like `open_file_dialog` / `open_folder_dialog`. Both are **front-end-only action rows** (`command: None`) resolved through `command_action::ui_action_for_front_end_action`, per fact 2, so they reach the user from the menu and one accelerator and not from the command palette. `Ctrl+Shift+S` is taken from the `win.sidebar-show-project` reserved row, which is deleted. One `wolf-core` change is required: `InputEvent::SaveAs` gains a `force: bool` field, because the dialog has already confirmed an overwrite while `saveas` without `!` fails `E13` and core degrades that to an invisible `tracing::warn!`.

**Findings that touch the six measured facts:** none contradicts them. Two supporting measurements I made from the source and record here because the plan leans on them:

- `Snapshot::current_file` is `expand('%:p')`, canonicalized with **a raw-path fallback** (`crates/wolf-core/src/redraw.rs:481-486`). So a never-written `Untitled N.md` reaches the front-end as `Some(<cwd>/Untitled N.md)` — fact 3, visible from the snapshot, with no RPC.
- `actions::action_for_command` has **no production caller** (only `command_action_round_trip`), which is what makes deleting a table row safe.

**Two pre-existing doc defects found**, to be fixed as part of this unit because it makes them worse: `crates/wolf-gnome/Cargo.toml:3` ends "saving is still from within Neovim"; `crates/wolf-gnome/src/lib.rs:318` says "there are no save / save-as dialogs"; `crates/wolf-gnome/README.md:18` says "Saving is `:w` or `:saveas` from inside Neovim."; and `tests/README.md`'s "Deliberate test gaps" claims "no dialog can be driven at all", which this unit falsifies. Separately, `flake.nix:559-561` and `crates/wolf-gnome/tests/README.md:109` **already disagree** about the guard count (eight vs nine, sixty-four vs sixty-five); flag and reconcile by counting rather than guessing.

---

## 2. The decisions, with their reasons

### 2.1 The accelerator rows

**Save takes `Ctrl+Shift+S`; Save As takes no chord and is menu-only.**

Reason: fact 5 says both macOS spellings (⌘S, ⇧⌘S) transliterate to the same `Ctrl+Shift+S`, so at most one can hold it. This table already answers that collision the same way twice — `win.new-buffer` (macOS ⌘N) holds `Ctrl+Shift+N` while `app.new-window` (⇧⌘N) is `MenuOnly`; `win.go-to-file` (⌘O) holds `Ctrl+Shift+O` while `win.open-folder` (⇧⌘O) is `MenuOnly`. The plain-⌘ row takes the chord; the ⇧⌘ sibling goes to the menu. Save is also the chord a user presses reflexively and constantly, where Save As is a deliberate act.

**What each row gives up, one line each:**
- `win.save` / `<Control><Shift>s` — gives up only the Ctrl+Shift alias for `<C-s>` (LSP signature help in insert/visual/select mode); plain `Ctrl+S` is never bound and still reaches Neovim as `<C-s>`.
- `win.save-as` / no accel — gives up nothing; it spends no chord at all.

**Mechanical check.** `<Control><Shift>s` parses under `parse_accel` (two known modifier tokens plus a one-char alphanumeric key). `forbidden_reason` accepts it: `single_letter` is true, `p.ctrl && !p.shift` is false (Shift is present), `p.alt` is false, `ctrl_fold` answers `Byte(0x13)` which is not in `CTRL_FOLD_FORBIDDEN`, and the `if single_letter { return None; }` gate exempts it from the declaration rule — so `ctrl_fold: None` is correct and required, exactly as the twelve existing letter rows have it. `action_for_key` matches modifiers exactly, so plain `Ctrl+S` never matches.

**What happens to `win.sidebar-show-project`: delete the row.**

I checked the alternatives against the tests before choosing:

- **May a row be `Reserved` with `accel: None`?** Yes, mechanically. Every accelerator invariant (`every_accel_parses`, `accels_use_key_names_not_symbols`, `no_accel_makes_a_neovim_key_unreachable`, `accels_are_unique`) opens with `let Some(accel) = binding.accel else { continue; }`; `bound_bindings()` and `action_for_key` filter on `Bound`; `menu_never_lists_a_reserved_action` bites only if a menu item names it, and none does. So it compiles and passes. It is nevertheless **wrong**: `BindingStatus::Reserved`'s own doc comment says "The chord is spoken for by a later unit", and a row with no chord speaks for nothing. Keeping it would make `Reserved` mean two things.
- **Is a `MenuOnly` row required to appear in `MENU_SECTIONS`?** No — no test loops over MenuOnly rows; `the_about_row_carries_no_chord` and `the_new_window_row_is_menu_only_and_listed` each assert `listed` for their own named row only. So `MenuOnly` with no menu item also passes — and is also wrong, for the mirror reason: `Command::SidebarShowProject` classifies `Deferred`, so `install_actions` skips it and a menu item would render insensitive.
- **Give the reserved row a different chord?** Rejected: it would spend one of the scarce free `Ctrl+Shift` letters on a feature that does not exist and whose future author cannot review the choice.

Deletion is safe and loses nothing: the command survives in `COMMAND_SPECS`, `command_action::ui_action_for`'s `Command::SidebarShowProject => Deferred(...)` arm is untouched (it matches on `Command`, not on the table), the palette already filters `Deferred` rows out, `:WolfSidebarShowProject` still drains and prints its reason, and `action_for_command` has no production caller. The macOS reason for the row (⇧⌘E, which collides with `win.buffer-picker`'s `Ctrl+Shift+E` anyway) already lives where it belongs — in that `Deferred` reason string. Three Reserved rows remain (the font-size ones), so `action_for_key_table`'s "a Reserved row must not match" case (`Ctrl+minus`) and `ctrl_fold_table`'s three `NoFold` rows stay non-vacuous.

### 2.2 What Save does on a buffer that is named but not on disk

**Rule: Save writes it. A dialog is offered only when the buffer has no file name at all.**

Fact 3 is the whole argument, and it cuts against the reflex answer. In every other editor an unnamed buffer has *no* name, so prompting is the only thing possible. Here `runtime/nvim/init.lua`'s `wolf_name_untitled` has already chosen a concrete, collision-checked name in the project folder — checked against files on disk, against every other buffer, and against stale swap files — and has eagerly created the swap file to claim it. That name is on screen from the moment the buffer exists: in the tab title, the pane statusbar, and the window's file row. Saving to the name the user is already looking at is the least surprising outcome available.

Three further reasons, in order of weight:

1. **Ctrl+S and `:w` must not disagree.** Both surfaces sit in one window over one buffer. If the chord popped a dialog where `:w` wrote silently, the user gets two behaviours for one act and cannot predict which.
2. **A redirect would need a fact the front-end does not have, and can get wrong.** "Not on disk" is a filesystem probe, and its answer drifts: a file deleted out from under a buffer that has been saved all session would suddenly start popping a modal on every Ctrl+S. Injecting an `exists` predicate is an established pattern here (`init_lua::resolve_init_lua`), so it is *possible* — it is simply the wrong rule.
3. **Save is the safety-critical chord.** Making it conditional on a modal means that when the dialog fails to present — which fact 4 shows is a live failure mode — the buffer is not saved while the user believes it is. Writing an unwanted `Untitled.md` is trivially undone; a silent non-save is not.

**The unnamed case is different and takes the dialog.** `Snapshot::current_file` is `None` for a genuinely nameless buffer (a typed `:enew`), and `write` there raises `E32: No file name`, which core degrades to an invisible warning. There is no working behaviour to diverge from, so Save falls through to the Save As dialog. This is the rule the seam states, and it is decidable from `Option<&Path>` alone — no probe, no RPC.

**What the dialog contributes over `:w` / `:saveas`.** For Save As: native folder browsing, name completion, and the overwrite confirmation, none of which a typed `:saveas` offers — and the user does not have to know the Ex command or type a path by hand. For Save: **nothing**, and that is exactly why Save never opens one on a named buffer.

**Two more shapes the seam refuses**, both readable from the same snapshot pass and both otherwise silent:
- **A preview scratch is active.** `Snapshot::is_preview_active()`; its `current_file` is `""` (the emit sends the empty string deliberately, `init.lua:387`), so without this arm Save would fall to `Choose` and open a modal over a pane that cannot be written.
- **A terminal buffer is active** (`win_buffers[active_win].is_terminal`). `term://…` is a name Neovim will not `write`.

### 2.3 Where the buffer path comes from (no new blocking RPC)

**`WolfCore::read_snapshot()` → `Snapshot::current_file`**, plus `Snapshot::is_preview_active()` and `Snapshot::active_win` joined into `Snapshot::win_buffers`. All are field reads under one `SnapshotReadGuard`; `read_snapshot` copies nothing (`draw.read` measures 0.00 ms, per DECISIONS "The measured reading"). **The enumerated ten blocking calls are unchanged.**

`WolfCore::pane_status(grid)` is deliberately **not** used, even though it is also RPC-free: it joins through the current-tabpage-only `Snapshot::windows` map and answers a *display* string with `NO_NAME` for an unnamed buffer — the exact trap `tabs_ui.rs`'s module docs record. Save needs "is there a path" as a value, not a label.

The shell reads it in one pass, the `preview_shape()` idiom: one `self.inner.core.borrow()`, one `read_snapshot()` bound to a local, an owned value built, the guard dropped before anything else touches the core. One value rather than three reads, because the three must describe the same instant.

### 2.4 The liveness gate

**Both presses are gated on `Some(Liveness::Alive)`, and neither is a blocking call.** The wording in every doc must be careful here, because it is the one place a reader could conclude the inventory grew.

- Neither issues an RPC. `InputEvent::Save` and `InputEvent::SaveAs` are fire-and-forget on the ordered input channel (fact 1); the dialog resolves on a later main-loop turn. **The inventory in DECISIONS "Crash and stall recovery" stays at ten, nine of them running-window.**
- The gate exists for `clipboard_ui::paste_decision`'s reason, not `nav_ui::nav_press`'s: a save queued behind an opaque crash overlay would reach a *restarted* Neovim minutes later and be applied to whatever buffer that fresh instance happens to be on.
- Save As adds a second, sharper reason: a **native modal dialog opening over the opaque crash overlay** invites the user to name a file that will never be written, and the modal covers the overlay's Restart button, which is the only route out of a stall (DECISIONS: "Nothing in this crate clears `NotResponding`").

The two reasons must not be collapsed, exactly as `clipboard_ui.rs`'s entry says of Copy and Paste.

**Save and Save As stay enabled in the empty state**, matching the buffer picker and the clipboard rather than `win.go-to-file` / `win.search-in-project` / `win.toggle-sidebar`. Those three are disabled because their *root* would be wrong; Save has no root to be wrong, and the captive Neovim in a project-less window is a working editor with a real `Untitled.md`. No change to `set_empty_state`.

### 2.5 Overwrite

**Add a `force: bool` field to `InputEvent::SaveAs`, applied as `saveas!` when true and `saveas` when false. The GTK glue always passes `true`, and that value comes out of the seam rather than being spelled at the call site.**

The problem: the save dialog confirms the overwrite itself, and then `saveas` (no `!`) fails `E13: File exists`, which `input::apply_batch` degrades to `tracing::warn!`. The user answers a confirmation and nothing is written, with nothing on screen.

Rejected alternatives:
- **Burn `!` into the only spelling.** Blast radius is nominally zero (`crates/wolf-core/tests/save_open.rs` is the sole construction site; `wolf-ffi` never builds the variant, and macOS has no save UI — measured by grep, matching fact 1). But it means the day macOS grows an `NSSavePanel` it inherits force-overwrite whether or not its panel confirmed, silently, in a crate shared by both front-ends. A destructive default with no opt-out is the wrong shape for a public API.
- **Front-end existence check.** Wrong: the buffer must be *renamed*, which only `saveas` does, and a check would race the write.
- **Leave it.** Unacceptable — it is a silent data loss of the user's stated intent.

**Blast radius of the chosen change.** `crates/wolf-core/tests/save_open.rs` (one construction site, `save_as_new_path`, which becomes `force: false` and keeps its meaning). `architecture.md:203`, which spells the variant. `crates/wolf-core/src/README.md`'s `input.rs` entry gains one Decisions bullet. macOS: zero producers, so nothing compiles differently, but the `wolf-ffi`-consumed public API changed and `macos-followups.md`'s standing rule (line 5) makes a ledger row mandatory.

**Evidence the change needs** (§4.C): a real-Neovim pair in `save_open.rs` proving both arms, each discriminating the other.

---

## 3. Files to create

### `crates/wolf-gnome/src/save_ui.rs`

The pure, GTK-free rulebook for Save and Save As. Modelled on `nav_ui.rs` / `tabs_ui.rs`: `wolf_core::Liveness` in, plain paths and bools in, values out. No widget type, no `WolfCore`, no filesystem.

**Module docs must carry** (following `nav_ui.rs`'s section shape): why the "named but not on disk" case writes rather than prompts (§2.2, including that fact 3 is the reason and that a reader must not "fix" it into a redirect); that neither press blocks and the gate is the paste reason plus the modal-over-overlay reason, with an explicit "the blocking inventory in DECISIONS.md is unchanged"; totality over `Option<Liveness>` with no `_` arm; and that the target is read from `Snapshot::current_file` and never from `pane_status`.

**Public API:**

```rust
pub const UNTITLED_SAVE_NAME: &str = "Untitled.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveTarget<'a> {
    pub path: Option<&'a Path>,   // Snapshot::current_file
    pub is_preview: bool,         // Snapshot::is_preview_active()
    pub is_terminal: bool,        // win_buffers[active_win].is_terminal, false when absent
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavePress { Write, Choose, Refuse(&'static str) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAsPress { Choose, Refuse(&'static str) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveProposal { pub folder: Option<PathBuf>, pub name: String }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveAsRequest { pub path: PathBuf, pub force: bool }

#[must_use] pub fn save_press(liveness: Option<Liveness>, target: SaveTarget<'_>) -> SavePress;
#[must_use] pub fn save_as_press(liveness: Option<Liveness>, target: SaveTarget<'_>) -> SaveAsPress;
#[must_use] pub fn save_dialog_proposal(path: Option<&Path>, cwd: Option<&Path>) -> SaveProposal;
#[must_use] pub fn save_as_target(chosen: &Path) -> Option<SaveAsRequest>;
```

**Rules, in evaluation order.**

`save_press` — a `match` over `Option<Liveness>` with no `_` arm. Liveness is checked **first**, so a wedged Neovim is reported as wedged rather than as a preview pane (`preview_ui`'s recorded ordering rule). Inside the `Some(Alive)` arm: `is_preview` refuses, then `is_terminal` refuses, then a private `save_name(path) -> Option<&OsStr>` decides — `Some` gives `Write`, `None` gives `Choose`. `save_name` is `path.filter(|p| !p.as_os_str().is_empty()).and_then(Path::file_name)`, which is what makes "named" mean "has a usable file name" rather than "is `Some`".

`save_as_press` — identical, except the accepted arm is always `Choose`. Two enums rather than one so the shell's Save As match has no dead `Write` branch.

Six distinct refusal reasons, each a lowercase mid-sentence clause (the `command_action` / `nav_ui` convention), shared by both functions where the cause is the same: `NotResponding`, `Dead`, `None` (core taken out by the close handler), preview, terminal.

`save_dialog_proposal`:
- `name` = the path's `file_name()` when there is one, else `UNTITLED_SAVE_NAME`.
- `folder` = the path's own `parent()` when it is non-empty and the path had a file name; else `cwd`; else `None` (leave GTK its default).
- **Why the buffer's own parent, not the cwd:** Save As on `src/deep/thing.md` must open where the user is working, or "save a copy beside it" becomes a navigation task. For a `Untitled.md` in the project root the two rules coincide, so nothing is lost.
- **Why the cwd passed in is not `tabs_ui::new_tab_cwd`'s answer:** that one refuses the empty state on purpose, so a tab is never `:tcd`'d into `$HOME`. A save dialog's folder is a starting point the user can change, not a declaration that a directory is a project, so the empty state's `$HOME` is a correct default here. The divergence is deliberate and must be stated.

`save_as_target`:
- `None` for an empty path, a path with no `file_name()`, or a **relative** path — `chdir_to`'s recorded reason: core would resolve it against the *process* cwd rather than Neovim's, so the file would land somewhere the user did not choose. `gio::File::path()` is absolute in practice; the rule is stated so a later change of source cannot smuggle one in.
- Otherwise `Some(SaveAsRequest { path: chosen.to_path_buf(), force: true })`.
- **The path is passed through verbatim — no extension is ever appended.** Proposing `.md` is the dialog's job. Rewriting the user's typed name would mean GTK's overwrite confirmation was asked about a *different* file than the one written, which turns a confirmed save into an unconfirmed clobber.
- `force` is always `true`, and the reason belongs on this function: the dialog has already confirmed. The shell copies the value out and decides nothing.

---

## 4. Files to modify

### A. `crates/wolf-core/src/input.rs`

- `InputEvent::SaveAs` gains `force: bool`, with a doc comment stating: `true` applies `saveas!`, for a caller whose own dialog has already confirmed the overwrite; `false` applies plain `saveas`, which fails `E13: File exists` on an existing path — an error the input pump degrades to a `tracing::warn!` the user never sees.
- The apply arm becomes `saveas!` / `saveas` on that flag, keeping the `fnameescape(path)` interpolation unchanged.

### B. `crates/wolf-gnome/src/lib.rs`

- `pub mod save_ui;` in alphabetical position (between `render` and `scrollbar`). **No `pub use` re-export** — no external consumer reaches for it.
- Add one crate-doc seam bullet for `mod@save_ui` in the same voice as the others, naming the four functions and the "named but not on disk writes; nameless prompts" rule.
- **Rewrite line 318 in place**: "…the menu's native Open File / Open Folder dialogs; there are no save / save-as dialogs." That clause is false after this unit. Replace with a present-tense statement that Save and Save As are native dialogs too and that saving goes through `InputEvent::Save` / `InputEvent::SaveAs`.

### C. `crates/wolf-gnome/src/actions.rs`

**Production data.**

1. **Delete** the `win.sidebar-show-project` `ActionBinding` in its entirety (currently lines 296-304), and the `// --- Reserved for later units …` comment stays with the three font-size rows that follow it.
2. **Insert two rows** immediately after `win.new-tab` and before `win.close-pane`, matching macOS File-menu order (New / Open / Save / Close):

```rust
    ActionBinding {
        action: "win.save",
        accel: Some("<Control><Shift>s"),
        ctrl_fold: None,
        command: None,
        status: BindingStatus::Bound,
        note: "macOS Cmd+S; front-end-only, since the registry carries no Save command, and gives \
               up only the Ctrl+Shift alias for <C-s> (LSP signature help in i/v/s), which is \
               still typed as plain Ctrl+S",
    },
    ActionBinding {
        action: "win.save-as",
        accel: None,
        ctrl_fold: None,
        command: None,
        status: BindingStatus::MenuOnly,
        note: "macOS Shift+Cmd+S; menu-only because Ctrl+Shift+S is spent on win.save and no \
               Ctrl+Shift+<letter> is free, the app.new-window precedent, so the primary menu is \
               its only route",
    },
```

3. **Insert a new `MenuSection`** between the current section 0 (New … Search in project) and section 1 (Back / Forward / Toggle sidebar / Terminal), holding `MenuItemSpec { label: "Save", action: "win.save" }` and `MenuItemSpec { label: "Save As…", action: "win.save-as" }`. Both rows carry `command: None`, so `menu_labels_match_the_registry` skips them and the labels are free — which they must be, since the registry has no Save titles. Note that in the section's comment, so nobody later adds a command without noticing the label constraint appears.

**Test constants that move** (this is the exact list):

| Site | Change |
| --- | --- |
| `EXPECTED_BOUND_ACTIONS: [&str; 14]` | → `[&str; 15]`, gaining `"win.save"` in sorted position between `"win.paste"` and `"win.search-in-project"` |
| `bound_set_is_exactly_this_unit` assertion message | "exactly fourteen" → "exactly fifteen"; add `Ctrl+Shift+S save` to the parenthetical chord list; add a one-element-diff paragraph for `"win.save"` in the established shape (demotion unbinds the chord, `action_for_key` stops scanning the row, `Ctrl+Shift+S` falls through to Neovim as `<C-s>`, Save becomes menu-only while the menu item advertises a chord that cannot fire; **not isolated** — `action_for_key_table`'s two `Ctrl+Shift+S` rows report `None` and `menu_never_lists_a_reserved_action` rejects the "Save" item) |
| `bound_set_is_exactly_this_unit` doc comment | no structural change |
| `action_for_key_table` doc comment | "the fourteen Bound chords" → "fifteen", in all three places (≈ lines 1375, 1455, 1525) |
| `action_for_key_table` cases | **+3 rows**: `("Ctrl+Shift+s -> win.save", key_from_char('s'), ctrl \| shift, Some("win.save"))`; the shifted-glyph twin with `key_from_char('S')`; and `("Ctrl+s (no Shift) -> None, so <C-s> (LSP signature help in i/v/s) still reaches Neovim", key_from_char('s'), ctrl, None)` |
| `ctrl_fold_table` | **no change** — its three `NoFold` rows are the font-size rows, which stay Reserved |
| `key_name_matches_table` | **no change** |
| `EXPECTED_APP_SCOPED_ACTIONS` | **no change** — both new rows are `win.` |

**New test** `the_save_as_row_is_menu_only_and_listed`, a sibling of `the_new_window_row_is_menu_only_and_listed`, asserting `accel == None`, `ctrl_fold == None`, `status == MenuOnly`, `command == None`, and `listed` in `MENU_SECTIONS`, each with the reason spelled out in the message (see §5 for its falsifiers). `win.save` needs no counterpart: it carries a chord, so all four accelerator invariants evaluate it.

### D. `crates/wolf-gnome/src/command_action.rs`

- Two new `UiAction` variants, doc-commented:
  - `SaveBuffer` — "Write the focused buffer to its own path; a buffer with no file name at all falls through to the save dialog instead, per `save_ui::save_press`."
  - `SaveBufferAs` — "Open a native `gtk::FileDialog` to choose a new path for the focused buffer."
- Two arms in `ui_action_for_front_end_action`: `"win.save" => Some(UiAction::SaveBuffer)`, `"win.save-as" => Some(UiAction::SaveBufferAs)`.
- **Module doc, "The three classes"**: rewrite the "Front-end-only rows" bullet in place to enumerate the two navigation rows, About, **and Save and Save As**, with fact 2's reason: adding `Command::Save` / `Command::SaveAs` is a cross-platform change (the Swift `CommandKind` port and `CommandDispatcher`'s `default`-free switch would both need arms) that cannot be compiled or verified on this host, and macOS has no save UI to point them at.
- **`front_end_action_rows_are_classified`'s non-vacuity message** enumerates the front-end rows by name — update it to include `win.save` and `win.save-as`.
- **No change** to `EXPECTED_DISPATCHED_IDS`, `CASES`, `ui_action_table`, `dispatched_commands_are_the_expected_set`, `APP_SCOPED_UI_ACTIONS`, or `unimplemented_reasons_are_distinct_and_non_empty` — this unit adds no `Command`.

### E. `crates/wolf-gnome/src/shell.rs`

- `use crate::save_ui::{self, SavePress, SaveAsPress, SaveTarget};`
- New const beside `OPEN_FILE_DIALOG_TITLE`: `const SAVE_DIALOG_TITLE: &str = "Save As";`
- New private struct `SaveShape` (owned, so no guard is held), the `PreviewShape` precedent: `path: Option<PathBuf>`, `is_preview: bool`, `is_terminal: bool`, with a doc comment saying the three must describe the same instant.
- `fn save_shape(&self) -> Option<SaveShape>` — `read_active_win`'s idiom exactly: one core borrow, one `read_snapshot()` bound to a local, `snapshot.current_file.clone()`, `snapshot.is_preview_active()`, and `snapshot.active_win.and_then(|w| snapshot.win_buffers.get(&w)).is_some_and(|b| b.is_terminal)`; the guard drops at function end, before any caller takes another core lock.
- `fn save_buffer(&self)` — reads liveness, then the shape, in two separate short borrows (`open_preview`'s shape). Matches `save_ui::save_press`, exhaustively:
  - `Write` → `self.send_input(InputEvent::Save)`
  - `Choose` → `self.open_save_dialog()`
  - `Refuse(reason)` → `eprintln!("wolf-gnome: save not performed: {reason}")`
- `fn save_buffer_as(&self)` — same shape over `save_ui::save_as_press`, with `eprintln!("wolf-gnome: save as not performed: {reason}")`.
- `fn open_save_dialog(&self)` — builds the proposal from the shape's path and `self.inner.rooted_cwd.borrow().clone()` (cloned out in its own statement, per the borrow discipline; it needs no core borrow at all and follows every re-root), then follows `open_file_dialog` **exactly**: `gtk::FileDialog::builder().title(SAVE_DIALOG_TITLE).modal(true)` plus `initial_name` and, when the proposal carries a folder, `initial_folder(&gio::File::for_path(dir))`; `let weak = Rc::downgrade(&self.inner);`; `dialog.save(Some(&self.inner.window), gio::Cancellable::NONE, move |result| …)`; inside, `upgrade(&weak)`, the same `// An Err here is overwhelmingly "the user dismissed the dialog"` comment, `if let Ok(file) = result { if let Some(path) = file.path() { if let Some(req) = save_ui::save_as_target(&path) { shell.send_input(InputEvent::SaveAs { path: req.path, force: req.force }) } } }`.
  - *Verify against gtk4-rs 0.11:* if `FileDialogBuilder` lacks `initial_name` / `initial_folder`, call `set_initial_name(Some(&name))` / `set_initial_folder(Some(&file))` on the built dialog instead.
- Two `perform` arms: `UiAction::SaveBuffer => self.save_buffer()`, `UiAction::SaveBufferAs => self.save_buffer_as()`.
- **Module doc**: extend the blocking-call section with one sentence in the shape of the existing "Paste adds none" / "The project-search overlay adds NO blocking call either" sentences: Save and Save As add none, both send on the ordered input channel and the dialog is asynchronous, and the ten stand. Then state the gate's reason as the paste reason plus the modal-over-crash-overlay reason, explicitly *not* the blocking one.

### F. `crates/wolf-core/tests/save_open.rs` — see §5.C.

### G. `crates/wolf-gnome/tests/smoke_headless.rs` — see §5.B.

### H. `crates/wolf-gnome/Cargo.toml`

Line 3 `description` ends "; saving is still from within Neovim." — **false**. Rewrite the clause to name the native Save / Save As dialogs. Per the `macos-followups.md` row-23 precedent this is not a `CARGO_BAZEL_REPIN`: `crates/wolf-gnome/Cargo.toml` is not in `crate_universe`'s `manifests`, and a `description` string resolves no dependency.

### I. `flake.nix`

The `# GATE-REQUIRED:` comment at 559-561: bump the smoke count for the two new scenarios, and reconcile the "sixty-four … the other eight are pure guards" phrasing against `tests/README.md`'s "nine guards … `65 passed`" — **these already disagree**. Count the `#[test]` functions in `smoke_headless.rs` that do not take `CAGE_LOCK`, and make both documents state the counted number. Comment-only edit; no package, derivation or attribute.

---

## 5. Test plan

Every behaviour below names the smallest source edit that would falsify it — the revert the coder applies to prove the test can go red.

### A. Seam tables, in `save_ui.rs`'s `#[cfg(test)] mod tests`

Follow `nav_ui.rs`'s structure: domain consts at the top (`ALL_LIVENESS_INPUTS: [Option<Liveness>; 4]`, `ALL_TARGETS: [SaveTarget; 7]` — `NAMED`, `NAMED_NESTED`, `UNNAMED`, `EMPTY_PATH`, `PREVIEW`, `PREVIEW_WITH_PATH`, `TERMINAL_WITH_PATH`), counts written as independent literals (the `SWEEP_INPUTS` / `NAMED_BUTTON_CASES` precedent — never as the product of the arrays' own lengths, which is a tautology), and a copied `is_sentence_cased` helper.

| # | Behaviour | Test | Falsifying edit |
| --- | --- | --- | --- |
| 1 | Save writes a named buffer and prompts only for a nameless one | `save_press_table`, 4 × 7 rows with a literal count assertion | In `save_press`, change the `save_name(..) == None` arm from `Choose` to `Write`. Red on the `UNNAMED` and `EMPTY_PATH` rows under `Alive`; that is the arm that turns Save on a nameless buffer into a silent `E32`. |
| 2 | A named-but-not-on-disk buffer still writes | the `NAMED` row of the same table, whose path is `/home/u/proj/Untitled.md` with a comment naming fact 3 | Add an `exists` probe and redirect to `Choose` — i.e. the rejected design. Red on `NAMED` alone. (Documented as the deliberate non-behaviour, so a later agent reading the table sees the choice.) |
| 3 | Only a live core may act | `only_a_live_core_may_save`, whole-domain sweep collecting every non-`Refuse` input, asserted against a literal expected set, plus a non-vacuity assertion that at least one accepts | Change the `Some(Liveness::Dead)` arm to fall through to the live behaviour. Red here and in `save_press_table`; the pair moving together is what says the classification changed. |
| 4 | Save As never writes without a dialog | `save_as_press_table` + `only_a_live_core_may_open_the_save_dialog`, same shape | Make `save_as_press` accept on `Dead`. Red on the modal-over-crash-overlay row. |
| 5 | A preview scratch and a terminal are refused, and liveness is read first | rows in both tables: `PREVIEW_WITH_PATH` under `NotResponding` must give the *wedged* reason, not the preview one | Move the `is_preview` check above the liveness match. Red on that row only. |
| 6 | The six refusal reasons are non-empty, lowercase and distinct | `save_press_reasons_are_distinct_and_non_empty` and its `save_as_press` twin, six probes each | Give the `Dead` and `NotResponding` arms the same string. |
| 7 | The dialog opens in the buffer's own folder | `save_dialog_proposal_table`, seven rows (§3) | Replace `path.parent()` with `cwd` in the named branch. Red on the `NAMED_NESTED` row alone — isolated. |
| 8 | A nameless buffer proposes `Untitled.md` in the window's cwd | rows 3–5 and 7 of the same table | Change the fallback name. Red on four rows plus behaviour 9. |
| 9 | The proposed name is the spelling the rest of the app produces | `the_untitled_save_name_is_the_new_buffer_spelling`, comparing `UNTITLED_SAVE_NAME` against a **re-typed literal** with the reason (it must match `runtime/nvim/init.lua`'s `wolf_next_untitled` at `i == 1`, or Save As proposes a name the app never makes and the buffer stops being markdown-typed) | Re-pin the const. **This one has no `src/` falsifier by construction** — it is a tripwire on a pin, the `the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes` shape. Say so in the doc comment. |
| 10 | The chosen path is passed through verbatim | `save_as_target_table`, including a `/home/u/proj/notes` row with no extension | Append `.md` when `extension()` is `None`. Red on that row alone — and its real cost is that GTK's overwrite confirmation was about a different file. |
| 11 | A relative or nameless choice is refused | rows in the same table | Drop the `is_absolute()` filter. Red on the relative row alone. |
| 12 | The front-end always requests a forcing `saveas` | every accepting row of `save_as_target_table` asserts `force == true`, with the reason in the message | Change `force` to `false`. Red on every accepting row. **State honestly in the doc comment**: this pins what the front-end *requests*; that the request has the effect it claims is `crates/wolf-core/tests/save_open.rs`'s. |

### B. Compositor smokes, in `crates/wolf-gnome/tests/smoke_headless.rs`

Both are **non-OCR readback smokes** — the observable is a file on disk, read by the Rust side. That avoids the block-cursor, page-density and fixture-name-vs-OCR-term rules entirely. Both route through `cage_command(SCENARIO, &cwd)` (never a second `Command::new("cage")`), take `CAGE_LOCK`, call `require_smoke_env()`, use `make_temp_cwd()` + `write_relaunch_fixtures`, `clear_evidence` on every readback path first, and register their fixture, readback and target file names in `RELAUNCH_FIXTURE_NAMES` so the generic loop keeps them clear of every other smoke's asserted terms.

**Smoke 1 — `ctrl_shift_s_saves_the_focused_buffer_to_disk_under_cage`** (mandatory).

Scenario, in the `NAVBACK_SCENARIO` shape (poll knobs, `wait_file`, exit codes `11` / `24` / `25`): launch; `:e $SAVE_FILE`; `i`, type `$SAVE_SENTINEL`, Escape; write `$BEFORE` from `bufname('%')` (precondition, so a failure below is attributable to the chord and not to a lost keystroke); fire `wtype -M ctrl -M shift -k s -m shift -m ctrl`; settle; teardown. **The scenario must contain no `:w` anywhere** and must not quit through a path that saves.

Rust assertions:
1. `status.success()` — precondition, no revert.
2. `$BEFORE` names `SAVE_FILE` — precondition.
3. **The discriminator**: the fixture file on disk contains `SAVE_SENTINEL`. Nothing else in the scenario writes it, so its presence is the chord's doing.

*Falsifying edit:* replace `shell::perform`'s `UiAction::SaveBuffer` arm body with `{}`. The chord still resolves and the action still activates; nothing writes; the file still holds only its heading. Predicted isolated among the smokes; every seam test stays green. The coder records the measured red per `tests/README.md`'s "A red-run demonstration is a deliverable".

**Smoke 2 — `the_save_dialog_writes_a_named_file_under_cage`** (judged sound; include).

The route matters and must be stated plainly: **Save As has no chord and its menu item is unreachable** (the primary-menu popover is a documented deliberate test gap). So this smoke reaches the dialog through the *other* door — `Ctrl+Shift+S` on a **nameless** buffer, which `save_press` answers `Choose`. That is a genuine end-to-end path through `save_press`'s `Choose` arm, `save_dialog_proposal`'s unnamed branch, `open_save_dialog`, the callback, `save_as_target`, and `InputEvent::SaveAs`.

Environment: chain `.env("GDK_DEBUG", SAVE_AS_GDK_DEBUG)` onto `cage_command`, with `const SAVE_AS_GDK_DEBUG: &str = "no-portals";` and a loud early assertion in the smoke body pinning that value against a re-typed literal, spelling out fact 4's hazard: without it GTK takes the portal route to the developer's real session bus, `cage` implements no `zxdg_exporter_v2`, and **a modal file dialog opens on the developer's desktop and the smoke hangs**.

Scenario: launch; `:enew` (a *typed* `:enew`, which leaves the buffer genuinely unnamed — the front-end's own New Buffer would name it); `i`, type `$SAVEAS_SENTINEL`, Escape; `Ctrl+Shift+S`; settle for the dialog to present; **select-all in the name entry** (`wtype -M ctrl -k a -m ctrl`) then `wtype -- "$SAVEAS_TYPED_NAME"`; `wtype -k Return`; settle; teardown. Assert on disk that `<cwd>/$SAVEAS_TYPED_NAME` exists and contains `SAVEAS_SENTINEL`.

*The one step the coder must verify by running it*: whether the pre-filled `Untitled.md` is replaced by the typed name or appended to. Fact 4 measured `wtype` of a name plus Return returning `Ok(<cwd>/typedname.md)`, but not against a pre-filled entry. If the first run produces a concatenated name, keep the select-all and adjust; do not weaken the assertion to a glob.

*Falsifying edit:* replace `shell::perform`'s `UiAction::SaveBuffer` arm's `Choose` branch with a no-op (or make `save_press` answer `Refuse` for a nameless buffer). No dialog opens, the typed name lands in the Neovim buffer instead, no file appears, and the readback poll fails.

**What it proves and does not prove** — record verbatim in `tests/README.md`:
- Proves: the action is installed, the seam's `Choose` answer reaches a real `gtk::FileDialog`, the dialog's `Ok(GFile)` reaches the callback, and the resulting `InputEvent::SaveAs` writes the file — against **GTK's in-process fallback**.
- Does **not** prove: anything about the portal route, which is the route a real desktop session takes (`gdk_display_should_use_portal` answers true wherever GDBus reaches a session bus). A different widget, a different overwrite confirmation, a different `GFile` producer.
- Does **not** prove: the overwrite confirmation, since the target does not exist, nor the front-end's `force: true` end to end. An overwrite smoke would have to keyboard-drive GTK's confirmation alert, which is fragile; recorded as unproven with the recipe rather than written.

### C. `wolf-core`, in `crates/wolf-core/tests/save_open.rs`

**No `BUILD.bazel` change is needed**, and that is the reason to put these here rather than in a new file: `crates/wolf-core/BUILD.bazel:1259-1281` already lists `tests/save_open.rs` and `tests/common/mod.rs` in the `rust_test(name = "save_open")` target. **If the coder creates a new test file instead**, it needs a new `rust_test` mirroring that block exactly — `name`, both `srcs`, `crate_root`, `tags = ["no-sandbox"]`, `size = "small"`, `edition = "2021"`, `data = ["//bazel/nvim:nvim_tree", "//runtime/nvim:init.lua"]`, the three `WOLF_*` env entries, and `deps = [":wolf_core"] + all_crate_deps(normal = True, normal_dev = True)` — and that wiring cannot be verified from this host. Do not create a new file.

| # | Behaviour | Test | Falsifying edit |
| --- | --- | --- | --- |
| 13 | An existing `SaveAs` producer keeps non-forcing semantics | update `save_as_new_path` to `force: false`, with a comment saying why `false` is right for a brand-new path and that it is what keeps the `E13` arm reachable | — (unchanged behaviour) |
| 14 | `saveas` without `force` refuses to clobber | new `save_as_refuses_to_overwrite_without_force`: a temp `.md` holding `ORIGINAL_TEXT`; a core on a *different* named buffer holding `SAVE_TEXT`; `SaveAs { force: false }` at the existing path; after a bounded settle assert the file still holds `ORIGINAL_TEXT` and not `SAVE_TEXT`, **and** that `core.read_snapshot().current_file` is still the original buffer's path — `:saveas` fires `BufFilePost`, so a successful rename would have moved it. The positive readback is what keeps the negative from being a timing artefact. | Change the apply arm to always `saveas!`. Red here; `save_as_with_force_overwrites` stays green. |
| 15 | `saveas!` overwrites | new `save_as_with_force_overwrites`: same setup with `force: true`; poll the existing file with `wait_for_file_contains` until it holds `SAVE_TEXT`, and assert `current_file` became the target | Drop the `!` — i.e. always plain `saveas`. Red here on the poll timeout; test 14 stays green. The two isolate each other. |

### D. What is deliberately **not** tested, and why

- The overwrite confirmation itself. It is GTK's or the portal's, never this crate's; nothing here observes it.
- The Save As **menu item's** own activation — the primary-menu popover is an existing documented deliberate gap.
- The `is_preview` / `is_terminal` / non-`Alive` refusal arms end to end: a refusal returns before any widget is touched, which is exactly the class `tests/README.md` already records as seam-table-only for `buffer_picker::open_decision` and `nav_ui::nav_press`.

---

## 6. Documentation

Exactly one owner per fact. Listed with what each must say.

**Must change:**

1. **`crates/wolf-gnome/src/README.md`**
   - **New** `#### save_ui.rs` entry, alphabetically between `render.rs` and `scrollbar.rs`. Purpose / Public API / Decisions / Gotchas, ≈200 words. Decisions carry: the named-but-not-on-disk rule and fact 3 as its reason; why the proposal takes the buffer's own parent; why no extension is appended; why the empty-state cwd diverges from `tabs_ui::new_tab_cwd`. Gotchas: the gate is not the blocking-call gate — link to DECISIONS.
   - **`command_action.rs` entry, edited in place**: the `ui_action_for_front_end_action` bullet currently reads "the two navigation rows and About". Rewrite that clause to include Save and Save As.
   - **`shell.rs` entry, edited in place**: one Decisions bullet beside the `preview_shape` one — Save and Save As read one `save_shape` pass each and issue no blocking call; the dialog carries `save_ui`'s proposal out and decides nothing.
   - **`actions.rs` entry**: **no edit required.** Nothing in it states a count or names `Ctrl+Shift+S`, and its "promoting a reserved row" gotcha stays true (three Reserved rows remain). Do not pad it.
   - **`lib.rs` entry**: no edit — it does not enumerate.

2. **`crates/wolf-gnome/DECISIONS.md`**
   - **"The accelerator namespace"**: extend the New Window paragraph with one sentence naming Save As as the second instance of the same rule (macOS ⇧⌘S; `Ctrl+Shift+S` spent on `win.save`; menu-only). Nothing there names `win.sidebar-show-project`, so its deletion needs no edit here.
   - **"Crash and stall recovery"**: **the enumeration of ten does not change.** Add one sentence beside the existing "Paste adds none" / "The project search adds none either" sentences, worded so it can never be read as Save joining the inventory: Save and Save As add none — both ride the ordered input channel and the dialog is asynchronous — and are nevertheless gated on `Alive` for the paste path's reason, with the added point that a native modal over the opaque overlay would cover the Restart button that is the only route out of a stall.
   - **"Present limitations"**: **delete** "No save or save-as dialog; saving is done from within Neovim. Open File and Open Folder are native dialogs." Replace with a present-tense bullet stating what is true: Save and Save As are front-end-only rows with no registry command, so **neither appears in the command palette**, whose rows come from `COMMAND_SPECS`; Save carries `Ctrl+Shift+S` and Save As is menu-only; and the overwrite confirmation belongs to GTK or to the desktop portal, so this front-end never observes it and cannot report a clobber it did not perform. (The evidence-side portal/cage fact goes to `tests/README.md`, below — the pair together covers the task's "palette gap and portal/cage limitation" with no duplication.)

3. **`crates/wolf-gnome/README.md`** — line 18's "Saving is `:w` or `:saveas` from inside Neovim." is false. Rewrite in place: saving goes the same way as opening, through `InputEvent::Save` / `InputEvent::SaveAs`, reached from the primary menu's Save and Save As items, from `Ctrl+Shift+S`, and from `:w` / `:saveas` typed inside Neovim.

4. **`crates/wolf-gnome/tests/README.md`**
   - `smoke_headless.rs` entry: the counts (56 → 58, non-OCR 19 → 21, `65 passed` → the counted number), and add "saving" to the "scenarios span…" list.
   - **Smoke conventions**: a new paragraph — a smoke that opens a native `gtk::FileDialog` **must** export `GDK_DEBUG=no-portals`, because GDBus reaches the developer's real session bus even with `DBUS_SESSION_BUS_ADDRESS` removed, so without it the smoke opens a modal dialog on the developer's desktop and hangs until it is answered. Per-smoke scenario environment, never production behaviour.
   - **"What is still unproven"**: three entries — the portal route is undriven (what smoke 2 does and does not prove, §5.B); the overwrite confirmation and the front-end's `force: true` are unexercised end to end, with the `wolf-core` pair named as what covers the effect; and Save As's menu item activation, which inherits the primary-menu gap.
   - **"Deliberate test gaps"**: the "native file dialogs … no dialog can be driven at all" bullet is now **false**. Rewrite in place: Open File and Open Folder are still undriven (trivial callback bodies; Open Folder's re-root is covered by the Open Recent smoke through the same `chdir_to`), while the save dialog is driven under GTK's in-process fallback.

5. **`feat-list.md`**
   - Line 99: tick Linux and put the branch in the Branch column — `| Native file dialogs (open / save / save-as) | `linux-feat/save-dialogs` | [x] | [x] |`. The macOS half predates any named branch, so the column carries the Linux branch alone (the column already mixes one-sided entries).
   - Line 105: rewrite the partial-coverage paragraph. It must stop saying a row is unticked. New content: all three dialogs are native on both front-ends; on Linux Save and Save As are front-end-only rows with no registry command, so they reach the user from the primary menu and (Save alone) `Ctrl+Shift+S`, and not from the command palette, whose rows come from the registry.
   - Lines 270-277: **delete item 2**, renumber 3 → 2 and 4 → 3. Item 1 (the preview coalesce) stays. **Change no other wording.**

6. **`architecture.md`** — line 203 spells `SaveAs { path }`; it becomes `SaveAs { path, force }`. That is the only architecture.md edit; the crate boundaries and contracts are unchanged.

7. **`crates/wolf-core/src/README.md`** — the `input.rs` entry's Public API bullet is generic ("…open (optionally at a line/column), save, chdir…") and needs no change, but add **one Decisions bullet**: `SaveAs` carries `force`, applied as `saveas!`, because a caller with its own confirming dialog must not lose the write to `E13: File exists`, which the input pump degrades to a warning nobody sees.

8. **`crates/wolf-core/tests/README.md`** — line 478 currently ends "…and `InputEvent::SaveAs` creating a path that did not exist." Edit in place to add the two new cases: a non-forcing `SaveAs` leaving an existing file and the buffer's name untouched, and a forcing one replacing both.

9. **`macos-followups.md`** — a new ledger row is **mandatory** under the standing rule at line 5 (`InputEvent` is `wolf-ffi`-consumed public API). Content: `InputEvent::SaveAs` gained `force: bool`; zero macOS producers today, so nothing compiles differently, but a future `NSSavePanel` must decide its own value since the panel confirms an overwrite the same way; `crates/wolf-core/tests/save_open.rs` gained two real-Neovim cases and **needs no `BUILD.bazel` change** (already wired as `rust_test(name = "save_open")`); `Cargo.toml`/`Cargo.lock` untouched, so no `CARGO_BAZEL_REPIN`; `crates/wolf-gnome` has no Bazel target. Also amend the existing `flake.nix` comment-chain row (row 12's habit) with the new smoke count.

10. **`flake.nix`** — the `# GATE-REQUIRED` comment (§4.I).

**Must NOT change, and why:**

- **`build.md`** — no build, run or test command changes. The two new smokes run in the existing `nix develop --command cargo test -p wolf-gnome`, and the `wolf-core` cases in the existing target. `GDK_DEBUG=no-portals` is per-smoke scenario environment, owned by `tests/README.md`, not a developer-facing command.
- **`command-lifecycle.md`** — this unit adds no `Command`, no registry row, no palette row and no drain path, so nothing in it goes stale. (Grep confirms it names neither Save nor `ui_action_for_front_end_action`.)
- **`runtime/nvim/README.md`** — no Lua changes.
- **`crates/wolf-gnome/src/actions.rs`'s README entry** — see item 1; nothing in it is falsified.
- **`crates/wolf-macos/**`** — nothing to change; fact 2 is why.

---

## 7. Implementation order

Every intermediate state compiles.

1. **`wolf-core`**: add `force` to `InputEvent::SaveAs` and the `saveas!` / `saveas` apply arm. This breaks the single construction site — fix `crates/wolf-core/tests/save_open.rs`'s `save_as_new_path` to `force: false` in the same step. Run `cargo test -p wolf-core --test save_open`.
2. **`crates/wolf-gnome/src/save_ui.rs`** plus `pub mod save_ui;` in `lib.rs`. Write the seam and its `#[cfg(test)] mod tests` (§5.A). Run `cargo test -p wolf-gnome --lib`.
3. **`command_action.rs`**: the two `UiAction` variants and the two `ui_action_for_front_end_action` arms. This makes `shell.rs::perform` non-exhaustive — go straight to step 4.
4. **`shell.rs`**: the title const, `SaveShape`, `save_shape`, `save_buffer`, `save_buffer_as`, `open_save_dialog`, the two `perform` arms. Build.
5. **`actions.rs`**: delete the `win.sidebar-show-project` row; add the two new rows and the new `MENU_SECTIONS` section. Then move the test constants (§4.C) and add `the_save_as_row_is_menu_only_and_listed`. Run `cargo test -p wolf-gnome --lib`.
6. **`command_action.rs` tests**: update `front_end_action_rows_are_classified`'s non-vacuity message and the module doc's class bullet.
7. **`wolf-core` tests**: add tests 14 and 15 to `save_open.rs`. Run them, then run each falsifying edit (§5.C) and record the measured reds.
8. **Smoke 1**, with its consts registered in `RELAUNCH_FIXTURE_NAMES`. Run it; run the `UiAction::SaveBuffer` revert and record the red.
9. **Smoke 2**, with `SAVE_AS_GDK_DEBUG` and its pin assertion. Run it (watch the first run for the pre-filled-name behaviour); run its revert and record the red.
10. **Seam falsifying edits** for §5.A rows 1, 3, 4, 5, 7, 10, 11, 12; record each measured red and its isolation.
11. **Docs** (§6), in the listed order, then `prettier --write` on every Markdown file touched and `./tools/lint/markdown_check.sh <files>`.
12. Full gate: `nix develop --command cargo test -p wolf-core`, `... -p wolf-gnome`, `nix develop --command cargo clippy --workspace --all-targets`.

---

## 8. Dependencies

**None.** `gtk::FileDialog::save` and the `initial_name` / `initial_folder` properties are already in the pinned `gtk4` 0.11.x; `gio::File::for_path` is already used in this crate; `wolf_core::Liveness`, `Snapshot` and `InputEvent` are already dependencies of `wolf-gnome`. No `Cargo.lock` change, therefore no `CARGO_BAZEL_REPIN`.

---

## 9. Lint considerations

No `#[allow]` directives are expected, and none should be added speculatively. The workspace sets no `[lints]` table and neither crate denies `missing_docs` or enables pedantic, so `cargo clippy --workspace --all-targets` at defaults is the bar.

Points to get right rather than silence:

- `#[must_use]` on all four `save_ui` public functions — the crate's convention, not a lint requirement.
- `SaveTarget<'a>` derives `Copy`, so passing it by value raises no `needless_pass_by_value`. It carries two bools, below `struct_excessive_bools`' threshold of three.
- `SaveProposal` and `SaveAsRequest` own their data and are not `Copy` — deliberate, since a `PathBuf` and a `String` cannot ride a `Copy` enum. That is why the proposal is a second function rather than a `SavePress` payload.
- `shell.rs`'s new functions take `&self` only, so `too_many_arguments` cannot bite.
- Both seam enums are matched exhaustively at every call site with no `_` arm, which is the point; `wildcard_enum_match_arm` is not enabled and must not be worked around.

---

## Orchestrator decisions after the coder's first pass

The production code for §7 steps 1-6 is in place and builds; `cargo clippy -p wolf-core
-p wolf-gnome --all-targets -- -D warnings` and `cargo fmt --check` are clean, and
`cargo test -p wolf-core --test save_open` passes (3 tests). Exactly one test is red:
`actions::tests::bound_set_is_exactly_this_unit`, which is the test-writer's to move.

Three plan corrections and two additions, all binding on the test-writer:

1. **The refusal-reason count in §3 / §5.A row 6 was wrong.** There are **five refusal
   causes per function** (`NotResponding`, `Dead`, no core, preview pane, terminal pane)
   and **six distinct strings across both functions**, because `NotResponding` carries a
   different string in `save_as_press` — the modal covers the stall overlay's Restart
   button, which `liveness_ui.rs:170` confirms is the only route out of `NotResponding`
   (a `Dead` core auto-restarts; a stalled one does not). A distinctness test must
   therefore probe five arms per function and assert six distinct strings across the
   pair. A test demanding six probes from one function cannot pass and must not be
   written.

2. **`Snapshot::is_preview_active()` does exist** (`crates/wolf-core/src/render.rs:962`,
   a field read of `current_is_preview`); the orchestrator's fact was about the
   `WolfCore` wrapper, which takes its own guard and is deliberately not used.
   `Snapshot::win_buffers[..].is_terminal` exists too (`render.rs:523`), and `WinBuffer`
   also carries `is_drawer` — the seam refuses a terminal either way.

3. **Two extra seam rows are approved** on top of §5.A's tables, because each
   discriminates a live branch nothing else reaches:
   - `save_dialog_proposal` with a path that has a file name but an empty parent
     (`Untitled.md`, no directory component) must fall through to `cwd`.
   - `save_as_target` on `/` — absolute and non-empty, but `file_name()` is `None`, so
     it is the one row that tells the `file_name` check apart from the `is_absolute`
     check.

4. `action_for_key_table`'s doc comment says "the fourteen Bound chords" in three
   places and carries no `Ctrl+Shift+S` row; §4.C's three new rows and those three
   counts are required.

5. `front_end_action_rows_are_classified`'s non-vacuity message enumerates the
   front-end rows by name and now omits Save and Save As. Message-only, and required.
