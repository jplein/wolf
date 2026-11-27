# Code review recommendations, 2026-11-27

Last verified at commit `b00f5da940051b119d4fae91be2760529115ade1`.

A review of the whole repository at that commit, written to answer eight questions about the code. It changes nothing: every item is a recommendation.

The code is about 82,000 non-test lines: 21,400 in `wolf-core`, 6,700 in `wolf-ffi`, 2,000 in `wolf-vt`, 29,500 in `wolf-gnome`, 19,800 of Swift in `wolf-macos` and 2,350 in `init.lua`. It was read in thirteen slices by separate reviewers, each reading its files in full. The code behind every claim this report leans on was then re-read by its author. [Method and limits](#method-and-limits) says what was and was not run.

Each finding carries a status:

- **Verified**: the cited code was re-read for this report and says what the finding says.
- **Tested**: verified, and confirmed by running something.
- **Reported**: a slice reviewer found it by reading and it was not re-checked. Confirm before acting.
- **Suspected**: the code path is real, but the consequence depends on platform or library behaviour nobody exercised.

Line numbers are those of the commit above. A bare Swift file name is under `crates/wolf-macos/Sources/WolfMacOS/` or `WolfKeyMap/`, a bare Rust file name is in the directory of the path before it, and a bare `:123` continues the file before it.

## Verdict

The architecture holds up. Core is free of any toolkit and a gate keeps it so. The boundary to the front-ends is three plain shapes. Decisions live in small pure seams with tests, and the tests drive a real Neovim, real ptys and a real compositor.

The hard things were looked for and not found: a data race, a lock-order cycle, a memory-safety defect in the unsafe code, a retain cycle that keeps a closed tab alive, or a `RefCell` re-entrancy panic. Several reviewers swept for each.

What needs work falls into five groups:

1. **Untrusted input is trusted.** A file name can become a shell command (S1). A previewed Markdown file can run script, take over its pane, launch things, and read the project through the browser preview (S2 to S5).
2. **A Neovim that is waiting on the user is treated as hung.** Close and Quit then skip the unsaved check and send `qa!`, which also deletes the swap files meant to be the safety net (C1). Failures elsewhere never reach the user (C2).
3. **Costs were sized for one tab.** Threads, database connections, watchers and descriptors multiply by tabs (P1). On macOS every wake repaints every cell through the bridge (P2). Wakes are uncoalesced and a busy terminal drives the editor's repaint (P3). Start, close and quit are serial per tab on the UI thread (P5).
4. **The same rule is written twice.** Two thirds of the macOS seam logic has a Rust twin in the GNOME crate, and the platform differences found here came from that layer (D1).
5. **A few units have grown very large,** and the pane-binding logic inside two of them is untested glue where several of the defects found here sit (A4, A5).

## What to change first

In rough order of value for effort. The first six are small.

| #   | Change                                                                                                                                                                                                 | Closes             |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------ |
| 1   | Stop building Ex commands from paths; use `nvim_cmd` with arguments.                                                                                                                                   | S1                 |
| 2   | Probe `nvim_get_mode` before a blocking query; refuse a close when Neovim is waiting on the user; never send `qa!` on an unchecked close.                                                              | C1                 |
| 3   | Lock the preview down: a content security policy in the rendered head, page script off, cancel navigations the app did not issue, allowlist launch schemes, and no `NSWorkspace.open` of a local file. | S2, S3             |
| 4   | Harden the browser preview server: check `Host`, add a secret path, refuse a non-absolute root.                                                                                                        | S4, S5             |
| 5   | Show failures: echo Ex errors to the toast; one visible "refused, and why" surface. Turn modelines off.                                                                                                | C2, S6             |
| 6   | Cheap per-tab relief: two runtime workers per tab, raise the descriptor limit, one database connection. Clamp search ranges. Bound automatic restarts.                                                 | P1, C5, C4         |
| 7   | Publish editor frames at `flush`, with a version per row; add scalar accessors; coalesce wakes.                                                                                                        | A1, R1, R2, P2, P3 |
| 8   | Bound content search in core and give both front-ends one capped projection.                                                                                                                           | P6                 |
| 9   | One restore policy and one crash-restart restore, in core and in the spec.                                                                                                                             | C3, C4             |
| 10  | Fan out start and shutdown across tabs; bound start with a timeout.                                                                                                                                    | P5, C9             |
| 11  | Hoist shared rules into core, the preview cluster first; then the pane model.                                                                                                                          | A2, A5, D1         |
| 12  | Gates: pin one Rust toolchain, add `--locked`, check the generated bindings, add a many-tab test and hostile-name tests, start a flake ledger.                                                         | T1 to T7           |

## Answers to the eight questions

### 1. Does the code repeat itself?

Within a file, rarely. Across layers, a great deal.

The largest duplication is between the two front-ends: about 2,700 lines of Swift seam logic, and about 9,700 lines of its tests, exist again in Rust (D1). Next is the list overlay, written three times on each platform (D2). Core repeats itself in the setup functions, the recency stores and the terminal bind path, and the bridge repeats its wake channel five times and describes each call three times (D3). The test plumbing is the most literal case: 124 hand-copied build stanzas (T5).

### 2. Is the code readable?

At the level of a function, yes. Of about 2,900 functions only 22 exceed 100 lines, names are precise, and comments give reasons.

At the level of a file, less so. Seven units are too large to hold in mind (A4). Comment volume has become a readability cost of its own: `pane_view.rs` is one third doc comment, `api.rs` nearly half, and the linter's limits apply to plain comments, not doc comments (T10). Long comments drift, and this review found stale or wrong statements in most large files.

### 3. Can the code be reasoned about? Is there deep coupling?

The pure parts are easy to reason about, and there are many of them. The boundary contract is easy too: a snapshot under a wake, a drainable queue, a bounded query.

Three kinds of coupling make the rest harder:

- **Front-ends know core's internals** and re-derive rules from them: the shape of the preview document, where a restarted Neovim starts, when a close may be asked (A2).
- **Contracts that exist only as prose.** There are two unordered channels into Neovim (R4). A read guard must not be held across a blocking call. The wake thunk must always defer. A timeout does not cancel its call. The steps of a wake must run in one fixed order. Each is documented; none is enforced.
- **Three notions of focus** are kept in step by hand: the front-end's active pane, the toolkit's first responder and Neovim's current window (R3).

The coupling is concentrated. `PaneContainerView`, `pane_view.rs`, `AppDelegate` with `WolfWindowController`, `TerminalRegistry` and `RedrawHandler` each know about most of their neighbours.

### 4. Are appropriate design patterns used?

Mostly, and consistently. Snapshot-and-wake with owned read guards, drainable queues, walk-then-swap, an effect queue in place of re-entrant C callbacks, generation stamps, an injected clock, and exhaustive matches with no wildcard arm are all the right tools and are applied the same way everywhere.

Seven are missing or misapplied:

| Where                             | In place of                                 | Use                                           |
| --------------------------------- | ------------------------------------------- | --------------------------------------------- |
| Editor grids (A1)                 | One shared snapshot mutated in place        | Published immutable frames, as terminals have |
| Paths into Neovim (S1)            | Ex command strings                          | Structured commands with arguments            |
| Liveness (C1)                     | Two states, alive and stalled               | A third: waiting on the user                  |
| Restore and close flows (A5, C11) | Flags advanced from many functions          | Explicit state machines with a failure exit   |
| Pane kind (A4)                    | Optional fields re-branched in each handler | A sum type                                    |
| Stores, watcher, listings (A3)    | One per tab                                 | Process-level services                        |
| Command interception (C7)         | Exact-word abbreviations                    | Parsing the command line                      |

### 5. Are there race conditions?

No data races were found. The logical races are in [Races](#races) and C1:

- A paint can read half of a redraw (R1).
- A click's focus is undone by an unrelated wake, sending keys to the wrong pane (R3).
- A blocking query can overtake queued input (R4).
- A close can race a Neovim that is busy or waiting, and lose unsaved work (C1).

### 6. What falls down with more windows, tabs or panes?

In the order a user is likely to meet them:

1. **A busy terminal anywhere in a tab** repaints every editor pane of that tab on each publish, on both platforms (P3).
2. **More panes on macOS**: frame cost is linear in visible cells and paid on every wake (P2).
3. **A broad content search** on a large project hangs the window (P6).
4. **Quit, or Close Window, with many tabs**: serial checks and serial shutdowns, each bounded by seconds (P5).
5. **Launch with many saved tabs**: every Neovim starts on the UI thread first, unbounded on macOS, and capped on GNOME by dropping tabs (C3, P5).
6. **Tens of tabs**: threads, connections and watchers multiply, and macOS may run out of descriptors (P1).
7. **A long-lived tab with many buffers**: each buffer switch does work linear in buffers ever opened (P9).

### 7. Are there major correctness, performance or security issues?

Yes, in each:

- **Security:** S1 and S2 are high; S3 to S6 follow from them or stand beside them.
- **Correctness:** C1 can lose unsaved work. C2 hides failed saves. C3 loses saved tabs. C5 can abort the app.
- **Performance:** P2, P3 and P6.

### 8. Other findings that should result in changes

- **Text input** is incomplete on both platforms, and on macOS that excludes input methods from the editor (C6).
- **Tests and gates** have gaps that let the two platforms drift and let scale regress unseen (T1, T2, T7).
- **Documentation** disagrees with the code in at least sixteen places, and the "last verified" stamps that should flag that are not checked (T9).
- **Parity gaps are recorded only in `notes/feat-list.md`**, not in either limitations list: macOS has no Save or Save As, and GNOME has no font-size commands.
- **A model nothing uses** is still carried: several tabpages per Neovim (A8).

## Security

Wolf's job is to open other people's Markdown, so a cloned repository is the threat model. The findings below say that model is not yet honoured, and the first two are the most urgent items in this report. The stated goal in [`config-decisions-report.md`](config-decisions-report.md) is that "opening an arbitrary Markdown repository" is safe.

### S1. A file name can become an Ex command or a shell command

**High. Tested. Both platforms.**

`fnameescape` in `crates/wolf-core/src/input.rs:729` is a hand copy of Vim's function that omits newline, backtick, `$`, `!` and a leading `+`. Four input events splice its result into an Ex string for `nvim_command`: `edit` (`:617`, `:791`), `saveas` (`:622`) and `tcd` (`:625`).

Each omission is exploitable by a name in a cloned repository, and three were run against the pinned Neovim 0.12.6:

- **Newline.** It ends an Ex command, so the rest of the name runs as the next one. Given `edit <dir>/a.md`, a newline and `!touch MARKER`, escaped as core escapes it, `nvim_command` started a shell.
- **Backticks.** The `init.lua` reviewer opened a path with one backtick ending its directory name and another inside its file name, and `./x.sh` between them. Neovim ran `./x.sh` from the tab's working directory and opened the file named by what was left.
- **`$`.** `cost$5.md` opened `cost.md`, and `saveas` wrote a different name than the save dialog had confirmed.

Clicking such a file in the tree, the picker or a search result is enough, and Open Folder reaches `tcd` the same way. The Lua side is safe except for newline: `init.lua` uses the real `vim.fn.fnameescape`, but `vim.cmd("edit " .. …)` still splits on a raw newline at five sites (`runtime/nvim/init.lua:1215`, `:1271`, `:2041`, `:2199`, `:2237`).

**Change.** Stop building Ex strings on both sides. `nvim_cmd` with `args = { path }` and `magic = { file = false, bar = false }` opened, saved and changed directory correctly for every hostile name the reviewer tried. One Lua helper can do it and also echo a failure (C2). Delete Rust's `fnameescape`, and add a real-Neovim test over names with a space, `%`, `#`, `|`, backticks, `$`, a newline and a leading `+`.

### S2. A previewed document runs its own HTML and script, and can take over its pane

**High. Verified for the code paths; the script-driven variants are suspected. Both platforms.**

Three facts combine:

- `render_html` hands pulldown-cmark's events straight to `push_html` (`crates/wolf-core/src/markdown.rs:581`), so raw HTML, `<script>`, event-handler attributes and `<meta http-equiv=refresh>` pass through. The document head carries no content security policy (`:592`). A reviewer reports that math is a second raw channel, through `\text{…}` in pulldown-latex.
- Both web views are built with default settings, so page script runs: `WKWebViewConfiguration()` at `PaneContainerView.swift:2019`, and `webkit6::WebView::builder()` at `crates/wolf-gnome/src/pane_view.rs:2576`.
- Both navigation handlers intercept only a clicked link and allow everything else (`PreviewNavigationDelegate.swift:31`, `pane_view.rs:2600`). A script's `location = …`, a meta refresh or a form submit is not a click.

So previewing a hostile file gives its author script in the pane, unrestricted network access, and the ability to replace the pane with any remote page. The front-ends' bookkeeping still believes the original document is loaded, so the next edit takes the in-place path and writes the freshly rendered Markdown into the foreign page. On macOS the page also gets the three script message handlers, which let it set what Edit > Copy copies and steal pane focus.

Two documents overstate the current guarantee: `crates/wolf-macos/Sources/WolfMacOS/Preview/README.md` says the pane only ever shows core-rendered HTML, and [`following-a-link-in-the-preview.md`](wolf-gnome/following-a-link-in-the-preview.md) says the web view never navigates itself. Both hold for clicked links only.

**Change.** In order of value for cost:

1. Add a constant content-security-policy `<meta>` to the head in `render_html`: no script, no frames, no form action, inline style and `data:` fonts only. It covers both front-ends, the browser preview and the math channel, and keeps the head byte-identical. Check that the app's own injected scripts still run under it.
2. Turn page script off and keep the app's: `allowsContentJavaScript = false` on macOS, `enable_javascript_markup(false)` on GNOME. Move the macOS handlers into a private content world.
3. In both policy handlers, cancel every main-frame navigation the app did not issue itself.

### S3. A click in the preview can launch local files and any URL scheme

**High on macOS, medium on GNOME. Verified for the code path; not run.**

On macOS, when core declines to preview a clicked `file:` link, the app opens the file with its default application: `NSWorkspace.shared.open(URL(fileURLWithPath: path))` at `PaneContainerView.swift:2230`. A relative link in a cloned README to a `.command` file, an app bundle or an executable is therefore one click from running it. Files that arrive by `git clone` carry no quarantine attribute, so Gatekeeper does not ask. `PreviewLinkSeam.classify` also has no check on the link's host, so `file://other-host/…` is read as a local path.

GNOME refuses that case outright, and its note gives the reason that GNOME has no Gatekeeper or quarantine to lean on. For cloned content macOS has none either.

Both platforms hand every scheme other than `file` to the system launcher (`PreviewLinkSeam.swift:43`, `crates/wolf-gnome/src/preview_link.rs:246`). Neither checks that a click came from the user, so with S2 a script's `a.click()` may launch a handler with no click at all. That last step is suspected, not exercised.

**Change.** Remove the `NSWorkspace.open` fallback: open text in an editor pane, and otherwise reveal the file in Finder or do nothing. Allowlist `http`, `https` and `mailto` for the launcher on both platforms. Require a user gesture on GNOME. Best, move the link classifier into core so one table serves both front-ends (D1).

### S4. The browser preview serves the project to anything that can reach the port

**Medium to high. Verified.**

`Open in Browser` starts a loopback server per tab. It parses only the request line (`crates/wolf-core/src/browser_preview.rs:307`), so it checks no `Host` header. The URL carries no secret (`crates/wolf-core/src/api.rs:770`). It serves any existing file under the root, dotfiles and `.git/` included, and the root is the pinned file's parent when that file is outside the working directory, so previewing `~/notes.md` makes the root the home directory.

The random port is the only barrier. Any local process can read the user's files under the root. A page that rebinds its name to `127.0.0.1` becomes same-origin and can read them too. Worst, the served document is the same unsanitised HTML as S2, at the same origin as every file under the root, so a script in a previewed file can fetch `/.env` or `/.git/config` and post it out.

The server also never stops: `BrowserPreview::abort` has no caller, although its comment says `shutdown` uses it.

**Change.** Reject a request whose `Host` is not the server's own loopback address and port. Put an unguessable path prefix in the URL. Serve only Markdown and the known asset types, and refuse any dot-component. Send the policy from S2 as a response header. Stop the server after an idle period.

### S5. The path-traversal guard passes everything when the root is empty

**Medium. Verified by reading.**

[`wolf-core-decisions.md`](wolf-core-decisions.md#browser-preview-security) calls the guard a hard requirement. `build_target` can produce an empty root: an unnamed Markdown buffer gives an empty pinned path, and when the working-directory query fails or that directory no longer exists, the fallback chain ends in `pinned_abs.clone()` (`crates/wolf-core/src/browser_preview.rs:429`). `resolve_path` then joins the request onto the empty root, canonicalizes it against the process's working directory, and tests `canonical.starts_with(root)`, which is true for an empty root (`:515`).

The app's working directory is `/` when launched from Finder, so `GET /etc/passwd` is served. `build_target` has no test.

**Change.** Represent "no root" as `None` and serve only the pinned buffer in that state. Make `resolve_path` refuse a root that is not absolute. Add the test.

### S6. A document, or its project, can set the editor's options

**Medium. Tested by the `init.lua` reviewer. Both platforms.**

`init.lua` sets neither `modeline` nor `vim.g.editorconfig`, so Neovim's defaults apply: modelines are read, and the built-in EditorConfig plugin is active. Neither is mentioned in any document.

- Ordinary prose in the first or last five lines is parsed as a modeline. `Some notes, ex: buy milk` raises `E518`, and that error is thrown inside `wolf_rebuild_layout`, `wolf_preview_file` and `wolf_open_split_left`, so such a file can break a launch restore or a preview.
- A modeline can turn off the document's own swap file, which is the crash-recovery design, or make the buffer `nofile`, which hides it from the open-file mirror.
- A project's `.editorconfig` applied `trim_trailing_whitespace`, which strips a Markdown two-space hard break on save.
- Both can set `spelllang`, which by `runtime/nvim/README.md` raises a blocking download prompt when the spell file is missing.

**Change.** Set `modeline = false`. Decide EditorConfig explicitly: turn it off, or keep it, document it, mask `spelling_language` and reconsider whitespace trimming for Markdown.

### S7. Smaller security items

| Item | Where | Status |
| --- | --- | --- |
| On the packaged Linux build, shells in terminal panes inherit the wrapper's `FONTCONFIG_FILE`, `GIO_EXTRA_MODULES`, the pixbuf module path and the `WOLF_*` binary paths, so programs started there load Wolf's modules. Only `WOLF_TAB_ID` is scrubbed. | `crates/wolf-core/src/terminal/pty.rs:81`, `flake.nix:661` | Verified |
| Terminal replies to program queries go to an unbounded queue. Output made of queries, with a foreground program that is not reading, grows it without limit. | `crates/wolf-core/src/terminal/session.rs:157` | Verified (queue), Reported (growth) |
| Kitty graphics stay enabled although nothing paints them, so a probing program is told images work and each screen may hold the library's default image budget. | `crates/wolf-vt/src/lib.rs:581` | Reported |
| An `OSC 7` working-directory report ignores its host, so a remote shell's path is treated as local, and the path is `stat`ed on the UI thread. | `crates/wolf-core/src/terminal/pty.rs:184`, `session.rs:430` | Reported |
| The open-channel socket has the umask's mode between `bind` and `chmod`, and the state directory is created with the default mode, so `wolf.sqlite` is readable by anyone who can traverse the home directory on Linux. | `crates/wolf-core/src/open_channel.rs:383` | Reported |
| `Wolf.app` is ad-hoc signed with no hardened runtime and no entitlements. That is fine for a locally built app and blocks distribution as it stands. | `build.md` | Reported |

Three things were checked and are sound: the privileged install of the `wolf` command quotes every path correctly, a paste cannot inject the bracketed-paste end sequence, and every string the apps build into their own scripts is escaped or passed as an argument.

## Data loss and correctness

### C1. Close and Quit can discard unsaved work when Neovim is busy or waiting on the user

**High. Verified; found independently by four reviewers. Both platforms.**

Three behaviours chain together.

**A waiting Neovim looks hung.** Neovim does not answer an ordinary request while it waits on a keystroke: after `<C-w>`, a count, an operator, or at a prompt such as the overwrite question from `:w`. [`wolf-core-decisions.md`](wolf-core-decisions.md#threading-and-locking) records this, and the input pump handles it by probing `nvim_get_mode`. `block_on_rpc` does not: it waits the full `RPC_TIMEOUT_DEFAULT` of 5 s on the UI thread, then marks the tab `NotResponding` (`crates/wolf-core/src/api.rs:2236`). So typing `<C-w>` and then clicking another tab freezes the app for 5 s and raises "Neovim stopped responding" over a healthy Neovim, because the tab switch captures the cursor with a blocking call (`AppDelegate.swift:418`).

**No answer means close.** Both close seams treat a timeout, and any liveness other than `Alive`, as "close unchecked" (`CloseTabSeam.swift`, `crates/wolf-gnome/src/tabs_ui.rs:111`), and Close Window and Quit count such a tab as clean. A tab falsely marked stalled is not asked at all.

**The close then sends `qa!`.** `NvimProcess::shutdown` asks Neovim to quit with `qa!` and waits up to 7 s before killing it (`crates/wolf-core/src/nvim.rs:1672`). The documented reason an unchecked close is safe is that swap files keep the edits recoverable. That holds only when Neovim never answers. A Neovim that was merely busy, or waiting at a prompt, obeys `qa!` as soon as it is free: the buffers are discarded and their swap files deleted, which a reviewer confirmed by experiment.

The W12 prompt, for a file changed both on disk and in the buffer, implies unsaved work, and the two-tabs-on-one-file case the spec supports is exactly where a user clicks the other tab while it shows.

**Change.**

1. In `block_on_rpc`, send the fast `nvim_get_mode` first. When it reports `blocking`, return a distinct "waiting on the user" result at once, without sending the call and without marking the stall.
2. Map that result to "refuse, and raise the tab" for Close Tab, Close Window and Quit. For a cursor capture, keep the last capture.
3. On any unchecked close, skip `qa!` and kill, as `kill_in_place` does, so the swap files survive.
4. Better for Quit: mirror "has a modified buffer" into the snapshot from an autocmd, as the open-file set is mirrored, so the unsaved check needs no round trip at all.

### C2. Failures do not reach the user

**Medium to high. Verified; the Neovim side tested by a reviewer.**

`nvim_command` returns an Ex error to its caller and does not show it. Core's `command()` drops that reply: `let _ = nvim.request("nvim_command", …).await?;` handles only a transport failure (`crates/wolf-core/src/input.rs:771`). The comment on `SaveAs` says the error becomes a `tracing::warn!`. It is not logged at all.

So on GNOME, Save on a read-only file, an unwritable path or a full disk does nothing visible, and the user learns of it at close time. The same holds for a failed open or directory change on both platforms. Inside `init.lua`, `:wq`, `:x` and `:wqa` have the same hole: `do_quit` runs the write under `pcall` and returns silently when it fails (`runtime/nvim/init.lua:907`).

Around this sit two wider patterns. Every blocking query flattens failure to a neutral value by contract, so a wedged Neovim and an empty answer look alike. And the GNOME front-end reports each refused action with `eprintln!`, at about 70 sites, which a desktop-launched user never sees.

`init.lua` also sets `writebackup = false` with no recorded reason, so a save overwrites in place and a write that fails midway truncates the file.

**Change.** Echo an Ex error through `nvim_echo` so it reaches the message toast, as `do_bdelete` already does; one helper shared with S1 covers Save, Open and `tcd`. Give each front-end one "refused, and why" surface in place of `eprintln!`. Record the reason for `writebackup`, or point `backupdir` at a private directory.

### C3. Launch restore follows different rules on each platform, and GNOME's loses tabs

**Medium to high. Verified.**

GNOME caps a restore at `MAX_RESTORE_TABS = 8` per window and `MAX_RESTORE_NEOVIMS = 16` overall (`crates/wolf-gnome/src/state_seam.rs:44`, `:54`). The comment says the numbers sit "far above any session a user assembles by hand", but New Tab has no cap, so nine tabs in a window is an ordinary session. The tabs past the cap are not restored, and a reviewer reports that the first commit after launch then writes the truncated session back, so the loss is permanent. The same write-back drops any tab whose Neovim failed to start.

macOS has no cap at all (`RestoreSeam.swift:130`): every saved tab starts a Neovim on the main thread before the first frame. It carries tabs that failed to start into later commits, which GNOME does not. It does no validation of a saved tree, so zero weights lay out zero-size panes. And the two disagree on whether the home directory restores as a project.

[`application-state.md`](../specs/application-state.md) §5 says launch "recreates every window" and mentions none of this.

**Change.** Put one `restore_plan` in `wolf_core::app_state` beside `restored_tab`: the caps, the tree validation and the directory rule, decided once and written into §5. Carry un-restored tabs through as seeds on both platforms so a cap or a failed start never erases them. Start background tabs lazily and the cap can go.

### C4. Crash recovery: a lost working directory on GNOME, and a restart loop on both

**Medium. Verified.**

Core respawns Neovim in its launch directory. macOS puts the tab's directory back with a `chdir` step (`LivenessSeam.swift:137`). GNOME has no such step: nothing in `crates/wolf-gnome/src` sends `Chdir`. A tab moved by a typed `:cd` reverts after any crash, and a reviewer traced the worst case, a tab that left the empty state by `:cd`, to a tab that restores empty at the next launch. `notes/feat-list.md` claims parity.

Both platforms allow one automatic restart, and both reset that budget as soon as a respawn succeeds (`crates/wolf-gnome/src/shell.rs:1669`, `TabCore.swift:165`). If Neovim dies because of what it has open, the cycle of crash, respawn, reopen and crash never ends, and a background tab loops with no overlay.

**Change.** Have `WolfCore::restart_nvim` re-apply the captured working directory and reopen the captured files itself, since it owns both mirrors; that deletes `restoreSteps` and `reopen_order` from the front-ends. Count restarts per tab over a time window and stop after a few.

### C5. A content search can crash the app

**Medium. Verified by reading.**

`match_from_event` strips the line ending from a matched line but copies ripgrep's match ranges unclamped (`crates/wolf-core/src/search.rs:464`). A reviewer checked ripgrep 15.2.0 on a file with Windows line endings: for the pattern `world.*` the range ends one byte past the stripped line. `context_window` then computes `char_count - match_end_char`, which underflows, and indexes past its boundary table (`crates/wolf-core/src/search_tree.rs:181`).

So a regex search that runs to the end of a line, in a file with such line endings, on a line longer than the sidebar's budget, panics. On macOS that panic is inside a bridged call, which aborts the process (A6).

**Change.** Clamp each range to the stripped line's length and drop empties. Add the case to `crates/wolf-core/tests/search_projection.rs`.

### C6. Text input gaps

**Medium. Verified for the code; the delivery path is standard AppKit and GTK behaviour, not run.**

- **macOS.** `PaneContainerView` conforms to `NSTextInputClient`, and `insertText` discards its text unless a terminal pane is active (`PaneContainerView.swift:3465`). Editor panes translate keys from the keyboard layout only. So the emoji picker and dictation insert nothing into the editor, and an input method cannot compose there, which rules out Chinese, Japanese and Korean. The decisions note records "only a terminal pane" with no reason, and it is not a listed limitation.
- **macOS.** Keypad Enter does nothing in any pane: only `0x24` is mapped in `KeyMap.swift`.
- **GNOME.** `gdk_key` drops Insert, the keypad navigation keys and F13 upward (`crates/wolf-gnome/src/input.rs:78`), and core's `Key` has no `Insert`.

**Change.** Send `insertText` to Neovim when no terminal is active. Map keypad Enter and the keypad navigation keys onto the existing named keys, and add `Key::Insert` in core. Either route insert-mode keys through the input method on macOS, or list the gap under "Present limitations" with its reason.

### C7. The captive Neovim's command interception is leaky

**Medium. Tested by the `init.lua` reviewer.**

`init.lua` intercepts `:q`, `:split` and their relatives with about 34 exact-word command-line abbreviations (`runtime/nvim/init.lua:685`). An abbreviation matches one spelling.

- `:qu`, `:qui`, `:exi`, `:wqal`, `:conf q`, `:w|q` and `<C-w><C-q>` reach Neovim's own quit. On the last pane that exits Neovim, which the supervisor reports as a crash, and every `wolf --wait` ends with `1`.
- `:vsp file` and `:spl` split natively with no layout intent, leaving a window no pane shows.
- `:vs file`, the commonest form with an argument, fails with a Lua error.

Three related holes sit beside it:

- `:q!`, `:wq!` and `ZQ` on a tab's last pane skip the unsaved check entirely (`runtime/nvim/init.lua:917`), so they act as `:qa!` and discard every hidden modified buffer. Opening files from the sidebar hides the previous buffer, so hidden modified buffers are the normal state.
- `wolf_terminal_exited` counts raw windows where every other close path asks `closable()` (`runtime/nvim/init.lua:1470`). Typing `exit` in a lone terminal pane with the drawer open closes the tab's last pane.
- `wolf_rebuild_layout` and `wolf_open_split_left` isolate no error per leaf, so one file that fails to load loses every window handle of a restore and leaves windows no pane shows.

**Change.** Replace the abbreviations with one command-line `<CR>` mapping that parses the line with `nvim_parse_cmd` and rewrites by command, bang and arguments; the reviewer checked that it resolves every form above. When forced, still refuse on a modified buffer shown in no window, as native `:quit!` does. Use `closable()` in `wolf_terminal_exited`. Wrap each leaf's load in `pcall` and always return the handles built so far.

### C8. A terminal resize can be dropped for good

**Medium. Suspected: the code is verified, the diagnosis is not.**

`Session::resize` returns early when the requested size equals Wolf's own record of it (`crates/wolf-core/src/terminal/session.rs:397`), not the size the kernel holds. `notes/retrospectives/close-followups-5.md` records a test in which `stty size` printed `24 80` after a resize to 100 by 30, one run in five. If a shell's own start-up resets the window size after Wolf set it, every later resend is deduplicated away and the pane's programs draw at 80 by 24 until the pane changes size again.

A front-end resizes a session straight after opening it, which is the timing of the failing test. This has been filed as a flaky test four times; it reads like a product defect.

**Change.** Compare against `TIOCGWINSZ` on the master, or open the pty at the pane's real size so no resize races shell start-up.

### C9. Start and restart have no time bound

**Medium. Verified by reading.**

`WolfCore::start` runs the spawn, the attach and thirteen setup steps under `runtime.block_on` with no timeout (`crates/wolf-core/src/api.rs:446`), on the UI thread. `init.lua` loads the user's `init_user.lua` last. Its errors are caught; a hang is not an error. An infinite loop or a start-up prompt there blocks `start` for ever, and with session restore the app can then never launch. `restart_nvim`, the recovery path, hangs the same way.

A related leak: when a setup step fails during a restart, the half-built Neovim is dropped but not killed, because nothing sets `kill_on_drop`. It stays attached to the shared snapshot, and when it eventually exits its supervisor marks the healthy replacement `Dead` (reported).

**Change.** Wrap the chain in a timeout, kill the child on elapse and return an error. Set `kill_on_drop(true)`, and kill the half-built process on the error path.

### C10. Saved state can go stale or unreadable without notice

**Medium to low. Verified for the writer; the rest reported.**

- The app-state writer ignores whether its write succeeded and records the tree as written (`crates/wolf-core/src/app_state_store.rs:235`). `submit` skips a tree equal to the last one. So after one failed write, every identical submit, the one at quit included, is skipped, and `flush` reports success with stale state on disk.
- A non-finite weight or share serializes as `null`, which does not parse back as a number, so one bad value makes the whole saved session unreadable at the next launch.
- A healed database is unlinked under the other live connections, which go on writing to the deleted file until quit.
- `swap_dir_for` names the durable swap directory with `DefaultHasher` (`crates/wolf-core/src/state.rs:53`), whose algorithm the standard library leaves unspecified. A toolchain bump could orphan every project's swap files, which are the crash-recovery data.

**Change.** Clear the baseline when a write fails. Sanitize non-finite values in `submit`. Share one connection per database (P1). Hash the path with a fixed function and pin a test vector.

### C11. Smaller correctness findings

| Finding | Where | Status |
| --- | --- | --- |
| **GNOME overlays act on a stale list.** The picker, palette and buffer picker re-rank on `search-changed`, which GTK delays by 150 ms, and activate on Enter. Enter inside that window activates the previous list's top row; in the palette that is the most recently used command, which can be Close Tab. | `crates/wolf-gnome/src/palette.rs:636`, `picker.rs:415`, `buffer_picker.rs:469` | Verified |
| **A close confirmation sticks.** macOS sets `closeConfirmed` before `performClose` and never clears it, so a declined close leaves every later close of that tab unchecked. GNOME confirms the tab that queued `close_tab` and then closes the selected one. | `WolfWindowController.swift:460`, `crates/wolf-gnome/src/shell.rs:1875` | Verified |
| **File opens can be stranded.** On macOS a restored drawer that fails to start leaves `pendingDrawerRestore` set for good, so every later `wolf FILE` for that tab queues and `wolf --wait` never returns. | `PaneContainerView.swift:2704`, `:2532` | Verified |
| **Closing a pane resets every divider on GNOME.** `normalize` re-equalizes each split in the tree; macOS keeps proportions and its note says that is what lets a resized layout survive a close. The Swift doc comment still says "re-equalize". | `crates/wolf-gnome/src/layout.rs:656`, `PaneTree.swift:436` | Verified |
| **Smallest pane.** A divider drag stops at 10 columns by 3 rows on macOS and at one cell on GNOME, and that weight persists. | `PaneContainerView.swift:82`, `crates/wolf-gnome/src/editor_inset.rs:247` | Verified |
| **Save As is not pinned to its buffer on GNOME.** The dialog's completion writes whatever buffer is current then, always with `saveas!`. Paste already pins its target at the press. | `crates/wolf-gnome/src/shell.rs:1433` | Reported |
| **A dead pane can never be removed.** Both front-ends detect a closed pane only through a bound grid, so a pane whose window closed before its grid resolved stays, and the last leaf can become a zombie. | `PaneContainerView.swift:2877`, `crates/wolf-gnome/src/pane_view.rs:1965` | Suspected |
| **A tab dragged to another window on GNOME.** No `page-attached` handler exists, while the tab registry is per window; comments in `tab_bar_slot.rs` acknowledge the drag. The reviewer traced keys swallowed and a leaked core. | `crates/wolf-gnome/src/tab_view.rs` | Suspected |
| **"Prefer tabs: Always" on macOS** may merge every restored window into one tab group, which is then saved. | `WolfWindowController.swift:300` | Suspected |
| **The preview covers the command line.** A web view is added above the command-line, toast and banner overlays; terminal views are placed below them. | `PaneContainerView.swift:2058` | Suspected |
| **A failed preview render blanks the pane** and is recorded as loaded, so it stays blank until the next edit. Both platforms. | `PaneContainerView.swift:1911`, `crates/wolf-gnome/src/pane_view.rs:2445` | Reported |
| **A preview keeps its first source path.** After Save As on a new file, the preview pane still names the old path and restores empty. | `runtime/nvim/init.lua:1971` | Tested by a reviewer |
| **The buffer picker jumps to line 1** when Enter is pressed on the current buffer, which it lists first. | `runtime/nvim/init.lua:1714` | Tested by a reviewer |
| **Search and the picker disagree on what a project contains.** The walk ignores `.gitignore` outside a repository and ripgrep does not; six Markdown extensions preview, two are searched. | `crates/wolf-core/src/files.rs:328`, `search.rs:230` | Reported |
| **The GNOME command line drifts.** It is drawn as one Pango run with a cell-positioned caret, and a long command is clipped with the caret off screen. | `crates/wolf-gnome/src/pane_view.rs:3909` | Reported |
| **`eprintln!` can abort GNOME.** `SIGPIPE` is ignored, so a write to a closed stderr panics inside a GTK callback. `frame_metrics.rs` states the rule that about 70 other sites ignore. | `crates/wolf-gnome/src/frame_metrics.rs:44` | Verified (count) |
| **GNOME handles no termination signal,** so logout or `kill` skips the unsaved check. | `crates/wolf-gnome/src/app.rs` | Verified |
| **A timed-out call still runs later.** The request is already in Neovim's queue, so a timed-out terminal bind, history move or layout rebuild takes effect after the caller has taken its fallback. | `crates/wolf-core/src/api.rs:1842` | Reported |

## Races

No reviewer found a data race, a lock-order cycle, or a guard held across an `.await`, and the documented locking rules match the code everywhere they were traced. The races that exist are logical: two things that should happen in one order can happen in the other.

### R1. A paint can read half of a redraw

**Medium. Verified for the mechanism; visible tearing is suspected.**

`apply_redraw` takes the snapshot's write lock once per `redraw` notification and releases it whether or not that notification carried `flush` (`crates/wolf-core/src/redraw.rs:512`). Neovim's own documentation says several batches may precede the `flush`, and that the user should see only the final state. Between batches the lock is free and the snapshot is mid-frame.

Core wakes only on `flush`, but other things paint: a terminal publish every 8 ms, any `wolf_*` notification, the macOS scrollbar fade timer, a live resize. Any of them can read a scrolled grid whose new rows have not arrived. GNOME also takes one read guard per pane, so one frame can paint two panes from two states. [`architecture.md`](../specs/architecture.md) calls the snapshot "a complete, paint-ready frame", which does not hold between batches.

**Change.** See A1: commit a frame at `flush`.

### R2. The snapshot's writer spins, and every keystroke takes the write lock

**Medium. Verified.**

`write_async` is a loop of `try_write` and `yield_now` (`crates/wolf-core/src/render.rs:1416`). It never registers as a waiting writer, so while any reader holds the snapshot a tokio worker runs at full load, and a writer can wait out a whole paint pass. GNOME holds a read guard for each pane's entire paint (`crates/wolf-gnome/src/pane_view.rs:3214`).

`clear_dismissable_toast` takes that write lock for every key, mouse or scroll batch, toast or no toast (`crates/wolf-core/src/pump.rs:163`). So a keystroke is not forwarded to Neovim until an in-progress paint ends. A terminal pane does the same per input event, through a spawned task.

**Change.** Check for a toast under a read lock, or an atomic flag, before taking the write lock. Replace the bare retry with a bounded parked attempt. A1 shortens the read side to a pointer swap.

### R3. A click's focus is undone by the next unrelated wake

**Medium to low. Verified for the logic on both platforms; frequency not measured.**

A click sets the active pane at once and queues `FocusWin`, which the pump sends at least 8 ms later. Any wake that lands first re-derives the active pane from Neovim's still-old `active_win` and sets it back; the next wake flips it again (`PaneContainerView.swift:2922`, `crates/wolf-gnome/src/pane_view.rs:3394` and `:2099`).

Keys typed in that window go to the wrong pane when one side is a terminal. Each flip also runs a blocking cursor capture and reports a terminal focus loss, which forces a file-tree rescan. With a busy terminal in the tab there is a wake every 8 ms, so this happens on most clicks. GNOME already has the guard, `pending_focus`, but only a placed file sets it.

**Change.** Hold the requested window in a small seam and ignore an `active_win` that predates the request. On GNOME, set `pending_focus` from the click.

### R4. Smaller races

| Race | Where | Status |
| --- | --- | --- |
| **A cancelled search can leave ripgrep running.** `cancel` kills only a child already published; the worker checks the flag only after ripgrep emits a line. A session dropped during spawn, with a query that matches nothing, runs to completion on every core. | `crates/wolf-core/src/search.rs:191`, `:286` | Verified |
| **A blocking query overtakes queued input.** Input reaches Neovim at least 8 ms late; a blocking query is written at once. Copy can yank before the last selection key lands, and `open_preview` after a queued focus move acts on the previous window. `wait_on` and `bind_terminal` were already moved onto the queue for this reason. | `crates/wolf-core/src/api.rs`, 25 `block_on_rpc` sites | Reported |
| **A session that exits before its placeholder binds leaves a dead pane,** in which even `<C-w>c` does nothing. The queued bind neither rechecks liveness nor reads its result. | `crates/wolf-core/src/terminal.rs:662`, `input.rs:866` | Reported |
| **A colour change during a terminal spawn is lost** for the new session, and the equality guard stops a later identical push from repairing it. | `crates/wolf-core/src/terminal.rs:324` | Reported |
| **Close-on-exec is set after `openpty` and `pipe`,** so another thread's fork in between inherits the pair. Linux has atomic forms. | `crates/wolf-core/src/terminal/pty.rs:78`, `:327` | Verified |
| **The pump's probe ignores `blocking`.** Any reply to `nvim_get_mode` is read as "waiting on a key", so a call that pumps the event loop without waiting releases a key past held state events. | `crates/wolf-core/src/pump.rs:306` | Verified (code), Suspected (effect) |
| **The recency stores document one transaction and run three statements,** so two connections can stamp the same sequence number. | `crates/wolf-core/src/recent_dirs.rs:84`, `command_mru.rs:87` | Verified |
| **Closing a window that is not current leaves a stale preview entry,** and a `:bd`-ed file stays in the open-file mirror, because both emits fire before the state changes. | `runtime/nvim/init.lua:2245`, `:1639` | Tested by a reviewer |

## Scale: windows, tabs and panes

Every tab owns a Neovim, a core and a runtime. That choice is documented and sound, but several costs around it were sized for one tab. They fall into three groups: what a tab costs while idle, what a wake costs, and what runs serially on the UI thread once per tab.

### P1. What one tab costs

**Medium to high. Verified for the counts; the descriptor limit is suspected.**

| Resource           | Per tab                                                                                | Why                                                                                            |
| ------------------ | -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Threads            | One tokio worker per CPU core, plus a file-watch thread, plus two per terminal session | `Builder::new_multi_thread()` with the default worker count, `crates/wolf-core/src/api.rs:343` |
| SQLite connections | Three, on the one `wolf.sqlite`                                                        | `RecentDirs`, `CommandMru` and `MruStore` each open their own, `api.rs:369`                    |
| Watches            | One recursive watch of the project root                                                | One `SearchRegistry` per core, `api.rs:386`                                                    |
| File listings      | One full listing per tab, and one more per open picker                                 | Sessions share nothing across cores                                                            |
| File descriptors   | About 11 for the core, plus 5 per terminal session                                     | Pipes, SQLite, the runtime, pty and wake pipes                                                 |
| Web processes      | One per preview pane, kept for background tabs                                         | No shared process or session                                                                   |

Twenty tabs on a ten-core machine is about 240 idle threads, 60 connections to one database and 20 watchers on what is often the same directory. On Linux those are 20 inotify instances with a watch per directory each.

Nothing raises the descriptor limit: there is no `setrlimit` anywhere. A reviewer observed a soft limit of 256 on this Mac, which a few tens of tabs with terminals would reach. Past it, store opens fail silently, terminals fail to open, and two accept loops spin (`crates/wolf-core/src/open_channel.rs:482`, `browser_preview.rs:150`).

**Change.** Three small steps first: `.worker_threads(2)` on the per-tab runtime, raise `RLIMIT_NOFILE` once in `WolfCore::start` beside the `SIGPIPE` call, and open the three stores on one connection. Then make the stores, the watcher and the walked listing process-wide services keyed by root, as `AppStateStore` and the open listener already are.

### P2. On macOS every wake repaints every cell, through the bridge, one cell at a time

**High. Verified for the calls; timings are the project's own measurement and a reviewer's extrapolation.**

- **Scalars are read by packing whole grids.** The bridge has no accessor for a grid's size, the default colours, the cursor's grid or the viewport, so Swift calls `read_frame_for_grid`, which resolves and copies every cell (`crates/wolf-ffi/src/lib.rs:6356`). Per wake that is grid 1, the largest, about four times, and each pane's grid two or three times (`PaneContainerView.swift:1372`, `:1384`, `:1661`, `:2732`, `:2960`, `:3291`).
- **Each painted cell is copied three times** (the pack, `cells()` at `crates/wolf-ffi/src/lib.rs:6281`, `text()` at `:4327`) and costs five to nine bridge calls. The cursor draw clones the whole vector again for one cell (`PaneContainerView.swift:3070`).
- **Each non-empty cell builds a dictionary, an attributed string and a `CTLine`** with no cache (`GlyphPainter.swift:52`). A blank cell holds a space, so it pays too.
- **Nothing is skipped.** `draw` ignores its dirty rectangle and every wake sets `needsDisplay` on the whole view (`PaneContainerView.swift:1570`). One changed cell costs the whole window.

The project's own note measures 22 ms for one grid of a 46 ms draw. A reviewer extrapolates 60 to 100 ms per frame for six panes on a large display. Terminal frames already have the right shape: a shared immutable frame, a version per row, a fetch per row.

**Change.** Cheapest first: reuse the cell vector for the cursor; add scalar accessors; skip the `CTLine` for a plain space and the fill for a default background. Then A1.

### P3. Wakes are too many, and each one does everything

**Medium to high. Verified for the mechanisms; the per-keystroke counts are a reviewer's.**

- **macOS does not coalesce.** Each wake is its own `DispatchQueue.main.async` block running the full pass (`WakeBridge.swift:17`). GNOME's one-slot channel collapses a burst (`crates/wolf-gnome/src/tab_view.rs:102`). When the main thread is busy, macOS replays identical passes against the same snapshot.
- **Terminal publishes ride the render wake.** A session publishes up to 125 times a second, and the front-end cannot tell that wake from an editor flush. `yes` in the drawer repaints every unchanged editor pane on both platforms, which defeats the terminal's own row damage.
- **Hidden things keep waking.** A closed drawer, a zoomed-away pane and a background tab's terminal all keep building frames and firing wakes, and a background tab runs its whole reconciliation for each one.
- **Core wakes whether or not anything changed.** Every `wolf_*` handler ends in the wake (`crates/wolf-core/src/redraw.rs:344`). One insert-mode keystroke in a Markdown buffer costs four write locks and three wakes, two of which matter only when a preview shows that buffer. One `:edit` sends three identical `wolf_current_file` notifications.
- **GNOME repaints all panes per wake** (`crates/wolf-gnome/src/pane_view.rs:2152`), at a measured 9.5 ms for one 90 by 34 pane, so two panes already miss a 60 Hz frame.

**Change.** Coalesce at the bridge with a pending bit per core. Give the snapshot a generation that every writer bumps and a per-grid stamp, and expose both, so a front-end can skip the editor pass on a terminal publish and skip an untouched pane. Compare before assigning a wholesale map and skip the wake when equal. Emit the Markdown cursor and dirty notifications only when a preview of that buffer exists. Add a per-session "shown" flag so a hidden terminal feeds its emulator but builds no frame.

### P4. A terminal publish rebuilds the whole grid, twice

**Medium. Verified.**

`screen()` walks every row and cell on each publish, allocating a 32-byte buffer per cell (`crates/wolf-vt/src/lib.rs:1092`, `:1244`), and `build_frame` then clones every cell again (`crates/wolf-core/src/terminal/session.rs:617`). Row dirtiness is read only to stamp versions, never to skip work, and any byte from the pty marks the session dirty. A reviewer counts seven library calls per cell and estimates 1 to 3 ms per frame for a 200 by 50 pane, all under the session lock that key input waits on.

libghostty-vt offers a dirty-row iterator that Wolf does not use. Scrollback is the library's default, which a reviewer reads as about one page; Wolf never sets it and no document records the choice.

**Change.** Read only dirty rows, share clean rows between frames, drop the intermediate copy, and skip the publish when nothing changed. Set scrollback from a named constant and record the memory bound it implies.

### P5. Everything per tab is serial on the UI thread

**Medium. Verified.**

| Path               | What runs, once per tab, in series                                                                                                                                                            |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Start              | A spawn, an attach and 13 setup steps, 26 round trips in all. Twelve of them fetch Neovim's whole API description to read one channel id (`nvim_get_api_info` appears 35 times in `nvim.rs`). |
| Launch             | Every saved tab's start, before the first frame. Unbounded on macOS (C3). macOS also starts a full directory walk for every folder tab, sidebar shown or not.                                 |
| Close Window, Quit | One unsaved check per tab, one cursor capture per edit pane, then one shutdown per tab.                                                                                                       |
| Shutdown           | `qa!` bounded by 2 s, a wait bounded by 5 s, a kill, and up to 1 s for terminals.                                                                                                             |

Each blocking call is bounded by the 5 s timeout, so the worst case multiplies: a reviewer puts Quit with 30 unresponsive tabs at about four minutes of frozen UI, and closing one wedged tab at 12 s. The exits are independent, so the sum could be a maximum. Closing a window of K tabs also captures K(K+1)/2 times on macOS, because each close re-captures every tab still open.

**Change.** Split shutdown into a non-blocking "begin" sent to every core and one bounded join with a shared deadline. Fetch the channel id once and run setup as one Lua call. Capture a tab group once per burst. With C1's change, a non-`Alive` tab is killed without the `qa!` wait.

### P6. Content search has no ceiling at any layer

**Medium to high. Verified on macOS; reported for core and GNOME.**

Core passes ripgrep no match or file-size limit and appends every match, keeping the whole matched line. Each batch of 128 matches bumps the version.

On macOS each bump re-projects every match so far across the bridge, about 15 calls per match, rebuilds every row object and calls `reloadData` (`SearchView.swift:174`). The total work is quadratic in the number of matches, on the main thread, and the Rust-side cache meant to prevent it is never asked twice for one key. On GNOME each bump re-projects everything too, and the drain loop never yields to GTK, so the window does not paint and Escape cannot be delivered while ripgrep outruns it. GNOME caps the rows it draws at 200; neither caps the work.

Regex is on by default, so `.` with all files on is enough to hang the window.

**Change.** Cap matches in core: stop ripgrep and set a `truncated` flag the panel can show. Pass ripgrep the walk's limits. Store a bounded window of each line. Give both front-ends one capped projection from core, and delete the bridge-side cache.

### P7. The file listing is walked, held and re-derived more than once

**Medium. Reported, with the rescan rule verified.**

- **A truncated listing re-walks for ever.** `needs_rescan` reads "absent from the listing" as "new file" and never consults `truncated` (`crates/wolf-core/src/files.rs:478`). When a walk stopped at the 50,000-entry cap or the time budget, every create or modify of an unlisted file triggers a full walk that produces the same truncated list. A home-directory root on macOS sees constant such writes.
- **Each streamed batch re-derives the whole view on the UI thread.** A 50,000-file walk is about 196 version bumps. Each rebuilds the sidebar's tree index, which a reviewer puts at about two million allocations at the cap.
- **The GNOME picker ranks with the unbounded ranker,** cloning and sorting every match per keystroke, though core has `rank_matches_top_n` for exactly this.
- **The macOS picker resolves 200 Finder icons per refresh** for about nine visible rows (`PickerView.swift:227`).
- **A burst such as `git checkout` re-walks back to back,** once per tab, because the debounce fires on a fixed leading edge.

**Change.** Rescan a truncated session only on a forced event. Flush on a timer, not a fixed count. Build the tree index on the worker and publish it with the version. Use the bounded ranker. Resolve icons per visible row and cache by type.

### P8. The Markdown preview

**Medium. Verified for the cadence and the loop; the rest reported.**

- **macOS renders about once per keystroke.** The blocking `render_preview` is throttled to 33 ms (`PickerRefreshCoalescer.swift:19`). GNOME measured that call as the largest single cost on its typing path and moved to about two renders a second. The two platforms now make opposite trade-offs, and nothing records the difference.
- **Each macOS render copies the whole document across the bridge,** about 1 MB of it constant inlined fonts, then discards everything but the body (`PaneContainerView.swift:1915`).
- **Two loops in the renderer are quadratic.** Marker placement restarts from the line start for every 100-column marker (`crates/wolf-core/src/markdown.rs:636`), so a line holding an inline base64 image costs seconds. A long fenced block filters all spans per row.
- **A slow render is reported as a hung Neovim.** The render runs inside the RPC timeout, so one over 5 s marks the tab `NotResponding`, and the timed-out task keeps running while each further edit starts another.
- **Code-block highlighting is uncached:** a reviewer measured 33 ms inside Neovim for a 1,000-line block on every render.
- **Each preview pane is its own web process,** kept alive for background tabs.

**Change.** Return the head once and the body per render from core (A2), and put one render coalescer in core for both front-ends. Walk each line once. Run the render under `spawn_blocking`, outside the liveness accounting. Cache highlights by language and text.

### P9. Per-wake work that grows with tabs or buffers

| Cost | Grows with | Where | Status |
| --- | --- | --- | --- |
| macOS rewrites the title and proxy icon of every tab in the group on every wake of any tab. | Tabs in the group | `WolfWindowController.swift:371`, `AppDelegate.swift:1068` | Verified |
| GNOME re-gathers every tab of the window on every wake, with a `stat` per terminal pane on the main thread. A shell whose directory sits on a dead network mount freezes every window. | Tabs, terminal panes | `crates/wolf-gnome/src/shell.rs:1807`, `crates/wolf-core/src/terminal/session.rs:430` | Reported |
| Each focus or buffer event resolves the real path of every listed buffer on core's notification loop, twice. A reviewer measured 68 ms per switch in Neovim alone at 2,000 buffers. | Buffers ever opened in the tab | `crates/wolf-core/src/redraw.rs:923`, `runtime/nvim/init.lua:1639` | Measured by a reviewer |
| GNOME sends an undeduplicated global resize for every pane's resize signal, so a divider drag queues at least two per frame ahead of the next keystrokes. | Panes | `crates/wolf-gnome/src/pane_view.rs:3352` | Verified |
| Recency writes run synchronously on the redraw handler and the input pump, each able to wait 2 s on a busy database. | Tabs writing at once | `crates/wolf-core/src/redraw.rs:1132`, `pump.rs:154` | Reported |

## Structure and coupling

### A1. Publish editor frames, as terminal frames are published

**The one structural change that pays for itself several times. Design recommendation.**

A terminal session builds an immutable frame, swaps it in behind an `Arc`, stamps a version per row, and wakes. A reader takes the frame in constant time, holds no lock while painting, and repaints only the rows whose version moved.

The editor grid has none of that. The redraw handler mutates one shared snapshot in place, readers hold its lock while they paint, there is one global `seq`, and nothing says which grid or row changed. That single difference is the root of R1 (torn frames), R2 (the spinning writer), P2 (the macOS repaint) and much of P3 (every pane repainted per wake).

**Change.** Buffer redraw events until `flush`, then publish each touched grid as an immutable frame with a version per row and a stamp per grid. Expose it over the bridge the way `FfiTerminalFrame` is exposed, with flat arrays in place of one object per cell. Both front-ends then invalidate by row, the read lock becomes a pointer swap, and a front-end can tell a terminal publish from an editor flush.

### A2. The front-ends know things only core should know

**Medium. Verified for the cases checked.**

Decision 1 of [`architecture.md`](../specs/architecture.md) promises one front-end-facing surface. In several places each front-end has instead learned how core works and re-derived a rule from it:

| What the front-ends know                                                                                                                   | Consequence                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| The byte shape of the rendered preview document. Both scan for `<body` and the last `</body>`, and insert a style after the first `<html`. | The stylesheet spec forbids angle brackets in comments to keep the scan safe, and the whole document crosses the bridge to be cut apart (P8). |
| `MARKER_COLUMN_STRIDE`, mirrored by hand in Swift.                                                                                         | A change on one side silently breaks scroll sync.                                                                                             |
| That a restarted Neovim returns to its launch directory.                                                                                   | macOS compensates; GNOME does not (C4).                                                                                                       |
| That only an `Alive` core should be asked before a close.                                                                                  | The gate is written in each front-end; core's two request calls issue the call regardless.                                                    |
| What a saved session may contain and how much of it to restore.                                                                            | Two policies (C3).                                                                                                                            |
| Which preview links are safe to follow.                                                                                                    | Two classifiers with different answers (S3).                                                                                                  |
| That one printable character sent to a terminal must be a key event, so core's `<C-w>` router sees it.                                     | The rule is written in both.                                                                                                                  |

Each row is a rule with a precedent for hoisting next door: `file_open::choose_pane`, `app_state::restored_tab`, `tab_rebuild_spec`, `pane_status` and `rank` all live in core, and no reviewer found drift in any rule that had been hoisted.

**Change.** Move each row into core. The preview cluster is the best first step: return the head once and the body per render, own the root-style insertion and the page script, classify links, and coalesce renders. It removes a security divergence, a megabyte copy per render and a hand-mirrored constant, and it would give GNOME scroll sync from the script macOS already runs.

### A3. Process-wide things are owned per tab

**Medium. Verified.**

The recency stores are documented as global, the filesystem is process-wide, and a project's file listing is the same for every tab on it. Each is nonetheless created inside `WolfCore::start`, so each exists once per tab (P1). The process already has the right pattern in two places: `AppStateStore` and the open-channel listener are process-level, with reasons recorded.

**Change.** Introduce a process-level services object that owns the stores, one watcher and the walked listings keyed by root, and hand each core a cheap handle. The runtime can stay per tab with two workers, because shutdown relies on dropping it.

### A4. The largest units

Functions are not the problem. Of about 2,900 functions only 22 exceed 100 lines, and the longest is a 290-line GTK `build`. The weight is in a few files and types that have collected unrelated state.

| Unit | Size | What it holds | Split along |
| --- | --- | --- | --- |
| `crates/wolf-gnome/src/pane_view.rs` | 5,409 non-test lines; `PaneInner` has 36 fields | Twelve responsibilities, from the wake order to file placement. Pane kind is encoded as optional fields, re-branched in every handler. | Message surfaces; the `Paned` tree and divider guards; the preview; pointer; terminal glue; and a pure pane model (A5). |
| `PaneContainerView.swift` | 3,543 lines, 69 stored properties, 133 functions | The same twelve. 19 dictionaries keyed by one window handle, cleared by hand in two lists. | One object per preview pane; a terminal coordinator; a per-wake scene value read by draw and hit-testing; the same pane model. |
| `crates/wolf-ffi/src/lib.rs` | 6,653 lines in one file | About 650 lines of logic among 5,000 of adapters. Each call is described three times: in the bridge module, on the method, and in the README. | Keep the single bridge module so generated output stays byte-identical; move the glue into submodules. `BUILD.bazel` names `src/lib.rs` and needs a glob. |
| `AppDelegate.swift`, `WolfWindowController.swift` | 1,250 and 1,156 lines | Two classes that reference each other concretely across about 47 members. Only `TabCore` has a protocol boundary, so only it has a live test. | A session committer, a close coordinator, an open router, a menu controller, a launch restorer; a protocol for the controller's twelve callbacks. |
| `crates/wolf-core/src/api.rs` | 2,302 non-test lines, 82 public methods | Mostly delegation; the real logic is `start`, the restart and `block_on_rpc`. About 1,000 lines are comment, with one timeout paragraph repeated about 20 times. | An `NvimSession` owning the runtime, process, liveness and timeout, with one `query` helper for the 25 hand-written wrappers. |
| `crates/wolf-core/src/redraw.rs` | `RedrawHandler::new` takes ten collaborators | The render fold also does persistence, terminal control and wait bookkeeping. It defines `WakeFn`, `TrackedCwd` and the queue types, so nine other modules import `redraw` for them. | Decode notifications into a typed event in one place and dispatch to owners. Move the shared primitives to their own module. |
| `runtime/nvim/init.lua` | 2,352 lines, about 60 globals | Internal state is global; some globals exist only after their setup step ran. | A `lua/wolf/` tree along the existing seams, which the bundle already supports for the colorschemes. |

### A5. The pane model is written on both platforms and tested in none of its glue

**Medium. Reported by three reviewers independently.**

The pure pane tree exists in Swift and in Rust (D1). The logic that binds it to Neovim does too: ensuring a first pane, applying intents, detecting closes, resolving grids and following the active window. On macOS that is a hand port of GNOME's functions into `PaneContainerView`. In both it lives in methods that hold widgets, so it has no headless test, and both decisions notes say such paths are covered by smokes or by hand only.

That untested band is where R3, the zombie panes and the stranded file opens of C11 all sit. Beside it, each front-end drives launch and restart restore with several `pending` fields advanced from many functions, an implicit state machine with no failure exit.

**Change.** One pure pane model that owns the tree, the bindings, the active pane, the drawer and the zoom. Its transitions take a snapshot view and return effects, such as "focus this window" or "capture that pane", for thin glue to perform. Make the restore sequence an explicit enum with a failure exit. In a shared crate it serves both front-ends (D1); even per platform it turns about 500 lines of glue into testable code.

### A6. The bridge's contract has four gaps

| Gap | Detail | Status |
| --- | --- | --- |
| **A panic aborts every tab.** | The release profile is `panic = "unwind"`, the shims are `extern "C"`, and no crate uses `catch_unwind`. A panic in any of 98 bridged methods is `SIGABRT` for every window. No reachable panic was found except C5. Funnel the methods through one helper that catches, logs, marks the core dead and returns the neutral value. | Verified |
| **Nothing checks the committed bindings.** | Swift compiles against the committed header, and C linkage carries no types. A signature changed in `lib.rs` without regenerating still compiles and links, then corrupts arguments. Generate into a temporary directory in a test and compare. | Verified |
| **Application state crosses through a process-global builder.** | About 860 lines, 16 free functions and five discriminant tables feed a scratch tree post-order, and three pure queries read it. It is safe only because every caller re-feeds first. A serialized value would cross as one string. | Reported |
| **Discriminants are kept by hand in four places.** | Only `command_kind` is checked mechanically. Swift's decoders fail open: an unknown close answer decodes to "no answer", which closes unchecked. `AppMenu.Tag` is an unchecked fifth copy for menu items. | Reported |

The decisions note also says the wake fires on a tokio worker. It can fire synchronously on the main thread inside a bridged call, which is safe only because the Swift thunk always defers. That invariant should be written down.

### A7. One contract in two languages, checked by neither

**Low to medium. Reported; the arity check is the reviewer's.**

The Lua-to-Rust contract is 19 notifications and two requests. A reviewer checked every one and they agree today. Nothing enforces that: no test asserts that each helper core calls exists in `_G`, or that each notification `init.lua` sends has a decoder, and a mistyped helper name comes back as an error reply the pump discards (C2).

The per-window state arrives as three notifications with three delivery disciplines, one scheduled and suppressible and two immediate, which core joins by window handle while tolerating either order. Nine call sites re-emit by hand with comments calling the order load-bearing.

**Change.** One scheduled `wolf_windows` notification carrying everything per window. One test that walks both name lists.

### A8. A multi-tabpage model nothing uses

**Low. Verified by search.**

Both front-ends run one tabpage per Neovim. Core, the bridge and `init.lua` still carry `create_tabpage`, `set_current_tabpage`, `close_tabpage`, `cwd_for_tabpage`, two input events, per-tabpage payloads on hot emit paths, and a paragraph of scoping rules in the architecture spec. No front-end calls any of it.

**Change.** Delete it, and simplify the snapshot contract's tabpage rules with it.

## Duplication

### D1. Two thirds of the macOS seam logic has a Rust twin

**Medium, structural. Reported in detail; the cases cited were verified.**

The GNOME decisions note says several seams are deliberate ports of a macOS counterpart, "so the two front-ends can be compared as values". A reviewer paired every seam:

- About 2,700 of 4,080 Swift seam lines have a counterpart in about 3,550 lines of Rust.
- The tests are ports too: about 9,700 Swift lines against about 21,000 Rust lines, often with matching names.
- Keeping the state model in Swift costs a further 1,200 lines of codec and bridge builder.

A GUI-free Rust crate that both front-ends already depend on sits beside all of it.

The ports have no reference side, and nothing checks that they agree. The drift found in this review came from exactly this layer: C3, C4, S3, the divider and pane-size rows of C11, and the preview cadence of P8. Smaller cases: the scrollbar inset is 2 px on one platform and 6 px on the other while a README says the rules are the same, and `cursor_colors` is duplicated Rust to Rust between `wolf-gnome` and `wolf-ffi`, kept identical by a comment.

**Change.** Hoist by value for cost, not all at once:

1. The preview cluster (A2).
2. The crash-restart restore, into `restart_nvim` (C4).
3. The launch-restore policy (C3).
4. The close gate, into core's two request calls.
5. Terminal frame work: one call for the changed rows, and cells resolved in the frame.
6. The pane tree cluster and pane model (A5), in a new GUI-free crate under the same forbidden-dependency gate, so core's surface stays about Neovim.
7. The small arithmetic, such as cell metrics, statusbar layout and scrollbar thumb, in that same crate.

Keep native what is native: key tables and input methods, glyph attributes, window stacking, AppKit tab-group handling, and the dispatch tables.

### D2. One list view, written three times on each platform

**Medium. Reported, with diffs.**

On macOS the file picker, the command palette and the buffer picker share their layout constants, search bar, key handling and selection code verbatim; two of them differ in 128 lines, mostly comments. On GNOME the three overlays repeat about 200 lines each, and their bugs exist in triplicate: the stale Enter of C11, and a selection that scrolls out of view.

**Change.** One list component per platform, taking a row source and an activation closure.

### D3. Smaller duplication

| What                                                   | Copies                                                                                                            | Where                                                                        |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| The wake channel: a static, a setter and a thunk       | Five on each side of the bridge, four more in GNOME                                                               | `crates/wolf-ffi/src/lib.rs:1590`, the five `*WakeBridge.swift` files        |
| The `setup_*` function body                            | Twelve, differing in a helper name; the chain that calls them is copied into `start` and the restart              | `crates/wolf-core/src/nvim.rs`, `api.rs:449` and `:2136`                     |
| The recency store                                      | `RecentDirs` and `CommandMru` differ in a table name, a key and a limit; `MruStore` is a third variant            | `crates/wolf-core/src/recent_dirs.rs`, `command_mru.rs`, `mru.rs`            |
| Spawn a session, bind it, remove it on failure         | Four                                                                                                              | `crates/wolf-core/src/terminal.rs:579` and `:634`, `api.rs:1280` and `:1835` |
| The terminal cell, cursor and scrollbar types          | Three layers, field for field                                                                                     | `wolf-vt`, `wolf-core`, the bridge                                           |
| The snapshot-and-wake session state                    | Hand-copied between file search and content search; the note tells new code to "adopt" a pattern that has no type | `crates/wolf-core/src/files.rs`, `search.rs`                                 |
| The liveness gate before a blocking call               | Ten in one GNOME slice, which is how the tree's click came to have none                                           | `crates/wolf-gnome/src/`                                                     |
| Home-directory abbreviation; the "Preview: name" label | Three each                                                                                                        | core, `CwdTitleSeam.swift`, `title.rs`                                       |
| A real-Neovim test target                              | 124 hand-written stanzas, 3,019 lines of `BUILD.bazel`                                                            | T5                                                                           |
| Test helpers in `wolf-ffi`                             | `char_key` 26 times, `send_ex_command` 24, 179 hand-rolled poll loops                                             | T5                                                                           |

## Tests, gates and build

The suites are large, about 164,000 lines in test directories and 36,000 inline, and their foundations are good: a real pinned Neovim, real ptys, a real compositor, state isolation funnelled through one place, and every download pinned by hash. A reviewer checked mechanically that no test file is unwired. The gaps are at the edges.

| # | Finding | Detail | Status |
| --- | --- | --- | --- |
| T1 | **The two platforms build with Rust toolchains far apart.** | Bazel pins 1.85.0 (`MODULE.bazel:47`). The flake takes `stable.latest` (`flake.nix:35`), 1.99 by the retrospectives. No `rust-version` is declared, so Linux clippy can demand an API macOS cannot compile, and the `notify-types` hold exists only as a lockfile line. Pin one version on both sides. | Verified |
| T2 | **Nothing tests more than two cores or three tabs.** | The scale model has no automated signal for thread, descriptor or process growth. Add one test that starts about 16 cores, drives each, shuts them down in mixed order and checks the counts return to baseline. | Reported |
| T3 | **The security guards are thinly tested.** | No hostile file name is ever opened (S1). The preview server has no test for an empty root, a symlink leaving the root, or a foreign `Host` (S4, S5). | Reported |
| T4 | **Flaky tests have no owner.** | Four retrospectives ask for a ledger; `notes/known-flakes.md` does not exist. One of the flakes is probably C8. | Verified |
| T5 | **Test plumbing is copied, not shared.** | No Bazel macro, so a shared attribute means editing 124 stanzas. Swift test sources are literal lists where the same file globs its library. The `wolf-ffi` suite re-implements its harness in every file. | Reported |
| T6 | **Waits are sized by time.** | Fixed settles of 100 to 300 ms, about 660 fixed sleeps in the compositor smokes, and no cap on test threads, so a machine with N cores can run N squared Neovims. `wait_for_flush` reads its baseline after the input was sent. | Reported |
| T7 | **The gate has holes.** | No `--locked`, and `.bazelrc` sets `lockfile_mode=update`, so a gate can pass on a lockfile it rewrote. The release bundle and the Nix package are never built by a gate. macOS lint skips the test code. `wolf-ffi` tests that find no Neovim pass silently. | Verified in part |
| T8 | **Pins are synchronized by hand.** | libghostty-vt, Neovim, ripgrep, Zig and the grammar revisions are named separately for Nix and Bazel with no parity check. One `pins.json` read by both would remove about 1,400 lines of scanners. | Reported |
| T9 | **"Last verified at commit" is not verified.** | The checker tests only that the value is 40 hex digits. [`wolf-gnome-decisions.md`](wolf-gnome-decisions.md) is 176 commits behind its crate, [`wolf-core-decisions.md`](wolf-core-decisions.md) 99, and 30 of 72 notes still carry the commit that created the scheme. The files a reader is told to trust first are the stalest. Check ancestry and report the distance, or move the stamp to each section. | Verified |
| T10 | **Prose is growing where the limits do not apply.** | Plain comments, which the linter caps at two lines, are 2 to 5% of lines. Doc comments, which are exempt, are 27 to 29%. Each test file is described in its header and again in a 48 to 75 KB README. Of the last 400 commits, 132 are documentation and 110 are retrospectives, against 141 of code. | Verified in part |

## Documentation that disagrees with the code

The repository's rule is that such a disagreement is a defect in one of the two. These were found in passing; the code is right unless the row says otherwise.

| Document                                                   | Says                                                                        | Code                                                                                                                          |
| ---------------------------------------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| [`architecture.md`](../specs/architecture.md), decision 10 | No user-config source path is added                                         | `init.lua` loads `init_user.lua` last                                                                                         |
| [`architecture.md`](../specs/architecture.md)              | The snapshot is a complete, paint-ready frame                               | Not between redraw batches (R1)                                                                                               |
| [`application-state.md`](../specs/application-state.md) §5 | Launch recreates every window                                               | GNOME caps it (C3)                                                                                                            |
| [`wolf-core-decisions.md`](wolf-core-decisions.md)         | The browser preview is blank after a restart until the command is run again | Re-running reuses the server and its stale handle, so it never recovers. The code is wrong; give the server the shared handle |
| [`wolf-core-decisions.md`](wolf-core-decisions.md)         | Cancelling a search kills ripgrep                                           | Not before the child is published (R4)                                                                                        |
| [`wolf-ffi-decisions.md`](wolf-ffi-decisions.md)           | The wake fires on a tokio worker                                            | Also on the main thread inside a bridged call                                                                                 |
| [`wolf-gnome-decisions.md`](wolf-gnome-decisions.md)       | Three one-shot timers exist                                                 | Four                                                                                                                          |
| `crates/wolf-gnome/src/README.md`                          | Tabs never move to another window                                           | Comments in `tab_bar_slot.rs` describe a tab dragged in                                                                       |
| `crates/wolf-macos/Sources/WolfKeyMap/README.md`           | The scrollbar thumb follows the same rules as GNOME's                       | Different insets                                                                                                              |
| `notes/feat-list.md`                                       | macOS restores no more than GNOME after a crash                             | macOS restores the directory (C4)                                                                                             |
| `crates/wolf-core/src/input.rs`                            | `fnameescape` mirrors Vim's; a failed `saveas` is logged                    | Neither (S1, C2)                                                                                                              |
| `crates/wolf-core/src/browser_preview.rs`                  | `abort` is the path `shutdown` uses                                         | `abort` has no caller                                                                                                         |
| `crates/wolf-core/src/recent_dirs.rs`, `command_mru.rs`    | One transaction                                                             | Three statements. The code is wrong                                                                                           |
| `crates/wolf-core/src/nvim.rs`, `redraw.rs`                | `wolf_current_file` has two arguments; `wolf_preview` sends triples         | Four, and four-tuples                                                                                                         |
| `PaneTree.swift`                                           | A remove re-equalizes                                                       | It renormalizes proportionally                                                                                                |
| `runtime/nvim/README.md`                                   | User settings override every Wolf default                                   | The setup steps install their mappings afterwards                                                                             |

Several source comments also narrate history ("now also fires", "moved from `app.rs`") or count callers, which the repository's own rules forbid.

## Suggested order of work

**First, the trust boundary and the data-loss path.** Items 1 to 5 of [What to change first](#what-to-change-first). Each is small, each closes a way to lose work or run someone else's code, and none depends on another. Land a test with each: hostile file names, a close during a pending chord, a document carrying script, a request with a foreign `Host`.

**Second, the cheap scale relief.** Item 6, plus the descriptor limit. These are a few lines each and buy headroom while the larger work is planned.

**Third, published frames.** Item 7 is the largest single change and the one most other performance findings wait on. Do core first, behind the existing accessors, then the bridge accessors, then each painter.

**Fourth, one policy in core.** Items 8 to 10 remove platform differences by deleting the front-end copy of a rule, not by reconciling two copies.

**Fifth, structure.** Item 11, and the splits in A4. The pane model is worth doing before the files around it are split, because it decides where their state goes.

**Throughout, the gates.** Item 12. The toolchain pin and `--locked` are one-line changes; the many-tab test is what would have caught P1.

## What is done well

These are patterns to keep, and several are the reason a finding above has a small fix.

- **The one hard rule is gated.** Core has no toolkit in its graph on either build system, and a check fails if that changes.
- **The boundary is three shapes,** applied without exception: a snapshot under a wake, a drainable queue, a bounded query.
- **Decisions are pure and tested.** The seams are small, total and guarded against non-finite input. Matches over `Liveness`, `Command` and the layout intents have no wildcard arm, so a new variant fails to compile in both front-ends.
- **Locking is written down and the code matches it.** Decode before the lock, wake after it drops, no guard across an `.await`. Three reviewers traced every acquisition and found no cycle.
- **Timeouts detach, they do not drop,** with the nvim-rs reason recorded and matching the library's source. Liveness is lock-free transitions with exactly-once wakes.
- **The terminal layer is careful.** Library callbacks only push onto an effect queue. Frames are immutable with a version per row. The router is pure with an injected clock. Every `unsafe` block carries an invariant a reviewer could check against the pinned headers, and all of them held.
- **Paths reach Lua as arguments,** never interpolated, which is why S1 is confined to four lines of Rust and five of Lua.
- **Persistence fails safe.** Only a corrupt database is ever deleted, the application state is written in one immediate transaction, and nothing is committed when no view is live.
- **The open channel is defensive:** an inode check before unlink, and an acknowledgement always ahead of a wait's end.
- **Tests exercise the real thing,** and a readiness file replaces launch sleeps. Every downloaded artifact is pinned by hash. The gate script runs every gate after a failure.
- **Documented divergences are pinned by mirrored tests,** which is what made the undocumented ones findable.

## Ruled out

Checked and found sound, so nobody needs to look again.

| Concern                                                                           | Why it is not a problem                                                                                                                                                   |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A `RefCell` re-entrancy panic in the GNOME front-end                              | Three reviewers walked every `borrow_mut` near a signal-emitting call. All are single statements, and every handler GTK can emit synchronously takes only shared borrows. |
| A closed tab kept alive by a reference cycle                                      | Closures hold weak references on both platforms; each teardown takes the core out once. Not checked with a profiler.                                                      |
| A wake delivered to the wrong tab, or to a torn-down one                          | Tokens are monotonic and never reused, the registry is touched only on the main thread, and a late wake finds nothing.                                                    |
| Memory safety in `wolf-vt`                                                        | Every constant, struct and signature in `ffi.rs` was checked against the pinned 1.0.0 headers. Nothing mechanical keeps it so, which is worth a size assertion.           |
| Injection into Lua, JSON, the app's own page scripts, or the privileged installer | Arguments are passed as values, the rebuild document is built by `serde_json`, and the installer's quoting is correct.                                                    |
| A paste injecting the bracketed-paste end sequence                                | The library replaces escape and control bytes.                                                                                                                            |
| Path traversal in the preview server with a real root                             | Canonicalization and a component-wise prefix test handle symlinks, `..` and percent-encoding. S5 is the empty-root case only.                                             |
| The cost of saving state on every wake                                            | The commit is throttled, makes no call to Neovim, and writes nothing for an unchanged tree.                                                                               |
| A resize feedback loop                                                            | Each size is sent only on a mismatch, and the toolkit's allocation never depends on Neovim's answer.                                                                      |
| An unwired test                                                                   | Every test file was checked against its build target on both build systems.                                                                                               |
| Function length                                                                   | 22 of about 2,900 functions exceed 100 lines.                                                                                                                             |

## Method and limits

**Slices.** Thirteen reviewers each read one slice in full:

- core's Neovim embedding and concurrency
- core's redraw fold, state and open channel
- core's search, Markdown and preview server
- terminal sessions with `wolf-vt`
- the bridge
- `init.lua` and its contract
- the macOS pane view
- the macOS application shell
- the GNOME pane view
- the GNOME shell
- the GNOME overlays and preview
- duplication between the front-ends
- tests, gates and build

Test code was sampled, not read in full.

**Verification.** For each finding marked Verified the cited lines were re-read while this report was written. Where two or more reviewers reached the same finding independently, the entry says so. Tables mix statuses and say which applies to each row.

**What was run.** Nothing was built and no test suite was run. What was executed ran in a scratch directory outside the repository. The pinned Neovim 0.12.6 ran headless for a check of S1's newline case and for the `init.lua` reviewer's eighteen scripts, which are the source of each "tested by a reviewer". Ripgrep ran on a scratch file for C5, and one reviewer read the system's descriptor limit for P1 and extracted the pinned libghostty-vt tarball to read its headers.

**Costs.** Every figure is an estimate from reading unless the text says measured. The measurements quoted are the project's own frame-time notes and the `init.lua` reviewer's timings.

**Not covered.** Vendored third-party code, the generated bindings beyond a skim, the icon resources, and anything that needs a running app: AppKit and GTK behaviour, WebKit's classification of navigations, and descriptor counts in a live process. Each such dependency is marked Suspected where it matters.
