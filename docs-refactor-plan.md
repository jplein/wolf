# Documentation refactor plan

A plan for reducing the size of this repo's READMEs while increasing their usefulness, and for changing the technical-writer agent's instructions so the docs stop re-inflating.

## Goals (restated)

The docs serve two audiences and have two jobs:

1. **Explain decisions which are not obvious** — if a type avoids `NSFoo` because of an Apple bug that thrashes CPU, that belongs in the README, so a later agent does not "simplify" the code by reaching for `NSFoo`.
2. **Describe the purpose of public types and functions**, plus anything non-obvious a consumer needs to know.

Everything in this plan follows from those two jobs. Content that serves neither is the thing to cut.

## Diagnosis

Measurements taken 2026-08-02 on branch `feat/docs-review`.

**Doc volume is comparable to code volume.** `crates/wolf-macos` is 79 Swift files / 14,251 LOC, documented by 367KB across two READMEs. Repo-wide the READMEs total ~1.15MB.

Largest offenders:

| Bytes   | File                                  |
| ------- | ------------------------------------- |
| 199,629 | `crates/wolf-core/src/README.md`      |
| 188,760 | `crates/wolf-macos/README.md`         |
| 178,714 | `crates/wolf-macos/Sources/README.md` |
| 92,659  | `crates/wolf-ffi/README.md`           |
| 88,392  | `crates/wolf-core/README.md`          |
| 80,104  | `runtime/nvim/README.md`              |

### Four compounding causes

**1. Two READMEs document the same files.** `crates/wolf-macos/README.md` (188K) and `crates/wolf-macos/Sources/README.md` (178K) both carry per-file entries for `PickerView.swift`, `BufferPickerView.swift`, `PaneContainerView.swift`, and the rest — near-identical prose that has _drifted_: only 298 of roughly 1,450 sentences match exactly. Per this repo's own rule that a doc/implementation divergence is a bug, a drifted duplicate pair means at least one of the two is currently a bug, and an agent reading one has no way to know the other disagrees.

**2. Changelog voice.** There are 33 instances of "no longer / now supported / the old / renamed from / this increment" in `crates/wolf-macos/README.md` alone (31 in `wolf-core/src/README.md`, 28 in `Sources/README.md`). The section `## Known limitations (this increment)` ends in a single **34,000-character** paragraph beginning "**Multiple windows now supported**…". These sentences describe deltas from a past state rather than the present system. This is the primary growth engine: an agent appends "X now does Y, the old Z is gone" instead of editing the description of X. Git already holds that history.

**3. Implementation transcription.** The `PickerView.swift` entry documents private method call ordering, private helpers, and layout-constant derivation — content that re-reading the file would supply anyway, and that goes stale on every refactor.

**4. Formatting that serves neither audience.** `crates/wolf-macos/README.md` is 188KB in 161 lines; the longest single line is 34KB. For humans it is unscannable. For agents it is worse than it looks: there are no `####` anchors to grep or cite, no retrieval unit smaller than a multi-KB blob, and diffs carry no information — recent commits show "7 added / 7 removed", meaning seven mega-paragraphs were rewritten with no visible delta. There are also 629 ALL-CAPS emphasis words in that one file; when everything is emphasized, nothing is.

## Recommendations

### 1. One file, one owner

Establish and write down the split:

- `crates/<crate>/README.md` — the outside view only: what the crate is, its public entry points, cross-cutting decisions, threading model, build. **No per-file entries.**
- `<source-dir>/README.md` — the files in _that directory only_, one entry each.

This satisfies the "every source directory has a README" rule while removing the duplicate axis. `crates/wolf-macos/Sources/README.md` currently covers both `WolfKeyMap/` and `WolfMacOS/`; those are separate directories and should each own their own file. Cross-reference by link, never by restating.

### 2. Fix the per-file entry shape

A fixed template with a hard budget beats a prose paragraph, because it makes "what does not belong" obvious at a glance:

```markdown
#### `PickerView.swift`

Purpose: 1–3 sentences.

**Public API** — signatures a consumer calls, plus only what the signature cannot say.

**Decisions** — bullets, each stating the constraint _and its reason_. Only things a reasonable agent would otherwise undo.

**Gotchas for consumers** — non-obvious interactions with other components.
```

### 3. Ban the changelog voice outright

Docs are present-tense descriptions of the current system. No "now", "no longer", "the old", "previously", "renamed from", "this increment". Retire "Known limitations (this increment)" as a concept — real limitations are present-tense limitations.

### 4. Consider splitting durable from volatile content

The decisions content ("we do not use `NSSearchField` because…") has a multi-year half-life; per-file API descriptions have a multi-week one. A per-crate `DECISIONS.md` keeps the durable material from being churned or lost inside the churn, and gives agents one cheap file to read before touching anything.

## Worked example: `PickerView.swift`

Today the entry is 3,473 characters on a single line. Nearly all of its value is three sentences buried mid-paragraph. Extracted and reshaped:

