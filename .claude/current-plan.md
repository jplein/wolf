# Work unit 19 — preview interaction for `wolf-gnome`

Branch: `linux-feat/preview-interaction` off `linux` (`e33e722`).

## Orchestrator decisions — binding, read before the plan

**O0. The branch is already reconciled. Ignore any instruction to reconcile it.**
The architect read a stale `git status` snapshot and opened its plan with a §0
saying the tree is on `linux-feat/markdown-preview` with `preview_ui.rs`
untracked. That was measured and is **false**: `git status --porcelain` shows
only `.claude/current-plan.md` modified, `HEAD` is `e33e722`, and
`crates/wolf-gnome/src/preview_ui.rs` is tracked. Unit 17 and unit 18 have both
landed. Do no branch surgery.

**O1. Row 3 (preview in browser) is IMPLEMENTED but its feat-list box stays
UNTICKED.** The architect recommended ticking with a caveat and recorded the
alternative; this is the alternative, and the reason is the brief's own rule:
*if it cannot be verified end to end, say so plainly rather than ticking on
faith.* What can be proved on this host is that the running GTK application
produces a real `http://127.0.0.1:<port>/<relpath>` URL and hands it to the
platform URI launcher; what cannot is a browser opening, because `cage` has
neither a browser nor a portal. That last step is the row's whole point.
The in-repo precedent is the floating-panel row, unticked for an unproven
construction. The tick is one character to add on a host with a browser, and the
prose must say exactly that. **Report this to the human as the one judgement I
deliberately did not make unilaterally.**

**O2. Do NOT build a smoke that asserts the launch FAILED.** The architect
diagnosed this correctly: it is honest but structurally wrong, because it asserts
the negation of the feature and inverts the day the host gains a browser. The
artifact is a **recorded measurement** in `crates/wolf-gnome/tests/README.md`,
unit 18's font-row precedent.

**O3. Do NOT make a smoke depend on a fetch tool that is not in
`REQUIRED_TOOLS`.** `curl`, `python3` and `wget` exist on this host but are not
pinned in the flake, and `REQUIRED_TOOLS` must stay five. A smoke that fetched
the loopback URL through one of them would be non-hermetic and would fail loudly
on a correctly provisioned host. The server half is already proved on Linux by
`crates/wolf-core/tests/browser_preview.rs`'s six real-nvim, real-socket tests —
**cite them, do not re-test them.**

**O4. The non-Markdown link policy is REFUSE, not hand-to-the-OS.** Approved as
the architect argued it: on GNOME the "default app" path can start an arbitrary
process, the trigger is a string in a document the user merely previewed, and
there is no Gatekeeper/quarantine counterpart to `NSWorkspace.open`. It is a
deliberate divergence from macOS, must be documented as one with its reason, and
is reversible by a later unit with its own row and its own evidence.

**O5. `preview_link.rs` is a new sibling module, not an extension of
`preview_ui.rs`.** Approved on the architect's ownership argument: `preview_ui.rs`
states *why* its two rules were joined (they are inverses), the link classifier
has no such relationship, and the 1:1 correspondence with
`WolfKeyMap/PreviewLinkSeam.swift` is what makes the two front-ends comparable as
values. Split principle: **`preview_link.rs` owns URI arithmetic; `preview_ui.rs`
owns liveness gates and load strategy.**

**O6. Row 1 is ticked only if the click measurement succeeds.** The coder must
measure whether `wolf-pointer-inject`'s synthetic click reaches WebKit's DOM hit
test under `cage` **before** any assertion is written. If it does not, land the
code, keep the pure tables, record the negative measurement, and leave the row
unticked. Do not tick on a green unit test.

**O7. Source comments go to the coder; Markdown goes to the technical-writer.**
The architect's step 1 bundled `crates/wolf-ffi/tests/README.md:106` into the
coder's comment pass. Split it: the coder fixes the five `.rs` sites, the
technical-writer fixes every `.md` restatement. One writer per file, always.

**O8. Test-writer-after-coder is accepted for the in-`src` unit tables only,**
because two writers cannot hold one file and the tables live inside
`preview_link.rs` and `preview_ui.rs`. The test-first guarantee is recovered by
the mandatory red runs: **a test whose named falsifier does not turn it red is
rejected and rewritten.** That is a gate, not a formality.

### Decisions taken after coder pass 1, on its findings

**O9. There is a SIXTH false treesitter site and it is the original — fix it.**
`runtime/nvim/init.lua:597-599` claims the helper "self-degrades to `false` for
every block because get_string_parser / query.get find nothing" on Linux/cargo.
The coder re-measured on this host against the pinned nvim 0.12.2 and confirmed
the seven parsers resolve with `highlights` queries while `go`, `rust`, `python`
and `javascript` do not — so the claim is false by exactly the same measurement.
The brief said "five sites"; the truth is six, and CLAUDE.md's rule is that a
divergence is a bug to address, not a quota to fill. It is a `.lua` source
comment, so it belongs to the **coder** (O7 splits `.rs` from `.md`; `.lua`
source comments go with the source).

**O10. `about:` is REFUSED and never launched, and `about:blank#frag` scrolls.**
The coder found that WebKitGTK's `load_html(html, None)` does not leave a
document with no base — it substitutes `about:blank`. So in the nameless-buffer
case a click arrives as `about:blank#sec`, which has a scheme, is not `file`, and
under the plan's row 1 would be handed to `gtk::UriLauncher`. Handing `about:` to
a browser is wrong. New arm, ahead of row 1:

- `about:blank#frag` (non-empty fragment) → `ScrollToFragment { frag }` — an
  anchor in a document with no location is still an anchor, and scrolling needs
  no document identity.
- any other `about:` URI → `Refuse`, with its own distinct reason.

**This also resolves finding (c)**: a bare `#section` in a nameless buffer now
scrolls instead of being refused. In a *named* buffer it never reached row 6
anyway, because the `file://` base makes it row 2.

**The plan's claim that rows 5-7 are "reachable only when the source path is
empty" is therefore wrong and must not be documented as written.** What a click
in a nameless-buffer preview actually delivers for a *relative* link is still
unmeasured; **the coder must measure it during the wiring pass** and report the
observed URI before anything is documented about rows 5-7.

**O11. A non-local `file://` authority is a third refusal reason — keep it.**
`file://evil.com/a/x.md` is `Refuse` with its own distinct reason. The plan's
7-row table omitted it. With O10 there are **four** distinct refusal reasons out
of `classify`, not two. The test-writer must sweep all four.

**O12. `scheme_of` implements the RFC 3986 grammar literally; do not test `C:\`.**
The plan's justification sentence was internally inconsistent — under that
grammar `C:` *is* a scheme. Wolf runs on Linux and macOS, so a drive letter is
not a real input, and a test row pinning it would constrain a correct change for
an input that cannot occur. **Add no `C:\` row.** Add instead
`/notes/12:30 standup.md`, a real input with a colon in a path segment, which the
grammar correctly rejects as a scheme because the first character is not ALPHA.

**O13. Two under-specified behaviours are settled as the coder implemented them,
and each owes its test row a reason rather than a bare assertion.**

- **A trailing slash on the `source_path` input is reflected, not normalized**
  (`/a/x.md/` → `file:///a/x.md/`). `nvim_buf_get_name` never produces one, and
  silently rewriting a caller's path is worse than reflecting it. Pin it with a
  message that says the row documents the reflection choice.
