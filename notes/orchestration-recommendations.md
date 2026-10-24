# Orchestration recommendations: how to stop writing tests that prove nothing

Last verified at commit `d1470d781355d548adde2a9da86e692a2d03dc60`.

Recommendations for changing how the subagents spawned by the `/orchestrate` skill work, drawn from the Linux parity effort. They are aimed at one specific, repeatedly observed failure: a test that is green, looks correct on inspection, and would stay green if the feature it names were deleted.

This is not a hypothetical risk. Over five work units, **eight** such tests were found. Every one of them passed code review. Every one of them was only caught by deliberately breaking the feature and watching the test not notice.

## Why this document exists in this form

A standalone recommendations file only pays off if its contents end up in `.claude/agents/*.md` and in the `/orchestrate` workflow itself. Advice that lives only here gets read once and never applied. Each section below therefore ends with the concrete change to make to an agent definition.

Those changes are applied. This file remains as the evidence behind them: the agent definitions carry the rule, and a rule without its reason is the first thing a later editor trims. Where each one landed:

| Recommendation                                   | Lives in                                                              |
| ------------------------------------------------ | --------------------------------------------------------------------- |
| Prove every new test can fail                    | `.claude/agents/coder.md`, "Proving a test can fail"                  |
| A tool's exit code is not proof of its effect    | same section                                                          |
| Assertions state what a failure means            | `.claude/agents/test-writer.md`, "Writing an assertion that can fail" |
| Premises about third-party behavior get measured | same section                                                          |
| Name the falsifying revert per behavior          | `.claude/agents/architect.md`, the test-plan item                     |
| Read tests adversarially                         | `.claude/agents/reviewer.md`, "Tests"                                 |
| Failure evidence gates unit completion           | `.claude/commands/orchestrate.md`, step 7                             |
| Settle disagreements by measurement              | `.claude/commands/orchestrate.md`, Rules                              |

Two recommendations were already covered by the definitions and needed no change: wiring every new test file into the target that runs it, and the discipline that an agent with no shell states what it could not verify rather than implying it did.

## The evidence

| # | Test | Why it proved nothing | Failure mode |
| --- | --- | --- | --- |
| 1 | `picker_opens_and_loads_selected_file_under_cage` | The fixture was named `pickertarget.md` and the assertion looked for the OCR term `pickertarget`. The sidebar renders every filename in the working directory, so the term was on screen whether or not the picker ever opened. It demonstrably passed while the picker never opened. | Fixture contamination |
| 2 | `vertical_split_shows_two_panes_under_cage` | Same shape: fixtures `paneleft.md` and `paneright.md` against asserted terms `paneleft` and `paneright`. | Fixture contamination |
| 3 | `colon_qa_closes_the_window_under_cage` | It asserted the process was gone. A panic in the GTK shutdown path makes the process just as gone as a clean exit, so the exact regression it was written to catch would have passed it. | Indistinguishable outcomes |
| 4 | Pointer injection via `wlrctl` | The tool exits 0 for every pointer command and delivers zero events to the client. Building on it would have produced a whole suite of green pointer tests with no pointer input. | Vacuous tool success |
| 5 | Pointer injection via `swaymsg ... cursor` | Returns `{"success": true}` and delivers zero bytes. The seat reports `"capabilities": 0, "devices": []`. | Vacuous tool success |
| 6 | `drag_selects_a_range_under_cage` (first version) | Passed 3 of 3 runs with the drag handler neutered. Its doc comment stated "Vim enters visual mode only on a drag" — which is false for the pinned Neovim: a release at a different cell enters Visual mode on its own. | False premise |
| 7 | Restart attempt-cap test | It counted child processes named `nvim`. A refused `WOLF_NVIM_BIN` wrapper is `/bin/sh`, which exits before its `exec`, so the count never matched. With the cap genuinely removed, the app made 75 restart attempts in 4 seconds and the test stayed green. | Wrong observable |
| 8 | Crash-overlay button geometry | The click landed at the button rect's centre. `set_size_request` is a minimum on a centre-aligned button, so a 40 px button and a 320 px button render centred on the same point and both pass. | Wrong observable |