```markdown
#### `PickerView.swift`

The floating file picker: a search bar over an `NSTableView`, hosted in its own borderless non-activating child `NSPanel` composited in front of the owning window.

**Public API**

- `open()` / `close()` / `isOpen`
- `weak var host: PickerPanelHosting` — set by `WolfWindowController`; drives panel present/dismiss/refit.

**Decisions**

- The search bar is hand-built (rounded container + magnifier `NSImageView` + borderless `NSTextField`), **not** an `NSSearchField` — its text cell clips at the font size used here.
- It lives in its own child `NSPanel` rather than as a subview of the pane container, because it must render above the preview `WKWebView`.
- Chrome colors are assigned as `CGColor`s, which do **not** follow appearance changes. `viewDidChangeEffectiveAppearance()` re-resolves them against the current `effectiveAppearance`. Do not "simplify" this to a one-time `NSColor` assignment — the chrome will freeze at construction and ignore dark/light switches.
- Streaming-traversal refreshes go through `PickerRefreshCoalescer`, not directly. A large tree emits hundreds of per-batch wakes; each direct refresh is a full re-rank. Keystroke filtering and `open()` refresh immediately — only traversal wakes coalesce.
- `close()` sets `isOpen = false` before teardown so a re-entrant `windowDidResignKey → close()` is a no-op.

**Gotchas** — while open, the panel is key and the text field holds first responder, so `PaneContainerView.keyDown` never fires and keys are not forwarded to Neovim.
```

That is roughly 1.4K, scannable, and it retains every fact that would stop a future agent from breaking something.

Dropped: call ordering inside `open()`, `updateResultsLayout()` / `refitPanel()` mechanics, row-inset derivation, the `FfiFileEntry` → `PickerRow` field mapping, the Up/Down/Enter/Esc key list (obvious for a picker), and the "the old `viewDidMoveToSuperview` pinning is gone" changelog note.

## Technical-writer agent instructions

The current prompt (`.claude/agents/technical-writer.md`) is 24 lines and says, in effect, "look for changes and update the docs." It has no notion of deletion, no budget, no style rules, and no anti-goals — so appending is the only behavior it is steered toward. Suggested replacement for its `## Your output` and `## Rules` sections:

```markdown
## Your output

For each changed source file, find its existing README entry and **edit that entry in place**. Read it first. Do not append new sentences alongside stale ones — if the entry is now wrong, rewrite it; if it describes something deleted, delete it.

You are expected to run net-negative as often as net-positive. At the end, report the byte delta of every file you touched. A README that only ever grows is a defect.

## The two-question test

Every sentence you write must pass one of these. If it passes neither, cut it.

1. Does it explain a decision that a future agent, seeing only the code, would reasonably "simplify" and thereby break? (Include the reason, not just the rule.)
2. Does a _consumer_ of this type need it, and is it absent from the signature?

## Never document

- Private methods, internal call ordering, or control flow the code shows plainly
- Layout constants, colors, timings, magic numbers (cite the named constant instead)
- Restatements of a signature in prose
- History: what something used to be, was renamed from, or no longer does. Present tense only — git holds the past. Never write "now", "no longer", "the old", "previously", "this increment"
- Anything documented in another README. Link to it instead

Rule of thumb: if 30 seconds of reading the source answers it, do not write it. Write only what reading the source would _not_ tell you.

## Ownership

- `crates/<crate>/README.md` — crate-level only: purpose, public entry points, cross-cutting decisions, threading, build. No per-file entries.
- `<source-dir>/README.md` — only the files in that directory, one entry each.

Never describe the same file in two READMEs.

## Format

- One `####` heading per file, so entries are greppable and citable.
- Follow the entry template: Purpose / Public API / Decisions / Gotchas.
- Budget: ~200 words per file entry. Exceeding it means you are transcribing implementation — cut, do not restructure.
- Paragraphs are at most 4 sentences, separated by blank lines. No paragraph over ~600 characters.
- No ALL-CAPS emphasis. Use bold sparingly, for genuine warnings only.
- No hard line breaks inside a paragraph; run `prettier --write` when done.

## Rules

- You do not build, edit source files, or run tests.
- Only update Markdown files.
- Where the README and the implementation disagree, the implementation is the fact and the README is the bug — fix the README, and say so in your report.
```

That last line deliberately inverts CLAUDE.md's "documentation is the source of truth." That rule is correct as a _design_ contract, but wrong for this agent, which runs _after_ the code has changed. Being explicit about the direction prevents the writer from preserving stale prose out of deference.

## Enforcement

`tools/lint/` already has an advisory-target pattern (`manual`-tagged, non-gating, CHECK plus FIX). A `//tools/lint:markdown` target in the same style could mechanically catch all four failure modes: maximum line length, banned changelog words, ALL-CAPS runs, and a per-README byte ceiling that trips a warning. Rules that are not checked will drift back, especially with agents doing the writing.

Note that `.prettierrc` sets `proseWrap: never` with `printWidth: 300` for Markdown, so prettier will not break up long paragraphs — it preserves them as single lines. Shortening paragraphs is an authorial act, not a formatting one, which is why it needs to be a rule in the agent prompt rather than a formatter setting.

## Suggested order

1. **Land the rules first** — the ownership split and the entry template in `CLAUDE.md`, and the rewritten technical-writer prompt. Otherwise the next agent run re-inflates whatever gets cleaned up.
2. **Do `wolf-macos` first**, as the worst case: delete the per-file entries from `crates/wolf-macos/README.md`, keeping them in `Sources/`.
3. **Rewrite entries file-by-file against the template**, verifying each claim against the source. The two files have drifted, so neither can be trusted wholesale. This is the expensive step and a good fit for parallel per-file agents.
4. **Repeat per crate, worst-first**: `crates/wolf-core/src` (199K), `crates/wolf-ffi` (92K), `runtime/nvim` (80K).