- **A query string is not stripped** (`file:///a/x.md?v=1` → path
  `/a/x.md?v=1`). `?` is a legal filename byte on Unix, so stripping would break
  a file legitimately named that way. Core answers `0` and the click prints a
  refusal, which is an honest failure. Note it in the module docs; do not "fix"
  it by copying `browser_preview::resolve_path`'s stripping, which is a **server**
  route parser and a different problem.

---

### Decisions taken after the wiring pass, on its three measurements

**M1 succeeded: the synthetic click reaches WebKit's DOM hit test.** Measured
2026-08-06 at cage output 1280x720. The blue-dominant cluster is bbox
`x [788,1002] y [165,176]`, centroid `(900, 171)`; clicking it moved the Neovim
readback from `Preview: alpha.md` to `Preview: beta.md`, and the handler logged
`nav=LinkClicked uri=file:///…/project/beta.md`. **Row 1 is tickable.**

**M2: the genuinely nameless-buffer case is UNREACHABLE from the running
application.** `runtime/nvim/init.lua:2229`, inside `wolf_setup_preview`'s
`emit_previews`, filters an empty `b:wolf_preview_source` out of the
`wolf_preview` notification, so `Snapshot::previews` never carries the pane and
no web view is ever created. Measured: zero `decide-policy` lines, both
statusbars reading `[No Name]`.

**M3: `gtk::UriLauncher::launch` reports `Ok(())` and nothing happens** under
`cage` with no session bus. There is no failure to print and therefore nothing
to assert in either direction.

**O14. Row 2 is proved by M2's leg (a), not by a nameless buffer.** The
Ctrl+Shift+N buffer is `Untitled 2.md` with an **absolute** `nvim_buf_get_name`
and a markdown filetype, and its preview bound: `Preview: Untitled 2.md`. That
is the row's real path — the shipped design is precisely that an anonymous
buffer gets a real unsaved `.md` name, so it is never nameless. A `:enew`
buffer with no name is outside the feature, not a gap in it. Row 2 is tickable.

**O15. `classify` rows 1, 2, 9 and 10 and `file_uri_for`'s empty-path `None` are
DEFENSIVE AND UNREACHABLE, and their test messages must say exactly that**,
citing `runtime/nvim/init.lua:2229` as the reason. Do not delete them — a guard
whose reachability depends on one Lua `if` is worth keeping — but do not let any
document claim they are "the nameless-buffer path" either. The plan's earlier
sentence that rows 5-7 are "reachable only when the source path is empty" is
wrong in both directions and must not be written down.

Row 8 is the row carrying real measured behaviour, and it is worth a message of
its own: a never-saved `Untitled 2.md` resolves a relative link through it, with
the space percent-encoded as `%20` — so `percent_encode_path` is load-bearing on
a path a user genuinely gets, not a defensive nicety.

**O16. No smoke may ever trigger `PreviewInBrowser`. This is a safety rule, not
a style one.** M3 found that `gio mime x-scheme-handler/http` answers
`google-chrome.desktop` inside the devShell, because `XDG_DATA_DIRS` reaches
`/run/current-system/sw/share/applications` **even under the smoke harness's
`HOME` redirect**. So on any host where `GtkUriLauncher` does work, a smoke
exercising this path would open a tab in the developer's real browser. Combined
with M3's "reports success, does nothing" there is no observable to assert
anyway. **Row 3 stays unticked** (O1 confirmed, for this sharper reason), the
artifact is the recorded measurement, and `tests/README.md` must record the
hazard so a later unit does not walk into it.

**O17. The palette collision is a real defect this unit introduced, and the fix
is to assert the TOP row.** Implementing row 3 un-`Deferred`s
`Command::PreviewInBrowser`, so the palette's classification filter no longer
removes it and the smoke's `prv` query now matches both `Preview` and
`Preview in Browser`. `palette::tests::palette_rows_for_the_preview_smoke_rank_the_named_command_first`
(renamed from `…_select_one_row_each`, whose name became false)
fails, correctly.

**No query can separate them** — `rank_matches` is subsequence-based and
`Preview` is a prefix of `Preview in Browser`, so every subsequence of the first
is a subsequence of the second. Uniqueness is unrecoverable and must not be
chased. Rewrite the assertion to "the **top** row for `prv` carries
`Command::Preview`", and pin the tie-break explicitly rather than assuming it:
add a row for an MRU seeded with `preview_in_browser`. **If that adversarial MRU
inverts the order, that is the finding** — say so in the test and record it in
`tests/README.md` as a live fragility of the preview smoke, rather than
weakening the assertion until it passes. The smoke's own state directory is
fresh per run and it never runs `preview_in_browser`, so the empty-MRU ordering
is what it actually depends on.

The two doc comments whose stated premise is now false must be rewritten in
place, not annotated: `palette.rs:684-689` (`PREVIEW_SMOKE_QUERIES`) and
`smoke_headless.rs:8479` (`PREVIEW_QUERY`). Both say the row is `Deferred` and
that the classification filter is what makes `prv` unique.

**O18. Do not extract a `read_liveness` helper on `PaneInner`.** Two identical
three-line borrows is not duplication worth a helper, and statement-per-borrow
is this file's stated discipline. Leave it.

### Decisions taken after the red runs

The red runs did their job: they found that one named falsifier is not a
falsifier at all, and that a second is falsifiable only as a conjunction.

**O19. Strengthen the link smoke so the base URI becomes falsifiable.**
Restoring `view.load_html(html, None)` leaves
`preview_link_click_repoints_the_pane_under_cage` **green**. Measured cause: with
no base URI WebKit delivers the **raw relative href** (`uri="lnkinto.md"`),
`classify` takes row 9, and `WolfCore::preview_file` resolves that relative path
against the captive Neovim's working directory — **which in this fixture is the
same directory as the source**. The follow succeeds by coincidence of the cwd.

That is precisely the "a check aimed at the right property that looks where the
defect cannot appear" failure this suite collects. Fix the fixture, not the
assertion: **put the source in a subdirectory so the cwd is not the resolution
answer.** Then a `None` base resolves the relative href against the wrong
directory, `preview_file` answers `0`, the readback does not change, and the
smoke reddens. Re-verify the click calibration after the move — the link text is
byte-identical and an href is not drawn, so the cluster should not move, but that
must be measured, not assumed.

**O20. Row 9 is still defensive, but not for the reason the docs give.** It is
**not** unreachable because nothing produces a schemeless target — measured, a
`None` base produces exactly that, and it is one line away. It is unreachable
because `file_uri_for` answers `Some` for every real source path, and the only
inputs that make it answer `None` are an empty source path (unreachable per M2's
`init.lua:2229` filter) and a relative one (`nvim_buf_get_name` never yields
one). State that precise reason everywhere row 9 is described. O15 stands for the
`about:` rows; it is **overruled for rows 9 and 10**.

