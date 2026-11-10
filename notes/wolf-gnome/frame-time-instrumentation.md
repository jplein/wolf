# Frame-time instrumentation

Last verified at commit `6298d01fb761da1bfdd64a0a25671a40b9d4e525`.

`WOLF_FRAME_STATS` arms an off-by-default measurement of the keystroke-to-pixel path, split the way everything here is split: a pure seam and a process-global glue module, both described in [`src/README.md`](../../crates/wolf-gnome/src/README.md#frame_statsrs). [`build.md`](../../build.md) owns how to run one.

**The bucket set is derived from the GTK draw path rather than transliterated from macOS, and it has two top-level families rather than one tree.** `queue_draw` only marks a widget dirty; GTK invokes the draw funcs from the frame clock after the wake handler has returned, so the `draw.*` buckets sit beside `wake` and not inside it. Summing the two families double-counts nothing; summing a parent with its own children counts those children twice.

**No timer, and one flush at exit.** "Threading and the wake" says nothing polls, and a periodic report would break that whenever the flag was set. The emit check runs in the drain-loop body after the `wake` region closes, so the emit's own cost is never inside a measured region, plus one non-periodic flush after `Application::run_with_args` returns.

Two consequences, stated rather than hidden. A report covering the final interval before the application goes idle waits for the next wake. And a process killed by a signal — which is what every `cage` smoke's teardown does — never reaches the exit flush, which is why `WOLF_FRAME_STATS_FILE` is appended to per report rather than buffered. With N windows there are N drain loops over one accumulator, and whichever wakes first after the interval elapses emits the process-wide report.

**Variable names, accepted values, the line prefix and the column layout are the macOS ones exactly.** One documented surface serves both front-ends, and the two reports then `diff` as text, which is the only cheap way to compare where each spends a frame.

**The `wakes/key` footer is an upper bound on the wakes attributable to typing, never an attribution.** Mouse, scroll, resize and out-of-band core notifications all inflate it. Under the headless recipe there is no pointer and the toplevel cannot be resized, so only the notifications are left — closer to attribution, and still not attribution.

**Diagnostic counters are the one carve-out from this crate's no-process-global-state rule.** That rule exists so two `WolfCore`s in one process are independent, which counters no application behaviour reads cannot threaten. With the flag unset the whole instrument is one acquire load and a branch per call site, and the accumulator is never allocated.

**A report may never take the application down.** `eprintln!` panics when the write to stderr fails, so every write goes through `write_all` with the `Result` discarded; a lost diagnostic beats a diagnostic that aborts. Nothing on the emit path touches Cairo, a widget or the core.

## The measured reading

Measured on an `AMD Ryzen AI MAX+ 395` under NixOS: release profile, headless `cage` at 1280x720, iA Writer Mono S at 10 pt over a 2068-line prose buffer, about 1230 keystrokes a run at a two-second report interval. Scenario A is one pane on a 90x34 grid with no preview; scenario B is two panes on 59x34 with a live Markdown preview. Two runs each, and the run-to-run spread is 0.6 % (A) and 2.2 % (B), so nothing under about 2 % is a signal.

**`draw.grid` was indicted, and the repair is [`render.rs`](../../crates/wolf-gnome/src/README.md#renderrs)'s one layout per paint.** `draw.pane`'s p95 median sat at 11.51 ms (A) and 12.16 ms (B) against an 8 ms frame-budget threshold, with 99.2 % of that parent inside `draw.grid`.

Over the identical recipe with the two binaries alternated in one session, `draw.grid`'s count × mean fell from 496.8 to 412.6 ms/s in A (−17.0 %) and from 176.6 to 147.2 in B (−16.7 %), and `draw.pane`'s p95 median by 19.7 % and 15.4 %.

**`draw.pane` sits over that 8 ms threshold even so**, at a 9.50 ms p95 median in A, and the remainder is glyph rasterization at about 1.8 µs a glyph. Getting under the threshold needs damage tracking or a shaped-glyph cache, each a unit of its own.

**`prev.render` was indicted too, and the repair is [The preview render's budget](../wolf-gnome-decisions.md#the-preview-renders-budget).** The suspicion "Crash and stall recovery" records — that it is the one blocking RPC a burst of typing reaches — was confirmed at 988 blocking render calls, a 6.93 ms mean against 1238 keystrokes, 15.6 % of wall clock, `wake`'s p95 at 9.1 ms and its worst interval at 34 ms.

The coalesced reading was taken against a baseline of its own, with both binaries built and alternated in one session rather than compared against the paragraph above. Over two runs each of scenario B, `prev.render` falls from 1186 / 1188 calls and 6490.6 / 6527.1 ms to 90 / 90 calls and 201.9 / 210.2 ms across a 55 s run — 11.84 % / 11.91 % of wall clock against 0.37 % / 0.38 % — and `wake`'s p95 median from 8.55 / 8.50 ms to 0.15 / 0.22 ms.

**The two baselines must not be spliced into one improvement figure.** That session's own baseline reads 11.9 % where the paragraph above records 15.6 %: the shape reproduces, at 1187 renders against 988 and a `wake` p95 of 8.5 against 9.1 ms, while the render mean is 5.48 against 6.93 ms, over a generated rather than byte-identical prose fixture and a denominator carrying about 22 s of non-typing wall clock. Measuring both binaries in one session is what makes the comparison sound.

**The worst interval does not improve, and no claim may say it does.** `wake`'s max reads 30.74 / 34.00 ms against 32.78 / 33.64 ms, and in all four runs it lives in the cold preview open, where the single `prev.load` sample that opens a pane spends 10.3–10.9 ms on the WebProcess spawn and the full `load_html`. Coalescing does not touch the cold open, so the p95 median is the statistic to quote.

**Total main-thread busy falls by about 3.5 points rather than by the render's whole share.** `draw.pane`'s count rises 78 % over the same wall clock at an unchanged per-frame mean — frames GTK wanted to deliver and could not while the thread sat in a blocking RPC — and the summed families go from 29.7 % to 26.1 %. The blocking RPC is gone and the wake path is 98 % cheaper; the freed CPU is largely spent painting a smoother editor rather than going idle.

**Nothing else is indicted.** `wake` in scenario A reads a 0.11 ms p95, and `wake.tree`, `wake.shell`, `wake.state`, `pane.state`, `draw.status`, `draw.cmdline` and `prev.load` all sit below the threshold at which a repair could pay for itself. `draw.read` reads 0.00 ms, which is the measurement behind the claim that `read_snapshot` copies nothing and contends with no tokio writer.