Two related findings, same root cause, outside the test suite: the claim that `Ctrl+Shift+P` "preserves every Neovim key" was false (it encodes to `<C-p>`), and the smoke harness's long-standing claim to "poll until the window maps" was false (it polls only that the compositor came up). Both were prose invariants nobody had ever measured.

## What the failure modes have in common

None of these are sloppiness. Each is a **correct-looking assertion resting on an unexamined assumption**:

- that a fixture is inert (it was rendered elsewhere on screen),
- that two outcomes are distinguishable (clean exit and panic are not),
- that a dependency behaves as believed (Neovim's visual-mode trigger),
- that an observable is the thing being tested (process name, rect centre),
- that a tool's exit code reflects its effect.

A reviewer reading the diff cannot see any of these, because the test reads correctly. Only execution against a deliberately broken build exposes them. **Therefore the only reliable defence is mechanical, and it belongs to the agent that runs tests.**

## Recommendation 1 — the coder must prove every new test can fail

This is the single highest-value change. A test that has never been observed failing is not yet known to be a test.

For each new or materially changed test, the coder should:

1. Run it and observe green.
2. Revert the feature under test by the smallest edit that disables it — not by deleting the test, and not by editing the assertion.
3. Re-run **only that test** and observe red.
4. Restore the file and verify the restoration by checksum, not by eye.
5. Re-run and observe green again.
6. Report the verbatim failure output from step 3.

Step 6 matters as much as step 3. "I verified it fails" is a claim; the panic message is evidence. In this effort, two tests were reported as verified and were later found to have stayed green — the check had been described rather than run.

Where the revert is not a one-line edit (a missing capability rather than a wrong branch), say so explicitly and describe what was done instead. Do not silently skip.

**Change to `.claude/agents/coder.md`:**

> For every test you add or materially change, you must demonstrate it can fail. Run it green; then disable the feature under test with the smallest possible edit (never by touching the test or its assertions); re-run that test alone and capture the verbatim failure; restore the source file and confirm the restoration with a checksum; re-run and confirm green. Report the captured failure output for each test. A test you have only ever seen pass is not evidence and must not be reported as coverage. If a test cannot be made to fail, say so plainly and explain why — that is a finding, not a formality to skip.

## Recommendation 2 — never accept a tool's exit code as proof of its effect

Two of the eight incidents were external tools reporting success while doing nothing. This generalises: exit codes describe whether a program ran, not whether it worked.

When a test depends on an external tool having an effect (input injection, a screenshot, a process signal, a network call), the coder should confirm the **effect** independently at least once — read back the state the tool was supposed to change, or observe the event on the receiving side — before any test is built on that tool.

**Change to `.claude/agents/coder.md`:**

> When a test depends on an external tool producing an effect, verify the effect independently before relying on the tool. An exit code of 0 means the program ran, not that it did anything. Confirm on the receiving side — read back the changed state, or observe the event arriving — and record how you confirmed it.

## Recommendation 3 — the test-writer must state, per assertion, what it discriminates

The best assertion messages in this codebase read like a decision tree: they name each way the test can fail and what each one implies. The relaunch smokes distinguish four outcomes in one message — the placeholder buffer alone means nothing was restored; the right names in the wrong order means the depth-first zip is inverted; too few names means the rebuild produced the wrong topology; and a look-alike fixture means the application reopened the first file it found rather than the saved one.

Writing that forces the author to ask what else could produce a pass — which is exactly the question that catches fixture contamination and indistinguishable outcomes at authoring time rather than three units later. It also makes the eventual failure diagnosable without re-deriving the test's intent.

A specific sub-rule with a mechanical guard already in this repo: **when a test asserts on a file's content, the file's name must not contain the asserted term.** Incidents 1 and 2 were both this.

**Change to `.claude/agents/test-writer.md`:**

> Every assertion message must state what a failure would mean — ideally enumerating the distinguishable failure modes and what each implies. Before writing an assertion, ask what else could satisfy it besides the feature working, and either eliminate that path or assert against it explicitly. When asserting on a file's content, never name the file after the asserted term. Prefer assertions that include a discriminating negative (the expected value is present and a specific look-alike is absent).

## Recommendation 4 — premises about third-party behaviour must be measured

Incident 6 is the instructive one: the assertion was well-formed, the fixture was clean, the test was reviewed — and it was worthless because a one-sentence belief about Neovim's behaviour was wrong. No amount of care about test _structure_ would have caught it.

Any premise about how a dependency behaves — the editor, the toolkit, the compositor, the shell — should be stated in the test's documentation and measured once, directly, rather than assumed. This repo runs a real Neovim in tests specifically so such premises can be checked.

**Change to `.claude/agents/test-writer.md`:**

> Where a test depends on how a third-party component behaves, state that premise explicitly in the test's documentation and have it measured against the real component rather than assumed. A premise you did not verify is the most likely reason a well-formed test proves nothing.

## Recommendation 5 — the architect should specify the falsifying revert

The coder cannot run Recommendation 1 well without knowing what "disable the feature" means for each acceptance criterion. The architect already decomposes the work; naming the revert alongside each criterion costs one line and makes the mutation check mechanical instead of improvised.

**Change to `.claude/agents/architect.md`:**

> For each acceptance criterion in the plan, name the smallest source edit that would falsify it — the revert the coder will apply to prove the corresponding test can fail. If you cannot name one, the criterion is probably not testable as stated; reformulate it.

## Recommendation 6 — the reviewer should read tests adversarially

The reviewer is read-only and cannot run anything, so it cannot catch these directly. It can, however, ask the question that exposes them on paper, and it caught one this way (the `:qa` smoke's panic blindness) before execution did.

**Change to `.claude/agents/reviewer.md`:**

> Review tests adversarially, not only for correctness. For each new test ask: what could make this pass without the feature working? Check specifically for fixtures that can satisfy their own assertion, pass states indistinguishable from a failure state, observables that are adjacent to the behaviour rather than the behaviour itself, and unstated premises about third-party behaviour. Confirm the coder reported captured failure output for every new test, and treat its absence as an unmet requirement rather than an oversight.

## Recommendation 7 — add the check to the unit completion step

The requirement only holds if a unit cannot be declared done without it. Add to the `/orchestrate` workflow's completion criteria, alongside the existing gate:

> A unit is not complete until every new or materially changed test has recorded evidence of failing when its feature is disabled, with the verbatim failure output captured in the coder's report. Report the evidence per test. If any test lacks it, say so explicitly rather than reporting the unit as complete.

**Change to `.claude/commands/orchestrate.md`:** add this as an explicit numbered step between the coder loop and the reviewer invocation, and repeat it in the completion report requirements.

## Recommendation 8 — settle disagreements by measurement, not another round

Not a test-quality item, but the same epistemics, and it cost real time. The orchestrator is forbidden from running commands, so it holds only second-hand reports and has no way to break a tie when two agents disagree. In one unit it dispatched several rounds of edits to fix a file that had already been fixed, because it was working from a stale snapshot; two subagents told it so independently and it believed neither. A single command settled it in seconds.

**Change to `.claude/commands/orchestrate.md`:**

> When two agents disagree about the state of a file or a test, do not dispatch another round of edits. Have the coder re-run the relevant command and paste its verbatim output, and settle the question on that evidence. Prefer one measurement to any number of confident reports.

## What this costs

The mutation check adds a revert, a targeted test run, and a restore per new test. In this effort that was minutes per unit against a suite where the slowest end-to-end tests run in tens of seconds — and it caught eight tests that would otherwise have shipped as false confidence, two of them hiding real defects (an uncapped restart loop that fired 75 times in 4 seconds, and a focus bug that killed all keyboard input).

The cost scales with the number of new tests, not the size of the suite, because only the test under examination is re-run. It is the cheapest verification in the workflow and the only one that has ever caught this class of defect.