**O21. Keep both empty-fragment guards; rewrite the prose.** Deleting
`classify`'s `.filter(|f| !f.is_empty())` alone leaves the row green, and so does
changing `split_fragment`'s collapse alone — the row is attached to the
conjunction. Do **not** delete either: the filter sits *after* `percent_decode`,
so the two guard different steps, and removing a guard on a decoder's edge case
to win a falsifier is the wrong trade. Instead rewrite `classify_table`'s
falsifier clause to say plainly that neither guard is individually falsifiable
and why. **If `percent_decode` can return an empty string from a non-empty
input, add that row** — it would make the filter individually falsifiable and is
the honest way to earn the falsifier back.

**O22. Four test doc-comment claims are measurably false and must be rewritten in
place.** Each predicts a falsifier outcome the red runs contradict:

1. `preview_link.rs`, `classify_table` — the bare-`#` redundancy is stated
   backwards (O21).
2. `preview_link.rs`, `classify_table` — "make `classify` a constant, property 2
   reddens and nothing else does". Property 1's first row reddens first, so
   property 2 is never reached and has no independent evidence. Either give it a
   row set that reaches it, or document it as belt-and-braces.
3. `preview_ui.rs`, `document_update_table` — "compare `source_tick` in rule 3,
   row 5 reddens". Row 3 reddens first. Row 5 **is** attached to the bufnr key
   (proved by deleting rule 3 outright), but not by the named edit.
4. `command_action.rs`, `dispatched_commands_are_the_expected_set` — predicts the
   palette test reddens "on its unfiltered floor". Measured: it reddens on the
   `actionable_rows` clause, because `rank_palette` runs before the
   classification filter so the floor of 2 still holds.

**O23. `WolfCore::preview_file` accepting a relative path is NOT a bug to fix
here.** Core's doc comment states a requirement on the caller ("path must be
absolute"), not a promise to reject. With the base URI in place the front end
always supplies an absolute path, so the contract holds where it matters. Note
it; change no core code in this unit.

**O24. The three GTK-signal rules have no test and that is recorded, not fixed.**
`on_preview_link`'s write-`pending_fragment`-unconditionally rule,
`sync_preview_view`'s remove-branch clear, and `on_preview_load_finished`'s
`take()` are reachable only through GTK signals. They are where the design's own
"stale fragment applied to the wrong file" hazard lives, and they are asserted
only in prose. Extracting a pure seam for the write rule is a real option and a
sensible later unit; it is **out of scope here** and belongs in
`tests/README.md`'s "What is still unproven".

## Scope

Three `feat-list.md` "Markdown preview" rows plus one documentation-correctness
item:

1. **Follow links inside the preview** (line 145, `61204b7`)
2. **Preview of anonymous / unsaved buffers** (line 152)
3. **Preview in browser** (line 153, `4728086`)
4. **Correct five false treesitter source comments**

Not in scope, not to be ticked: syntax-highlighted code blocks (blocked on ELF
parser coverage), and the floating-panel pickers row.

**No new dependency. `Cargo.toml` does not change and `Cargo.lock` must not
move.** If the lock moves, something unrelated happened — find out what before
proceeding.

## The three rows are three different SHAPES of work

- **Row 2 needs no code.** Already merged, already platform-neutral. It needs
  evidence from the running GTK application and nothing else.
- **Row 3 needs one small GTK arm.** Core is complete and Linux-green today.
- **Row 1 is the whole unit's engineering.**

---

## Established by reconnaissance — verify cheaply, do not re-derive

### Row 2 — already live on Linux

`feat/preview-anonymous-files` is fully contained in `main`. The substantive
commit is `0266b96`, "Name anonymous buffers `Untitled.md` and make preview
work", whose problem statement is "anonymous files are named `[No Name]`, and
preview silently fails." The fix is entirely platform-neutral:

- `runtime/nvim/init.lua` — `_G.wolf_next_untitled`, `_G.wolf_name_untitled`,
  `_G.wolf_new_untitled`, `_G.wolf_name_untitled_current`, and a `VimEnter`
  autocmd in the `WolfUntitled` augroup. Names are `Untitled.md`,
  `Untitled 2.md`, … (index 1 un-numbered; `Untitled 1.md` never generated),
  colliding against on-disk `fs_stat`, every `nvim_list_bufs()` name, and a swap
  file. Naming sets `filetype = "markdown"` and **never writes to disk**.
- `crates/wolf-core/src/input.rs` — `InputEvent::NewBuffer` calls
  `new_untitled(nvim)`; the `Chdir` arm calls `name_untitled_current(nvim)`.

**There is no name-based refusal anywhere.** Every preview gate keys on
**filetype**: `wolf_open_preview` returns `0` unless `vim.bo.filetype ==
"markdown"`, and `preview_ui::preview_press` gates on `markdown_active`.

### Row 3 — core complete, already proved on Linux

- `WolfCore::open_in_browser(&self) -> String` (`api.rs:576`) returns
  `http://127.0.0.1:<port>/<percent-encoded relpath>`, or
  `http://127.0.0.1:<port>/` for an unnamed buffer, or `""` on three error cases
  (RPC error, non-Markdown buffer, bind failure), each logged at `warn`.
- Loopback-only bind and the path-traversal guard are **hard security
  requirements** (`crates/wolf-core/DECISIONS.md:79`).
- 15 in-module unit tests plus `crates/wolf-core/tests/browser_preview.rs`'s six
  real-nvim, real-socket integration tests run under `cargo test` on this host.
- macOS's caller is four lines: call, refuse empty/unparseable, `NSWorkspace.open`.
  It is a **View-menu item with no keyboard shortcut**, enabled on
  `isMarkdownActive`.
- `gtk::UriLauncher` needs `v4_10`; the crate pins `v4_12`, which implies it.
  `new(uri)` and `launch(parent, cancellable, cb)` are ungated; only `can_launch`
  needs `v4_20` and is not used.

### Row 1 — core, FFI and Lua are done and reachable from `wolf-gnome`

- `WolfCore::preview_file(&self, win: i64, path: &str) -> i64` (`api.rs:715`)
  rebinds an existing preview window. Returns the linked file's **source** bufnr,
  or `0` when the path is empty, unreadable, not Markdown, on RPC error, or after
  shutdown. **It never creates a pane.**
- `path` **must be absolute; core does no relative resolution.** There is no
  `resolve_link`/`open_path` helper anywhere.
- The RPC path is **already exercised on Linux** through `WolfCore::apply_nav`'s
  `NavTarget::Preview`, which `wolf-gnome` drives via `nav_ui.rs`.
- Heading anchors are done (`slugify`, `heading_slugs`, `HeadingIdAssigner`,
  `scroll-margin-top`).
- **Links are emitted verbatim.** `markdown.rs` never touches `Tag::Link`. No
  rewriting, no custom scheme, no `data-` attribute, no `<base>`. So
  relative→absolute resolution is the **browser engine's** job in both front-ends.
- What must be newly written in Rust is everything in
  `WolfKeyMap/PreviewLinkSeam.swift` (125 lines, pure, 8 headless tests).

### The GTK-side gap

- `pane_view.rs:1441` — `view.load_html(html, None)`. **Base URI is `None`
  today**, so relative links cannot resolve at all.
- `pane_view.rs:1306` — the `webkit6::WebView` has **no signal connections**.
- `PaneWidgets.win` **is** the preview window handle `preview_file` needs.
  `refresh_pane_state` (`:743-751`) already looks up `snap.previews.get(&win)` by
  it; `pane_for_win` exists at `:1096`. **No new plumbing.**
- `PreviewInfo.source_path` is already per-pane.

### The trap this row would otherwise introduce

`load_preview_document(pane_id, html)` (`:1394`) takes the in-place body-swap
branch whenever the `<head>` offset is unchanged — and the head is byte-identical
across renders **by design**. It **takes no source identity**, so it cannot detect
a source switch. Without a guard, a link-follow swaps the linked file's body into
a document whose **base URI still points at the previous file's directory**, and a
pending cross-file `#anchor` has no load edge to fire on.

---

## Design

### A. The link policy

`classify(uri: &str, source_path: &str) -> PreviewLink`, over the URI WebKit has
already resolved against the base.

| # | URI shape | GTK action | vs macOS |
| --- | --- | --- | --- |
| 1 | scheme present and not `file` (`http`, `https`, `mailto`, `tel`, …) | `OpenExternally { uri }` → `launch_uri` | same |
| 2 | `file`, same path as source, non-empty fragment | `ScrollToFragment { fragment }` | same |
| 3 | `file`, same path, absent or empty fragment | `PreviewFile { path, fragment: None }` | same |
| 4 | `file`, different path (± fragment) | `PreviewFile { path, fragment }` | same |
| 5 | no scheme, non-empty path | `PreviewFile { path, fragment }` (defensive) | same |
| 6 | no scheme and no path (bare `#`, empty href) | **`Refuse(reason)`** | **diverges** |
| 7 | `file` with an empty path (`file://`) | **`Refuse(reason)`** | **diverges** |

Same-path comparison is **lexical normalization** of both sides (drop `CurDir`,
pop on `ParentDir`, keep `RootDir`), so `/a/./x.md == /a/x.md`, matching
`standardizedFileURL.path`. **No symlink resolution** — it would be a filesystem
touch in a pure seam, and macOS does not do it either. The fragment is
percent-decoded and an empty fragment collapses to `None`.

**Superseded by O15/O20.** These rows are defensive, but not because nothing
produces a schemeless target — measured, a `None` base produces exactly that.
They are unreachable because `file_uri_for` answers `Some` for every real source
path.

**External `http(s)` opens the system browser** (O1's helper is shared with row 3:
one place logs a failure, one place decides the parent window).

**A `file:` link to a non-Markdown file is REFUSED** — see O4.

**The `decide_policy` return-value convention.** WebKitGTK's `decide-policy` uses
a true-handled accumulator: **`true` means "this handler answered the decision;
do not run the default handler"; `false` means "let WebKit's default handler
decide"**, which for a navigation calls `use_()`.

- `decision_type == PolicyDecisionType::NavigationAction` **and** the decision
  downcasts to `NavigationPolicyDecision` **and** `navigation_action()` is `Some`
  **and** `navigation_type() == NavigationType::LinkClicked` **and** `request()`
  yields a non-empty `uri()` → **answer the decision first** with
  `decision.ignore()`, then act out of band, then **return `true`**.
- Every other input → touch the decision not at all and **return `false`**.

Silent failure modes to state in the code comment:

- Returning `true` unconditionally: the initial `load_html`'s own decision is
  never answered and the preview is **permanently blank**.
- Returning `false` after `ignore()`: the default handler calls `use_()` on an
  answered decision; the first answer wins but the code no longer says what it
  decided.
- Not filtering on `PolicyDecisionType::NavigationAction` first: `NewWindowAction`
  decisions are the *same class*, so a downcast-only filter would treat a
  `target="_blank"` as an in-page navigation. Returning `false` for
  `NewWindowAction` is correct.
- `NavigationType::Other` is what the initial `load_html` arrives as. Filtering on
  `LinkClicked` is exactly "anything that is not a user click is allowed".

**Ordering rule, in every arm: answer the decision before doing anything else.**
`preview_file` is a blocking RPC; running it before `ignore()` would hold WebKit's
signal emission open for the whole timeout budget.

### D. The source-switch guard

New pure function in `preview_ui.rs`, immediately after `needs_reload`:

```
pub enum DocumentUpdate { FullLoad, InPlaceEligible }
pub fn document_update(current: PreviewLoad, loaded: Option<PreviewLoad>,
                       fragment_pending: bool) -> DocumentUpdate
```

Total, in this order:

1. `loaded.is_none()` → `FullLoad` (first load; no committed `<head>`).
2. `fragment_pending` → `FullLoad` (the scroll needs a `LoadEvent::Finished`
   edge, and only a real load produces one).
3. `loaded.source_bufnr != current.source_bufnr` → `FullLoad` (the base URI
   belongs to the previous file).
4. otherwise → `InPlaceEligible`.

**What must not regress, stated in the doc comment:** rule 4 preserves unit 17's
measurement — `window.scrollY` 2000 preserved in 24 of 24 samples on a live edit,
against 0 of 24 for a full `load_html`. The guard distinguishes "same source, new
tick" from "different source" on the **bufnr, never the tick**: two freshly opened
files share a `changedtick`, the same defect `needs_reload`'s fourth row records.

**Deliberately NOT ported:** macOS's `updateStrategy` rule 2, "a load is in
flight". That hazard exists on GTK today and predates this unit; porting it means
porting `previewLoadsInFlight`, `previewDidFailLoad`,
`didFailProvisionalNavigation` and the superseded-load reasoning. **Record it as a
known divergence in `DECISIONS.md`; do not build it.**

**Threading change.** In `refresh_previews`, replace the `due: Vec<(PaneId,
PreviewLoad)>` tuple with a named struct — the file's own `PaneJoin`/`PaneState`
precedent:

```
struct DueRender { id: PaneId, load: PreviewLoad, source_path: String,
                   update: preview_ui::DocumentUpdate }
```

Built inside the existing single `panes.borrow()` block from `p.current_load`,
`p.loaded`, `p.preview.borrow()`'s `source_path`, and
`p.pending_fragment.borrow().is_some()` — all four read at the same instant,
which is why the strategy is computed there and not later.

```
fn load_preview_document(&self, pane_id: PaneId, html: &str,
                         source_path: &str, update: preview_ui::DocumentUpdate)
```

Two body changes and only two: the `swap` match guard gains `update ==
DocumentUpdate::InPlaceEligible &&` ahead of `recorded != 0 && offset ==
recorded`; and `view.load_html(html, None)` becomes `view.load_html(html,
preview_link::file_uri_for(source_path).as_deref())`.

`head_len` is unchanged — on a source switch the head is byte-identical, which is
precisely why the head guard alone cannot see a switch. `PreviewLoad` gains no
field.

### E. The base URI

`file_uri_for(source_path: &str) -> Option<String>` in `preview_link.rs`.

- **Names the source FILE, never its directory, and carries no trailing slash.**
  Under RFC 3986, `beta.md` against `file:///home/u/proj/alpha.md` replaces the
  last segment → `file:///home/u/proj/beta.md`. WebKitGTK's
  `webkit_web_view_load_html(content, base_uri)` matches
  `loadHTMLString(_:baseURL:)` here — no divergence that matters.
- **Both trailing-slash mistakes are silent.** `…/alpha.md/` resolves `beta.md`
  one level too deep; `preview_file` gets a nonexistent path, returns `0`, click
  does nothing. `file:///home/u/proj` (directory, no slash) resolves to the
  **parent** directory. Pin both with test rows.
- **Percent-encoding is mandatory, not tidiness.** Keep `A-Za-z0-9`, `-._~`, `/`;
  encode every other byte as uppercase `%XX`. Without it a source path containing
  `#` truncates the base URI at a fragment and a space produces an invalid URI.
- **A relative path returns `None`** — prefixing `file://` onto one parses the
  first segment as the **authority**, a total silent breakage. Defensive
  (`PreviewInfo.source_path` comes from `nvim_buf_get_name`), but pin it.
- **An empty source path returns `None`, and the caller passes `None` through** —
  today's behaviour, deliberately preserved. **Do not synthesize a cwd base**: the
  document has no location, and inventing one resolves links against a directory
  it has no relationship to. Consistent with `preview_status`, which collapses an
  empty source path to a bare `"Preview"`.
- One observable side effect for `DECISIONS.md`: with a `file://` base the
  document acquires a file origin, so `![](img.png)` can now resolve from disk.
  Everything else is inlined base64.
- **The coder must confirm on a live run that the initial load still renders with
  a non-`None` base URI.** `preview_document_follows_the_configured_appearance_under_cage`
  is the tripwire.

### F. The fragment scroll

- **Same file**: `evaluate_javascript` immediately, the existing body-swap shape.
- **Cross file**: store in a new `PaneWidgets` field `pending_fragment:
  RefCell<Option<String>>` and apply on `connect_load_changed` matching
  **`LoadEvent::Finished`**. Connect in `sync_preview_view` beside
  `connect_decide_policy`, same `Weak` + `pane_id`. `take()` in its own statement.
- **The coupling to D**: `load-changed`/`Finished` fires only for a real
  `load_html`, never for a body swap. That is exactly why `document_update` rule 2
  forces `FullLoad` when a fragment is pending. State it in both files' docs.
- **The write rule, with a real falsifier: a successful `preview_file` ALWAYS
  writes the slot** — to `Some(frag)` or to `None`. Otherwise: click a cross-file
  link with `#a`, then before the render lands click another with no fragment, and
  the stale `#a` is applied to the second file. `classify` returns
  `fragment: Option<String>` and the glue assigns unconditionally.
- **Clear the slot with the view**, beside `loaded` and `head_len` in
  `sync_preview_view`'s remove branch — all three belong to the view, not the pane.
- **JS injection safety.** Use `serde_json::to_string` plus the `U+2028`/`U+2029` (LINE SEPARATOR / PARAGRAPH SEPARATOR)
  replacements `preview_swap_payload` already applies; return
  `Result<String, serde_json::Error>` so the call site prints and skips.
  **One difference: a SINGLE encoding, not a double one** — `preview_swap_payload`
  double-encodes because its script does `JSON.parse(...)`; this script embeds the
  literal directly as `document.getElementById(<literal>)`. Say so, or the next
  reader "fixes" it into a double encode and the scroll silently targets a quoted
  id.
- **Deliberate divergence from macOS's escaper**: do **not** escape `<` and `/`.
  `preview_swap_payload` already establishes the reason — this is JS source handed
  to `evaluate_javascript`, never HTML, so `</script>` has nothing to close.
  Carrying two different escapers invites the belief that one is load-bearing.

---

## Verification

### Row 1 — follow links

**(a) Pure unit tables** in both modules. Necessary, not sufficient: they prove
classification, not that a click does anything.

**(b) One new `cage` smoke, `preview_link_click_repoints_the_pane_under_cage`,
in `smoke_headless.rs`.** No new file. Built on `POINTER_PREAMBLE` through
`pointer_scenario(body)`, joining the seven pointer smokes on `CAGE_LOCK`.
`REQUIRED_TOOLS` stays five.

**Whether a synthetic click reaches WebKit's DOM hit test under `cage` is NOT
established. The coder must measure it before writing any assertion (O6).**
Established: the web view takes pointer input (`DECISIONS.md:226` — that is *why*
a click in a preview does not focus the pane), and `wolf-pointer-inject` produces
real `wl_pointer` events. **There is no keyboard alternative** — the view is
`set_focusable(false)`/`set_can_focus(false)` and must stay so, or
`click_then_close_keeps_keyboard_input_alive_under_cage` breaks.

**Deriving the click point** — this file's own measured-constant procedure:

- *Phase A, a measurement, not a committed test.* A temporary scenario opens a
  preview whose source is one wide link on the first line, `grim`s it, and decodes
  with the existing `decode_capture` / `Capture::pixel`, printing the bounding box
  of every blue-dominant pixel (`b > 150 && b > r + 60`). The dark link colour is
  `#4ea1ff` (`themes/default.css:37`) against `#1b1b1b` and `#d4d4d4`, and
  `SMOKE_APPEARANCE` is `dark`, so the cluster is unambiguous — **provided the
  search rectangle is inset from the pane edges**, because `ACTIVE_PANE_CSS`'s
  focus ring is `#3584e4`. Use the existing `SampleRect` + `PANE_EDGE_INSET_PX`.
- Record the measured rectangle in the doc comment with the date and the cage
  output size; set the click constants to its centroid.
- **The committed smoke re-asserts the calibration**: decode capture A, find the
  link cluster, assert the click constant falls inside it; then assert the Neovim
  readback changed. Without the first, the constant has exactly the failure this
  file records at line 1947 — "stayed green while describing a layout that no
  longer existed".

**The evidence assertion is a Neovim readback, not OCR.** After a successful
follow, `tabpagebuflist()` maps to a preview scratch named `Preview: beta.md`
where it named `Preview: alpha.md` — exact, produced by Neovim, unsatisfiable by
any sidebar row or `:e` toast. Reuse `PREVIEW_READBACK_PREFIX` and the existing
comma-joined `writefile` idiom.

**If the measurement shows WebKit never receives the click**, land the code, keep
the pure tables, record the negative measurement in `tests/README.md`'s "What is
still unproven", and **leave row 1 unticked**.

**The fragment scroll's effect reaches no test** (a scroll offset is invisible to
OCR). Test `scroll_into_view_js` purely and record the gap.
**The external-link arm reaches no test either** — nothing under `cage` can
capture a browser opening. Record the gap.

### Row 2 — anonymous / unsaved buffers

**A new smoke, `preview_of_an_unsaved_buffer_renders_under_cage`.** Not an
extension: `NEWBUF_SCENARIO`'s assertions are about the new buffer replacing the
old and a preview leg would move its calibrated capture; `PREVIEW_SCENARIO` is
already the longest in the file with eleven assertions and a calibrated A/B pair,
and two independent features behind one exit code contradicts this file's
"exit codes carry meaning" rule.

Scenario: launch and poll; Ctrl+Shift+N; type a distinctive marker as an `h1` into
the never-saved buffer and Escape; drive Preview through the palette with
`PREVIEW_QUERY`; sleep `PREVIEW_SETTLE`; write the `tabpagebuflist()` readback;
`:e <a filler file>` over the **source** pane — `PREVIEW_SCENARIO` step 7's proven
move, which takes the marker's raw text off screen so an OCR hit can only have
come from the web view, while the untitled buffer stays loaded and modified under
nvim's `hidden` default; capture.

Two assertions:

1. **Load-bearing:** the readback contains an entry named `Preview: Untitled.md`.
   That one string proves the naming autocmd ran, the markdown filetype gate
   passed, and the preview bound — all three.
2. OCR of the capture contains the marker, drawn only by the web view.

The marker must be registered in `ASSERTED_OCR_TERMS` so
`sidebar_filler_names_cannot_satisfy_any_smoke_assertion` covers it, and must
appear in no fixture name.

### Row 3 — preview in browser

**Implemented; box stays unticked (O1). No failure-asserting smoke (O2). No
unpinned fetch tool (O3).**

Observable, and where:

- **The command dispatches rather than defers** — the fast suite.
  `EXPECTED_DISPATCHED_IDS` goes 17 → 18 with `"preview_in_browser"`;
  `ui_action_table` row 21 goes `Want::Deferred` → `Want::Exact(UiAction::OpenInBrowser)`.
- **The loopback server binds, serves, and answers a real HTTP GET** — already
  proved on Linux; cite, do not re-test.
- **A non-empty loopback URL is produced inside the running GTK app and handed to
  the launcher** — from the app's stderr, as a **recorded measurement** written
  into `crates/wolf-gnome/tests/README.md`. The coder runs the app once under
  `cage` by hand with stderr captured and records the observed URL and the launch
  failure verbatim.

---

## Red runs — the exact falsifying edit for every new or changed test

**`preview_link::classify`**
- Delete the `scheme != "file"` arm → the `https` and `mailto` rows redden.
- Compare raw strings instead of normalized paths → the `/a/./x.md` row reddens.
- Return `Some("")` instead of `None` for an empty fragment → the same-file
  bare-`#` row reddens.
- Return the non-Markdown refusal as `PreviewFile` → the row-6/7 refusal rows redden.
- **Non-vacuity**: make `classify` return `Refuse` for every input → the assertion
  that the range over the sweep is exactly
  `{OpenExternally, ScrollToFragment, PreviewFile, Refuse}` reddens. Without it a
  constant classifier satisfies every negative-only row.

**`preview_link::scroll_into_view_js`**
- Interpolate the fragment raw → the row whose fragment carries a `"` reddens.
- Drop the `U+2028`/`U+2029` (LINE SEPARATOR / PARAGRAPH SEPARATOR) replacements → those two rows redden.
- Emit `querySelector` instead of `getElementById` → the pairing assertion reddens
  (assert the script contains both the literal and `scrollIntoView`).

**`preview_link::file_uri_for`**
- Append a trailing `/` → the `"/a/x.md"` row reddens; its message states the
  resolution consequence.
- Return `Some("")` for an empty path → the empty-path row reddens.
- Return `Some(...)` for a relative path → that row reddens; its message names the
  authority-parsing trap.
- Drop the percent-encoder → the space and `#` rows redden.

**`preview_ui::document_update`**
- Change the final arm to `FullLoad` → the "same source, new tick" row reddens,
  and its message names what that costs (unit 17's 24/24).
- Compare `source_tick` instead of `source_bufnr` in rule 3 → the "different
  source, same tick" row reddens.
- Delete the `fragment_pending` clause → the pending-fragment row reddens.
- **Non-vacuity**: assert the range is exactly both variants.

**`preview_ui::preview_file_press` / `browser_press`**
- Test the buffer-kind gate before the liveness match → the property-2 clause
  reddens.
- Add `Some(Liveness::NotResponding) => Follow` → the table row reddens.

**`preview_ui::browser_url`** — return `Some(raw)` unconditionally → the
empty-string row reddens.

**`preview_ui::refusal_reasons_are_distinct_and_mid_sentence`** (rewritten in
place to cover the new gates) — give two new arms the same string → reddens.

**`command_action::dispatched_commands_are_the_expected_set`** — revert
`Command::PreviewInBrowser` to `Deferred(...)` → a one-element diff naming
`preview_in_browser`. Run alone, per that test's own convention.

**`preview_link_click_repoints_the_pane_under_cage`**
- **Best falsifier:** in the `decide-policy` handler, return `false` for a
  `LinkClicked` navigation without calling `ignore()`, letting WebKit navigate
  itself. The view loads `beta.md` as raw text and the readback still names
  `Preview: alpha.md` → red. This is exactly the defect the design prevents.
- Second: restore `view.load_html(html, None)`. The clicked link resolves against
  `about:blank`, `classify` sees no `file` scheme, readback unchanged → red.
- **Stated non-falsifier, which must be written down:** forcing `document_update`
  to `InPlaceEligible` always leaves this smoke **green** — Neovim rebinds either
  way. The guard is invisible to this smoke; its only falsifiable evidence is the
  unit table.

**`preview_of_an_unsaved_buffer_renders_under_cage`**
- Revert `_G.wolf_new_untitled` to a plain `:enew`. The buffer is then `[No Name]`
  with no markdown filetype, `wolf_open_preview` returns `0`, no preview window
  exists, no `Preview: ` entry in the readback → red.
- **The OCR half has no independent falsifier**: the same revert reddens it. Say
  so rather than inventing one.

**`sidebar_filler_names_cannot_satisfy_any_smoke_assertion`** — add either new
asserted term as a substring of a `SIDEBAR_FILLER_FILES` name → red.

---

## Files

### New: `crates/wolf-gnome/src/preview_link.rs`

Pure, GTK-free URI rulebook. Port of `WolfKeyMap/PreviewLinkSeam.swift`. `std` and
`serde_json` only — no `gtk`, no `gdk`, no `webkit6`, no `WolfCore`.

**Public**
- `pub enum PreviewLink` — `OpenExternally { uri: String }`,
  `ScrollToFragment { fragment: String }`,
  `PreviewFile { path: String, fragment: Option<String> }`, `Refuse(&'static str)`.
  Derives `Debug, PartialEq, Eq`.
- `pub fn classify(uri: &str, source_path: &str) -> PreviewLink`
- `pub fn file_uri_for(source_path: &str) -> Option<String>`
- `pub fn scroll_into_view_js(fragment: &str) -> Result<String, serde_json::Error>`

**Private, tested from the child module**
- `split_fragment` — split at the first `#`; empty fragment collapses to `None`.
- `scheme_of` — the RFC 3986 grammar (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`),
  so a Windows-looking path is not mistaken for a scheme.
- `path_from_file_uri` — strip `file://`, refuse a non-empty authority other than
  `localhost`, percent-decode.
- `percent_decode` — `%XX` into bytes, then `String::from_utf8_lossy`. Lossy is
  deliberate: a non-UTF-8 path is one `preview_file(&str)` cannot take, and a
  replacement character produces a path core's `filereadable` rejects — a refusal,
  not a wrong file.
- `percent_encode_path` — keep `A-Za-z0-9-._~/`, encode the rest uppercase `%XX`.
- `normalize_path` — lexical only. No symlink resolution.

Module docs carry: the classification table, the two divergences from macOS and
their reasons, the base-URI trailing-slash trap, the single-vs-double encoding
difference from `preview_swap_payload`, and the deliberate omission of macOS's
`<`/`/` escapes.

Use `char::from(byte)` and `u8::from_str_radix` in the decoder so no `as` cast and
no `#[allow(clippy::cast_*)]` is needed.

### `crates/wolf-gnome/src/lib.rs`
`pub mod preview_link;` in alphabetical position; a seam-list doc bullet before
the `preview_ui` one.

### `crates/wolf-gnome/src/preview_ui.rs`
Four new public items; **do not restate what `preview_link.rs` documents, link to
it.**
- `DocumentUpdate` + `document_update` (§D), immediately after `needs_reload`.
- `PreviewFilePress { Follow, Refuse(&'static str) }` +
  `preview_file_press(liveness: Option<Liveness>)` — total, no `_` arm,
  `nav_ui::nav_press`'s shape. **No buffer-kind gate**: the linked file's
  Markdown-ness is core's own gate and the front end cannot know it without
  touching the filesystem. Say so.
- `BrowserPress { Open, Refuse(&'static str) }` +
  `browser_press(liveness, markdown_active)` — liveness **before** the Markdown
  gate, `preview_press`'s shape and reason.
- `browser_url(raw: &str) -> Option<&str>` — `None` for empty or all-whitespace.
  `clipboard_ui::clipboard_write`'s shape. **Deliberately no
  `http://127.0.0.1:` prefix check**: that is core's contract and duplicating it
  would give it two owners. Say so.
- Module docs: extend "The blocking calls" with the two new RPCs and their gates;
  extend "Totality is the contract" with the two new total matches; add a short
  section for `document_update`'s couplings.
- **Rewrite `refusal_reasons_are_distinct_and_mid_sentence` in place** to sweep
  the new gates alongside the existing eight. Do not add a second test.

### `crates/wolf-gnome/src/pane_view.rs`
- Import `crate::preview_link`, `preview_ui::DocumentUpdate`, and
  `webkit6::prelude::PolicyDecisionExt` in the **targeted** `use` (never
  `webkit6::prelude::*` — it re-exports `gtk::prelude::*` and makes every
  `WidgetExt` method ambiguous against `adw::prelude::*`). Add the `Cast` trait
  for the downcast; if ambiguous, spell it fully qualified as `shell.rs` does for
  `WidgetExt::clipboard`.
- `PaneWidgets`: add `pending_fragment: RefCell<Option<String>>`, documented as
  belonging to the **view**. Initialize in `new`; clear in `sync_preview_view`'s
  remove branch beside `loaded` and `head_len`.
- `sync_preview_view`: `&self` → `self: &Rc<Self>`. After
  `set_focusable(false)`/`set_can_focus(false)` and before `add_overlay`, connect
  `connect_decide_policy` and `connect_load_changed`, each capturing
  `Rc::downgrade(self)` and `pane_id`.
- New `fn on_preview_link(self: &Rc<Self>, pane_id: PaneId, uri: &str)` — reads
  `win` and `source_path` in **separate statements** (the `on_pane_press` pattern
  at `:1795`; look `win` up per event rather than capturing it), calls `classify`,
  matches **with no `_` arm**:
  - `OpenExternally { uri }` → `launch_uri(...)`.
  - `ScrollToFragment { fragment }` → `scroll_into_view_js`, print and skip on
    `Err`, else `evaluate_javascript`.
  - `PreviewFile { path, fragment }` → gate on `preview_ui::preview_file_press`;
    on `Follow`, `core.preview_file(win, &path)` with the borrow in its own
    statement; on `> 0` write `pending_fragment` **unconditionally**; on `0` print
    the refusal — **never** hand the file to the OS default handler (O4).
  - `Refuse(reason)` → print.
- New `fn on_preview_load_finished(&self, pane_id: PaneId)` — `take()` in its own
  statement, then evaluate.
- New `pub(crate) fn launch_uri(parent: Option<&gtk::Window>, uri: &str, what: &str)`
  — a free function in this file (`shell` already depends on `pane_view`, so the
  direction is acyclic; a new module for one call would need a README entry it has
  no reason to earn). Doc comment records that it is shared with
  `AppShell::preview_in_browser` and why. The callback prints
  `wolf-gnome: {what} launch failed for {uri}: {err}` — the prefix the row-3
  measurement reads.
- `refresh_previews`: receiver → `self: &Rc<Self>`; `due` tuple → `DueRender`;
  pass `source_path` and `update` through.
- `load_preview_document`: new signature and the two body changes. **Rewrite its
  "The head guard" doc section in place** to add the source-identity conjunct and
  the base URI; do not append beside the stale one.
- Add a `read_liveness` helper on `PaneInner` if none exists, mirroring
  `refresh_previews`' three-line short borrow.

Callers `on_render_wake` (`:512`) and `apply_layout_rebuild` (`:633`) are already
`self: &Rc<Self>`, so no further conversion propagates. `load_preview_document`
stays `&self`.

### `crates/wolf-gnome/src/command_action.rs`
`UiAction::OpenInBrowser`; `Command::PreviewInBrowser` → it; module docs
"seventeen" → "eighteen"; `EXPECTED_DISPATCHED_IDS: [&str; 18]` gains
`"preview_in_browser"` (keep sorted); `CASES` row 21 → `Want::Exact(...)`.

### `crates/wolf-gnome/src/shell.rs`
- `perform`: `UiAction::OpenInBrowser => self.preview_in_browser(),`
- New `fn preview_in_browser(&self)` — `read_liveness()` + `preview_shape()`,
  `preview_ui::browser_press`, on `Refuse` print
  `wolf-gnome: preview in browser not opened: {reason}`, on `Open` call
  `WolfCore::open_in_browser` with the borrow in its own statement, filter through
  `browser_url`, hand a `Some` to `pane_view::launch_uri`. A `None` prints the
  same prefix with a **distinct** reason. **The two prefixes must be
  distinguishable** — that separation is what makes the row-3 measurement mean
  anything.

### `crates/wolf-gnome/tests/smoke_headless.rs` (test-writer)
Two new scenario constants and two `#[test]`s, their fixture and OCR constants,
and registrations in `ASSERTED_OCR_TERMS`, `RELAUNCH_FIXTURE_NAMES` and the
`SIDEBAR_FILLER_FILES` doc list. Update `CAGE_LOCK`'s "forty-four cage smoke
tests", "The seven pointer-driven smokes", and the module doc's inventory —
**recount from the source, never adjust by arithmetic.** Reuse `decode_capture`,
`Capture::pixel`, `SampleRect`, `pixel_target`, `pixel_index`, `pointer_scenario`,
`PREVIEW_READBACK_PREFIX`, `PREVIEW_QUERY`, `PREVIEW_SETTLE`. Add no helper that
duplicates one of these.

### The five false comment sites (coder, comment-only)
1. `crates/wolf-core/src/nvim.rs:1386-1389`
2. `crates/wolf-core/src/nvim.rs:1424-1426`
3. `crates/wolf-core/src/nvim.rs:1447-1449`
4. `crates/wolf-core/src/markdown.rs:245`
5. `crates/wolf-ffi/tests/code_block_highlight.rs:21-25`

The correct statement in every case: `wolf_highlight_code_blocks` sits at
**module level** in `runtime/nvim/init.lua`, outside the vendored-tree gate, and
gates **per language** — so it runs on Linux and returns `false` only for a fence
whose language has no parser and `highlights` query on the runtimepath.
**Site 5's SKIP is correct and must not change**: its fixture is a `go` fence and
`go` genuinely has no parser on the Linux/cargo path. Correct the stated
mechanism, never the skip. `crates/wolf-ffi/tests/treesitter_highlight.rs`
(33, 40, 119) is **not** a sixth site — leave it alone.

---

## Test wiring

`wolf-gnome` has no `BUILD.bazel` and appears in no Bazel target, so `cargo
test`'s discovery satisfies the wiring rule for that crate. Every new test in this
unit lives in `wolf-gnome` — in a `#[cfg(test)] mod tests` inside `src/`, or in
the existing `tests/smoke_headless.rs`. **No new `tests/*.rs` file, no
`BUILD.bazel` change.**

Edge cases beyond the falsifiers: a `file:` URI with a `localhost` authority; a
percent-encoded fragment (`#a%20b`); a source path containing a space and one
containing `#`; a `..` that pops past the root; a trailing slash on the source
path; an all-whitespace browser URL; a `PreviewLoad` whose `source_tick` is `0`
(the "nvim has never reported a tick" sentinel `refresh_pane_state` produces).

---

## Order — writers serialized, no test run while an agent edits

Restructured after coder pass 1 to cut round trips: the test-writer's two passes
are merged, since the second only ever depended on the coder's wiring, and the
coder's wiring and measurement passes are merged, since the measurements need the
wiring and nothing else needs to happen between them.

1. **Coder — DONE.** The five source comment sites; then the pure code
   (`preview_link.rs`, `lib.rs`, `preview_ui.rs`'s four new items). Gate green,
   `Cargo.lock` unmoved.
2. **Coder — DONE.** O9's sixth site (`runtime/nvim/init.lua`) and O10's `about:`
   arm.
3. **Coder** — the GTK wiring **and both measurements**, in this order so nothing
   fails to compile between:
   a. `command_action.rs` (this makes `shell::perform`'s `_`-free match an
      `E0004`; that is the point).
   b. `shell.rs` — the `perform` arm and `preview_in_browser`.
   c. `pane_view.rs` — sub-order: `pending_fragment` and its two lifecycle sites;
      `launch_uri`; `load_preview_document`'s new signature **together with** its
      one caller and `DueRender` in one edit; the receiver conversions;
      `on_preview_link`, `on_preview_load_finished`, and the two connections.
   d. The **click measurement** (O6), the **row-3 stderr measurement**, and the
      **nameless-buffer relative-link measurement** (O10), all recorded verbatim.
      No assertion may be written before its measurement exists.
4. **Test-writer** — one pass over everything: the pure unit tables in
   `preview_link.rs` and `preview_ui.rs` (O8, O11-O13), `command_action.rs`'s
   table updates, and the two new `cage` smokes with their guard registrations,
   using the coder's measured constants.
5. **Coder** — the red runs. Every falsifier applied, that test run **alone**,
   the exact failure message recorded, restored, re-run green, restore verified.
6. **Reviewer.**
7. **Technical-writer** — all Markdown, one pass, then `prettier --write` and
   `./tools/lint/markdown_check.sh`.
8. **Gates**, each exit status captured separately, never piped through `tail`.

---

## Documentation ownership — rewritten in place, never appended alongside

1. **`crates/wolf-gnome/src/README.md`** — new `#### preview_link.rs` entry before
   `#### preview_ui.rs`. **Rewrite** `preview_ui.rs`'s Public API to add the four
   new items plus one Decisions bullet for the bufnr-not-tick key (fold the two
   `preview_status` bullets into one to stay in the ~200-word budget).
   **Rewrite** `pane_view.rs`'s Decisions to add the decide-policy
   cancel-everything rule plus the base URI, and the full-load guard; fold
   something to stay in budget. **Check and rewrite** `command_action.rs` and
   `shell.rs` if either states a dispatched count or enumerates actions.
2. **`crates/wolf-gnome/DECISIONS.md`** — **rewrite "Crash and stall recovery" in
   place**: the blocking inventory goes from seven reachable / six from a running
   window to **nine reachable, eight from a running window**, and the gate
   paragraph must name `preview_file_press` and `browser_press`. **This paragraph
   is easy to miss and getting it wrong makes the crate's own inventory a lie.**
   New section **"Following a link in the preview"** — the cross-file "why" no
   single entry can own. **Rewrite the "Present limitations" bullet at line 226**:
   the click-does-not-focus fact stays and gains its counterpart — the same
   pointer routing is what makes a link click work.
3. **`crates/wolf-gnome/tests/README.md`** — **50,328 bytes against a 50,000
   advisory ceiling, already over**, and the additions are ~800-1,200 more.
   **Cut first**: the three paragraphs at lines 105-111 transcribing the internals
   of the three mechanical fixture guards, reduced to one paragraph naming them
   (~1,400 bytes). Second candidate: the four-line `#### ../testdata/paint_golden.png`
   entry. **Then add**: recounted smoke totals (recount, do not do arithmetic);
   the pointer-smoke count; three "What is still unproven" entries (the fragment
   scroll's effect, the external-link browser open, `document_update`'s
   invisibility to the link smoke); and the **recorded row-3 measurement**.
4. **`crates/wolf-core/src/README.md`** — check whether `#### nvim.rs` restates
   the "Linux/cargo no-op" claim; rewrite in place only if it does.
   `#### markdown.rs` does not.
5. **`crates/wolf-ffi/tests/README.md`** — **rewrite line 106 in place.** It says
   "without them the captive treesitter block self-degrades and there is nothing
   to assert." The real reason `code_block_highlight.rs` skips is that the `go`
   parser is absent from the runtimepath.
6. **`feat-list.md`** — lines 145 and 152 `[ ]` → `[x]` **subject to the evidence
   landing**; **line 153 stays `[ ]` per O1**. **Line 161 must be corrected**: it
   claims all three rows are "unimplemented on GTK rather than blocked", which
   this unit falsifies. Add a short paragraph naming what each row's evidence is,
   and for row 3 state plainly that the code is implemented and the final hand-off
   to a system browser is unverified on a compositor with neither browser nor
   portal. Check line 165 still matches the rewritten `DECISIONS.md:226` bullet.
7. **`architecture.md` — no change** (verified: line 181 already names both core
   APIs in front-end-agnostic terms; line 239's "preview scroll-sync is
   macOS-only" stays true — this unit adds no scroll sync).
8. **`build.md` — no change.** No new tool, command or environment variable.
9. **`crates/wolf-gnome/README.md`** — verify it names no dispatched-command or
   smoke count.

## Lint

`#[must_use]` on every new pure function. Every new refusal string non-empty,
mid-sentence, and unique across its module. Statement-per-borrow in the signal
closures to avoid `clippy::significant_drop_tightening` — **do not reach for an
`#[allow]`**. `char::from` / `u8::from_str_radix` in the decoder, never an `as`
cast. `markdown_check.sh` will flag the per-entry word budget on the rewritten
`preview_ui.rs` / `pane_view.rs` entries and the per-file size warning on
`tests/README.md`; both are addressed above.

## Status

Plan approved with decisions O0-O8. Coder step 1 next.
