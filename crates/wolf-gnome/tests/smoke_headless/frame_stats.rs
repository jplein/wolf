//! The frame-time instrumentation smoke: typing bursts with `WOLF_FRAME_STATS` armed, read
//! from the report file the running application appends to.

use super::*;

// --- The frame-time instrumentation smoke ---

/// FRAME-STATS smoke: the report file the running application appends to, under the
/// resolved `XDG_RUNTIME_DIR` rather than in the smoke's cwd.
///
/// Outside the working directory for the reason every readback in this binary is: the
/// tree sidebar renders the cwd by name, so a file dropped there is page text. This
/// smoke captures no image, so that is belt and braces — what it really buys is that
/// the file cannot become a picker candidate or a sidebar row that changes what the
/// application is doing while it is being measured.
const FRAME_STATS_FILE_NAME: &str = "wolf_gnome_frame_stats.txt";

/// The value handed to `WOLF_FRAME_STATS_INTERVAL`, in seconds, as the string the
/// environment carries.
///
/// One second rather than the 5 s default so that four typing bursts separated by
/// `BURST_GAP` close several report intervals inside a scenario that still runs in
/// about ten seconds. **It is coupled to `BURST_GAP` in [`FRAME_STATS_SCENARIO`]**:
/// the gap must exceed this, or two bursts land inside one interval and the scenario
/// produces fewer reports than it reads as producing.
///
/// `preview_pane_renders_and_edit_source_returns_under_cage` hands the same value
/// to its own launch for a different reason: its assertion 12 SUMS `prev.render`
/// across every report, so how many reports a run is cut into cannot move the total.
/// All that smoke needs is that at least one interval closes before the teardown
/// signal, which any value well below its runtime satisfies.
pub(crate) const FRAME_STATS_INTERVAL_SECS: &str = "1";

/// The text typed into the buffer in each burst, after the insert-mode `i`.
///
/// Nineteen characters of plain lowercase, so one burst is twenty-one keystrokes
/// (`i`, nineteen letters and spaces, `Escape`) and four bursts are eighty-four. It
/// is deliberately ordinary prose: nothing in this smoke reads a pixel or OCRs
/// anything, so the text has no assertion to satisfy and no fixture-name hazard —
/// what it has to be is a realistic keystroke stream through
/// `window::wire_keys`.
const FRAME_STATS_BURST_TEXT: &str = "the quick brown fox";

/// The floor for the summed `keys=` counter across every report footer.
///
/// The scenario types eighty-four keystrokes, and the last burst's are expected to
/// reach no report at all — the teardown kills the app with a signal, so the exit
/// flush never runs and the final interval is never emitted. Twenty is far below the
/// sixty-odd a healthy run reports and far above the zero a build with
/// `note_keystroke` deleted reports, which is the discrimination this floor is for.
/// A tighter number would be a claim about `wtype`'s timing rather than about the
/// instrument.
const FRAME_STATS_MIN_KEYS: u64 = 20;

/// The grep prefix every emitted line carries, re-typed here as a LITERAL rather
/// than imported from `wolf_gnome::frame_stats::LINE_PREFIX`.
///
/// Same discipline as [`EXPECT_SMOKE_APPEARANCE`]:
/// reading the production const would make this file follow a renamed prefix
/// silently, and the prefix is the whole reason a report survives being interleaved
/// with anything else on stderr.
const FRAME_STATS_LINE_PREFIX: &str = "wolf-frame-stats ";

/// The line `frame_metrics::install()` writes once when it finds the flag set.
///
/// Only the fixed part is stated: the interval it prints is a formatted duration and
/// this smoke asserts no wall-clock magnitude anywhere.
const FRAME_STATS_ENABLED_NEEDLE: &str = "wolf-frame-stats enabled interval=";

/// The FIRST report's header line, up to its formatted window length.
///
/// `#1` rather than `#` so this cannot be satisfied by a file that starts at some
/// other index, and the two spaces before `window=` are part of the pinned layout
/// (see the seam's `format_produces_the_exact_pinned_table`).
const FRAME_STATS_FIRST_REPORT_NEEDLE: &str = "wolf-frame-stats #1  window=";

/// The bucket label whose count is identically the wake counter.
const FRAME_STATS_WAKE_LABEL: &str = "wake";

/// Every bucket the live keystroke-to-pixel path must have recorded at least once,
/// re-typed as LITERALS.
///
/// Deliberately not `Bucket::ALL.map(Bucket::label)`: a bucket DELETED from the enum
/// would shrink that list and this loop would silently check fewer labels while
/// staying green, which is precisely the vacuity this table exists to prevent. It is
/// also the whole of what the smoke says about wiring — that each of these regions
/// was entered and timed at least once during a real typing session — so the list is
/// stated where a reader meets the assertion.
///
/// `prev.render` and `prev.load` are absent on purpose; see
/// [`FRAME_STATS_ABSENT_LABELS`].
const FRAME_STATS_REQUIRED_LABELS: [&str; 13] = [
    "wake",
    "wake.tabs",
    "wake.tree",
    "wake.shell",
    "wake.state",
    "pane.wake",
    "pane.state",
    "pane.prev",
    "draw.pane",
    "draw.read",
    "draw.grid",
    "draw.status",
    "draw.cmdline",
];

/// The two buckets that must NOT appear anywhere in the file, and the non-vacuity
/// half of [`FRAME_STATS_REQUIRED_LABELS`].
///
/// Both are inside the per-preview loop of `pane_view`'s preview refresh, and this
/// scenario never opens a preview, so both stay at zero count for the whole run.
/// Without this half, a `summarize` that emitted every bucket regardless of count
/// would satisfy all thirteen required labels while proving that none of them was
/// ever recorded.
const FRAME_STATS_ABSENT_LABELS: [&str; 2] = ["prev.render", "prev.load"];

/// The FRAME-STATS scenario, run as one `sh -c` string inside one `cage`.
///
/// Modelled on `SCENARIO` — launch, poll, settle, type, tear down — with `grim`,
/// `tesseract`, every fixture file and the pointer helper removed: the evidence this
/// smoke reads is a file the running application wrote, not a pixel.
///
/// Four bursts separated by `BURST_GAP` rather than one continuous stream, because
/// this front-end arms no timer: a report is emitted from the last statement of the
/// drain-loop body, so it rides a WAKE. With the application idle between bursts, the
/// wake that carries a report is the first keystroke of the FOLLOWING burst — which
/// is why four bursts are typed to close several intervals, and why the last burst's
/// keystrokes are expected to reach no report at all.
///
/// Exit codes: 11 the app died before it was ready for input, 12 it died between
/// bursts, 13 it died during the final idle. Each is distinct because "the app is
/// gone" and "the app shut down" are the same observation otherwise, and because a
/// dead app produces exactly the empty file that a disabled instrument produces.
const FRAME_STATS_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
#
# BURST_GAP -- the idle between typing bursts, and the one knob that is this
# smoke's own. It must EXCEED WOLF_FRAME_STATS_INTERVAL (set on the Rust side; see
# FRAME_STATS_INTERVAL_SECS), because nothing in this front-end polls: a report is
# emitted from the drain loop, so it rides a wake, and with the app idle between
# bursts the wake that carries it is the first keystroke of the NEXT burst. Below
# the interval, two bursts land inside one report and the run produces fewer
# reports than it reads as producing.
BURST_GAP=1.2

# Launch the real app in the background; remember its pid.
"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome frame-stats smoke"

# Four bursts of insert-mode typing, each followed by an idle longer than the
# report interval, so at least two report intervals close with keystrokes inside
# them.
for burst in 1 2 3 4; do
    if ! kill -0 "$app_pid" 2>/dev/null; then
        echo "wolf-gnome frame-stats smoke: app died before burst $burst" >&2
        exit 12
    fi
    wtype -- "i$FRAME_STATS_BURST"
    wtype -k Escape
    sleep "$BURST_GAP"
done

# One more idle window. The report covering the third burst was emitted by the
# fourth burst's first wake; this is simply margin so the write has certainly
# landed before the teardown signal.
sleep "$SETTLE"

if ! kill -0 "$app_pid" 2>/dev/null; then
    echo "wolf-gnome frame-stats smoke: app died during the final idle" >&2
    exit 13
fi

# Best-effort teardown: the app may already be gone. It is killed with a SIGNAL,
# so `frame_metrics::flush_final` never runs -- which is exactly why the file is
# appended to PER REPORT rather than buffered to exit.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// One `label count mean p50 p95 max` row of a frame-stats report.
///
/// The five numeric fields are kept as RAW strings so that assertion 10 — "every
/// bucket row parses" — is a real assertion rather than something the parser already
/// performed and turned into a panic somewhere else.
struct FrameStatsRow {
    /// The bucket label: the first whitespace-separated token after the prefix.
    label: String,
    /// The 1-based line number in the report file, for failure messages.
    line_no: usize,
    /// The count field, which carries a trailing `*` when the percentiles are
    /// windowed.
    count_field: String,
    /// Mean, p50, p95 and max, in that column order, in milliseconds.
    millis_fields: [String; 4],
}

/// One report block: its header index, its bucket rows and its footer counters.
pub(crate) struct FrameStatsBlock {
    /// The report index from the `#<n>` header.
    index: u64,
    /// The header's 1-based line number, for failure messages.
    line_no: usize,
    /// The bucket rows between the column row and the footer.
    rows: Vec<FrameStatsRow>,
    /// `keys=` and `wakes=` from the footer. `None` until the footer is read, and a
    /// block that still holds `None` at the end of the file is a truncated report.
    footer: Option<(u64, u64)>,
}

impl FrameStatsBlock {
    /// The `keys=` counter this block's footer carried.
    fn keys(&self) -> u64 {
        self.footer.map_or(0, |(keys, _)| keys)
    }

    /// The `wakes=` counter this block's footer carried.
    fn wakes(&self) -> u64 {
        self.footer.map_or(0, |(_, wakes)| wakes)
    }
}

/// Parses a count field, tolerating the truncation marker the formatter appends
/// INSIDE the field.
fn parse_frame_stats_count(field: &str) -> Option<u64> {
    field.strip_suffix('*').unwrap_or(field).parse::<u64>().ok()
}

/// Sums one bucket's `count` column over every report block in a file.
///
/// Zero when the label appears in no block, which `summarize`'s zero-count skip makes
/// mean "that region was never entered" rather than "the row was dropped" — the
/// reading `preview_pane_renders_and_edit_source_returns_under_cage`'s assertions
/// 12 through 14 all rest on. An unparseable count is a PANIC rather than a skipped
/// row, because a silently-dropped row would lower a sum that three assertions read
/// as evidence.
pub(crate) fn sum_frame_stats_counts(
    blocks: &[FrameStatsBlock],
    label: &str,
    path: &Path,
    smoke: &str,
) -> u64 {
    blocks
        .iter()
        .flat_map(|block| block.rows.iter())
        .filter(|row| row.label == label)
        .map(|row| {
            parse_frame_stats_count(&row.count_field).unwrap_or_else(|| {
                panic!(
                    "{smoke}: line {} of {} is a {label:?} row whose count field is {:?}, which is \
                     not an integer (optionally followed by the `*` truncation marker). The count \
                     is a `u64` printed into a right-aligned field, so anything else means the \
                     row's format string and its arguments disagree — a formatter defect rather \
                     than anything about the region the row names.",
                    row.line_no,
                    path.display(),
                    row.count_field
                )
            })
        })
        .sum()
}

/// Parses one `keys=` / `wakes=` counter out of a footer.
fn parse_frame_stats_counter(
    digits: &str,
    what: &str,
    line_no: usize,
    path: &Path,
    smoke: &str,
) -> u64 {
    digits.parse::<u64>().unwrap_or_else(|e| {
        panic!(
            "{smoke}: line {line_no} of {} carries a `{what}=` footer counter {digits:?} that is \
             not a non-negative integer ({e}). Both counters are `u64`s printed with `{{}}`, so a \
             non-integer means the footer's format string and its arguments have drifted apart.",
            path.display()
        )
    })
}

/// Splits a frame-stats report file into its report blocks.
///
/// The install header (`enabled interval=…`) and each report's column row are
/// skipped; everything that is neither a header, a column row nor a footer is a
/// bucket row and is attached to the block that is open. A block left without a
/// footer is a hard error rather than a dropped row, because the alternative reading
/// — silently discarding a truncated block — would let a report cut in half satisfy
/// every assertion below it.
pub(crate) fn parse_frame_stats_blocks(
    text: &str,
    path: &Path,
    smoke: &str,
) -> Vec<FrameStatsBlock> {
    let mut blocks: Vec<FrameStatsBlock> = Vec::new();

    for (offset, line) in text.lines().enumerate() {
        let line_no = offset + 1;
        let Some(body) = line.strip_prefix(FRAME_STATS_LINE_PREFIX) else {
            panic!(
                "{smoke}: line {line_no} of {} does not carry {FRAME_STATS_LINE_PREFIX:?}: \
                 {line:?}. Assertion 5 checks the same property and should have fired ahead of \
                 this, so reaching here means the file was rewritten between the two reads.",
                path.display()
            )
        };
        let mut tokens = body.split_whitespace();
        let Some(first) = tokens.next() else {
            continue;
        };

        // The once-only install header, and each report's column row.
        if first == "enabled" || first == "bucket" {
            continue;
        }

        if let Some(digits) = first.strip_prefix('#') {
            let index = digits.parse::<u64>().unwrap_or_else(|e| {
                panic!(
                    "{smoke}: line {line_no} of {} looks like a report header but its index \
                     {digits:?} is not a number ({e}). The header's shape is \
                     `#<index>  window=<seconds>s`.",
                    path.display()
                )
            });
            blocks.push(FrameStatsBlock {
                index,
                line_no,
                rows: Vec::new(),
                footer: None,
            });
            continue;
        }

        if let Some(digits) = first.strip_prefix("keys=") {
            let keys = parse_frame_stats_counter(digits, "keys", line_no, path, smoke);
            let wakes_token = tokens.next().unwrap_or_else(|| {
                panic!(
                    "{smoke}: line {line_no} of {} is a footer with no `wakes=` field: {body:?}. \
                     The footer's shape is `keys=<k> wakes=<w> wakes/key=<ratio>`.",
                    path.display()
                )
            });
            let wakes_digits = wakes_token.strip_prefix("wakes=").unwrap_or_else(|| {
                panic!(
                    "{smoke}: line {line_no} of {} has {wakes_token:?} where a `wakes=` field \
                     belongs: {body:?}",
                    path.display()
                )
            });
            let wakes = parse_frame_stats_counter(wakes_digits, "wakes", line_no, path, smoke);
            let block = blocks.last_mut().unwrap_or_else(|| {
                panic!(
                    "{smoke}: line {line_no} of {} is a report footer that no `#<index>` header \
                     precedes. Every report is written by ONE `write_all`, so a stray footer means \
                     the file is being appended to by something other than this app.",
                    path.display()
                )
            });
            assert!(
                block.footer.is_none(),
                "{smoke}: report #{} (line {}) has TWO footers, the second at line {line_no} of \
                 {}. A block ends at its footer, so this means a header line went missing and two \
                 reports have been merged into one — at which point assertion 11's per-report \
                 comparison is between one report's wake rows and another's counter.",
                block.index,
                block.line_no,
                path.display()
            );
            block.footer = Some((keys, wakes));
            continue;
        }

        // Anything else is a bucket row.
        let fields: Vec<&str> = tokens.collect();
        assert_eq!(
            fields.len(),
            5,
            "{smoke}: line {line_no} of {} is a bucket row for {first:?} carrying {} field(s) \
             after the label, want 5 (count, mean, p50, p95, max). Raw body: {body:?}. A short row \
             means `format_report`'s column list and its header row have drifted apart, which \
             breaks the alignment the two front-ends' tables are meant to be diffed on.",
            path.display(),
            fields.len()
        );
        let block = blocks.last_mut().unwrap_or_else(|| {
            panic!(
                "{smoke}: line {line_no} of {} is a bucket row that no `#<index>` header precedes: \
                 {body:?}",
                path.display()
            )
        });
        block.rows.push(FrameStatsRow {
            label: first.to_string(),
            line_no,
            count_field: fields[0].to_string(),
            millis_fields: [
                fields[1].to_string(),
                fields[2].to_string(),
                fields[3].to_string(),
                fields[4].to_string(),
            ],
        });
    }

    for block in &blocks {
        assert!(
            block.footer.is_some(),
            "{smoke}: report #{} (header at line {} of {}) has no `keys=…` footer. Every report is \
             emitted as ONE `write_all` of a fully-formatted string, so a header without a footer \
             means the write was cut short — most plausibly the app was signalled mid-write, which \
             the scenario's trailing idle exists to make impossible.",
            block.index,
            block.line_no,
            path.display()
        );
    }

    blocks
}

/// The frame-time instrument is wired into the LIVE keystroke-to-pixel path: a real
/// `wolf-gnome` under `cage`, typed into with `wtype`, writes a report file whose
/// buckets name the regions it actually entered.
///
/// ## Why it exists
///
/// `WOLF_FRAME_STATS_FILE` is the one seam of this instrument a test can read, and
/// reading it is the only honest evidence that the buckets are wired into the live
/// path rather than merely compiled. The seam's own `#[cfg(test)] mod tests` covers
/// every rule that is decidable from values — the bucket table, the percentiles, the
/// ring bound, the byte-exact report layout, the environment parsing — and can say
/// nothing at all about whether `window.rs`, `shell.rs` and `pane_view.rs` ever call
/// into it.
///
/// ## What it asserts, in order
///
/// 1. The scenario exited 0.
/// 2. The report file exists and is non-empty.
/// 3. It carries the once-only `enabled interval=` install line.
/// 4. It carries the FIRST report's header, `#1  window=`.
/// 5. Every line carries the grep prefix.
/// 6. Summed over every footer, `keys=` is at least [`FRAME_STATS_MIN_KEYS`].
/// 7. Summed over every footer, `wakes=` is greater than zero.
/// 8. Every one of [`FRAME_STATS_REQUIRED_LABELS`] appears as a bucket ROW in at
///    least one report.
/// 9. Neither of [`FRAME_STATS_ABSENT_LABELS`] appears anywhere in the file.
/// 10. Every bucket row parses: a positive integer count and four non-negative
///     millisecond fields.
/// 11. Within each report, the `wake` row's count equals that report's `wakes=`.
///
/// Assertion 8 is a ROW match on the label token rather than a `contains`, and that
/// is load-bearing: every `wake.*` label carries `wake` as a prefix, so a substring
/// test for `wake` is satisfied by `wake.tabs` alone and four of the five wake
/// buckets could be unwired with the smoke still green.
///
/// Assertion 9 is assertion 8's non-vacuity half. Without it, a `summarize` that
/// emitted all fifteen buckets regardless of count would satisfy every required
/// label while proving that not one of them was ever recorded.
///
/// ## What it deliberately does NOT assert
///
/// No wall-clock magnitude, no particular count, no value for the `wakes/key` ratio,
/// no number of reports beyond "at least one", and no bucket ordering ACROSS reports.
/// Each of those is either a claim about the machine the suite happens to run on or a
/// claim about `wtype`'s timing, and none of them is a claim about the instrument.
/// The ratio is additionally an UPPER BOUND on wakes attributable to typing rather
/// than an attribution, so there is no value it could be asserted against.
///
/// Two consequences of the no-timer cadence are visible here and are not defects: a
/// report is emitted on the first WAKE at or after the interval boundary, so with the
/// application idle between bursts each report is carried by the next burst's first
/// keystroke; and the teardown kills the app with a signal, so
/// `frame_metrics::flush_final` never runs and the final burst's keystrokes reach no
/// report. That is why [`FRAME_STATS_MIN_KEYS`] is a floor far below what is typed.
///
/// ## Red runs to capture (each a one-line source edit)
///
/// | edit | expected red |
/// | --- | --- |
/// | delete `frame_metrics::note_keystroke()` in `window::wire_keys` | assertion 6 fails with a summed `keys=0` |
/// | make `frame_metrics::enabled()` return `false` | assertions 2 and 3 fail — the file is never created |
/// | delete the `end(Bucket::DrawGrid, …)` at the `paint` call in `pane_view` | assertion 8 fails on `draw.grid` |
/// | delete the zero-count skip in `frame_stats::summarize` | assertion 9 fails on both absent labels |
#[test]
fn frame_stats_report_covers_the_live_typing_path_under_cage() {
    const SMOKE: &str = "frame_stats_report_covers_the_live_typing_path_under_cage";

    let runtime_dir = require_smoke_env();

    // Locate the freshly built binary under test.
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // No fixtures: no pixel is read. The default HOME (the cwd's parent) keeps the editor branch.
    let cwd = make_temp_cwd();

    // The app APPENDS to this path; it starts absent because the runtime directory is
    // this smoke's own and fresh.
    let stats_path = runtime_dir.join(FRAME_STATS_FILE_NAME);

    // --- Assertion 1: the scenario ran to completion --------------------------
    let status = cage_command(FRAME_STATS_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("WOLF_FRAME_STATS", "1")
        .env("WOLF_FRAME_STATS_INTERVAL", FRAME_STATS_INTERVAL_SECS)
        .env("WOLF_FRAME_STATS_FILE", &stats_path)
        .env("FRAME_STATS_BURST", FRAME_STATS_BURST_TEXT)
        .status()
        .expect("failed to spawn `cage` for the headless frame-stats scenario");
    assert!(
        status.success(),
        "{SMOKE}: the cage frame-stats scenario exited unsuccessfully ({status}). Exit 11: the app \
         died before it was ready for input, so nothing was ever typed. Exit 12: it died between \
         typing bursts. Exit 13: it died during the final idle. All three are distinct from a \
         clean shutdown on purpose — a process that panicked in the GTK main loop and one that \
         exited cleanly are equally gone, and both produce the same empty report file that a \
         disabled instrument produces. Report file expected at {}.",
        stats_path.display()
    );

    // --- Assertion 2: the file exists and is non-empty ------------------------
    let meta = std::fs::metadata(&stats_path).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: expected the frame-stats report file {} to exist after the scenario, but: \
             {e}. `frame_metrics::install()` opens it create/append as the FIRST statement of \
             `app::run`, so an absent file means either the flag was not read (check \
             `frame_stats::is_enabled` and that `install()` is called before anything else) or the \
             open failed and degraded to stderr-only — which is deliberate behaviour for an \
             unopenable path, and this path is under XDG_RUNTIME_DIR precisely so it is openable.",
            stats_path.display()
        )
    });
    assert!(
        meta.len() > 0,
        "{SMOKE}: the frame-stats report file {} exists but is EMPTY. The install header alone \
         should have been written the moment the flag was read, so an empty file means \
         `install()` opened the handle and never wrote through it — or wrote through a different \
         handle. It is NOT evidence that no report interval elapsed: the header does not wait for \
         one.",
        stats_path.display()
    );

    let text = std::fs::read_to_string(&stats_path).unwrap_or_else(|e| {
        panic!(
            "{SMOKE}: could not read the frame-stats report file {}: {e}. It was non-empty a \
             moment ago, so this is about the file's bytes rather than its existence — the writer \
             emits UTF-8 through `write_all`, so invalid UTF-8 here means something else is \
             appending to the path.",
            stats_path.display()
        )
    });

    // --- Assertion 3: the once-only install header ----------------------------
    assert!(
        text.lines()
            .any(|line| line.starts_with(FRAME_STATS_ENABLED_NEEDLE)),
        "{SMOKE}: no line of {} begins with {FRAME_STATS_ENABLED_NEEDLE:?}. That line is written \
         by `frame_metrics::install()` as soon as it finds `WOLF_FRAME_STATS` set, before any \
         interval can elapse, so its absence says the flag was never read as SET — check \
         `frame_stats::is_enabled` against the literal \"1\" this smoke passes, and check that \
         `install()` really is the first statement of `app::run`. File contents were:\n{text}",
        stats_path.display()
    );

    // --- Assertion 4: at least one report was emitted -------------------------
    assert!(
        text.lines()
            .any(|line| line.starts_with(FRAME_STATS_FIRST_REPORT_NEEDLE)),
        "{SMOKE}: no line of {} begins with {FRAME_STATS_FIRST_REPORT_NEEDLE:?}, so the install \
         header is there and not one report followed it. The instrument arms no timer: a report is \
         emitted from the LAST statement of the drain-loop body, so this says either that \
         `maybe_emit()` is not being called from that loop, or that `interval_elapsed` never \
         answered true, or that `is_empty()` answered true on every wake — which would mean \
         `record`, `note_wake` and `note_keystroke` are all reaching a different accumulator from \
         the one the emit reads (the accumulator is a `thread_local!`). Note the TWO spaces \
         before `window=`: they are part of the pinned layout. File contents were:\n{text}",
        stats_path.display()
    );

    // --- Assertion 5: every line is greppable ---------------------------------
    for (offset, line) in text.lines().enumerate() {
        assert!(
            line.starts_with(FRAME_STATS_LINE_PREFIX),
            "{SMOKE}: line {} of {} does not begin with {FRAME_STATS_LINE_PREFIX:?}: {line:?}. \
             Every line of every report carries the prefix so a report survives being interleaved \
             with anything else on the same stream; an unprefixed footer or column row is exactly \
             the line a `grep wolf-frame-stats` drops without saying so.",
            offset + 1,
            stats_path.display()
        );
    }

    let blocks = parse_frame_stats_blocks(&text, &stats_path, SMOKE);

    // Non-vacuity for assertions 6 to 11, which loop over the blocks: the parser found a report.
    assert!(
        !blocks.is_empty(),
        "{SMOKE}: assertion 4 found a `#1` header in {} but the parser assembled NO report blocks \
         from it, so every assertion below would pass over an empty list. The parser splits on the \
         header line, so this means the header's shape changed. File contents were:\n{text}",
        stats_path.display()
    );

    // --- Assertion 6: the typing reached Neovim, and was counted --------------
    let total_keys: u64 = blocks.iter().map(FrameStatsBlock::keys).sum();
    assert!(
        total_keys >= FRAME_STATS_MIN_KEYS,
        "{SMOKE}: the `keys=` counters over all {} report(s) in {} sum to {total_keys}, want at \
         least {FRAME_STATS_MIN_KEYS}. The scenario typed four bursts of \
         {FRAME_STATS_BURST_TEXT:?} plus an `i` and an `Escape` each — eighty-four keystrokes — and \
         expects the LAST burst's to reach no report (the teardown signals the app, so the exit \
         flush never runs). A ZERO here is the signature of `frame_metrics::note_keystroke()` \
         missing from `window::wire_keys`, since every other counter in the file would still be \
         populated. A small non-zero number instead points at the keys not arriving: check the \
         scenario's exit status above first, since a dead app types into nothing. File contents \
         were:\n{text}",
        blocks.len(),
        stats_path.display()
    );

    // --- Assertion 7: the drain loop ran --------------------------------------
    let total_wakes: u64 = blocks.iter().map(FrameStatsBlock::wakes).sum();
    assert!(
        total_wakes > 0,
        "{SMOKE}: the `wakes=` counters over all {} report(s) in {} sum to zero, so nothing ever \
         called `frame_metrics::note_wake()`. Reports exist, which means the emit at the end of the \
         drain-loop body IS running — so the counter call is missing from the top of that same \
         body, or is placed after an early `continue`. File contents were:\n{text}",
        blocks.len(),
        stats_path.display()
    );

    // Assertion 8: a row match on the label token, since a substring `wake` matches `wake.tabs` alone.
    for label in FRAME_STATS_REQUIRED_LABELS {
        let found = blocks
            .iter()
            .any(|block| block.rows.iter().any(|row| row.label == label));
        assert!(
            found,
            "{SMOKE}: no report in {} carries a bucket row labelled {label:?}, so that region was \
             never entered and timed during a real typing session — `summarize` omits every \
             zero-count bucket, which is what makes an absent row mean exactly that. Check the \
             `frame_metrics::scope` call site for this bucket, and remember that a `Scope` bound \
             with `let _ = frame_metrics::scope(..)` rather than to a NAMED guard drops instantly \
             and records a region of zero length, which still produces a row — so an absent row is \
             the call site missing outright. Labels actually seen: {:?}. \
             File contents were:\n{text}",
            stats_path.display(),
            blocks
                .iter()
                .flat_map(|block| block.rows.iter().map(|row| row.label.as_str()))
                .collect::<std::collections::BTreeSet<_>>()
        );
    }

    // --- Assertion 9: the non-vacuity half of assertion 8 ---------------------
    for label in FRAME_STATS_ABSENT_LABELS {
        assert!(
            !text.contains(label),
            "{SMOKE}: {} carries {label:?} somewhere, but this scenario never opens a Markdown \
             preview, so that bucket cannot have been recorded even once. Either `summarize` stopped \
             omitting zero-count buckets — in which case assertion 8 above is satisfied by a \
             formatter that prints all fifteen labels whatever was recorded, and proves nothing — \
             or something is recording into a preview bucket from a non-preview path. This is the \
             one assertion in this smoke whose failure invalidates another one's pass. File \
             contents were:\n{text}",
            stats_path.display()
        );
    }

    // --- Assertion 10: every bucket row is a table row, not prose -------------
    for block in &blocks {
        for row in &block.rows {
            let count = parse_frame_stats_count(&row.count_field).unwrap_or_else(|| {
                panic!(
                    "{SMOKE}: report #{}, line {} of {}: the count field of the {:?} row is \
                     {:?}, which is not an integer (optionally followed by the `*` truncation \
                     marker). The count is a `u64` printed into a width-5 right-aligned field, so \
                     anything else means the row's format string and its arguments disagree.",
                    block.index,
                    row.line_no,
                    stats_path.display(),
                    row.label,
                    row.count_field
                )
            });
            assert!(
                count > 0,
                "{SMOKE}: report #{}, line {} of {}: the {:?} row has a count of zero. \
                 `summarize` omits every zero-count bucket, so a zero-count row on disk means that \
                 skip is gone — the same defect assertion 9 catches from the other side, and the \
                 one that would let assertion 8 pass over a report in which nothing was recorded.",
                block.index,
                row.line_no,
                stats_path.display(),
                row.label
            );
            for (column, field) in ["mean", "p50", "p95", "max"]
                .iter()
                .zip(row.millis_fields.iter())
            {
                let value = field.parse::<f64>().unwrap_or_else(|e| {
                    panic!(
                        "{SMOKE}: report #{}, line {} of {}: the {column} field of the {:?} row is \
                         {field:?}, which does not parse as a number ({e}). Every one of the four \
                         is nanoseconds divided by a million and printed with two decimals, so a \
                         `NaN` or an `inf` here means the division saw something other than a \
                         nanosecond count.",
                        block.index,
                        row.line_no,
                        stats_path.display(),
                        row.label
                    )
                });
                assert!(
                    value >= 0.0,
                    "{SMOKE}: report #{}, line {} of {}: the {column} field of the {:?} row is \
                     {value}, which is neither zero nor positive. Durations are unsigned, so a \
                     NEGATIVE value means a subtraction wrapped or an `as` cast was used where \
                     `u64::try_from(..).unwrap_or(u64::MAX)` belongs; a `NaN` — which parses \
                     happily and then fails this comparison — means the millisecond division saw \
                     something that was not a nanosecond count at all.",
                    block.index,
                    row.line_no,
                    stats_path.display(),
                    row.label
                );
            }
        }
    }

    // Assertion 11: true by construction unless one side is conditional. A report with no wakes has no
    // `wake` row, so `unwrap_or(0)` treats "no row" and "wakes=0" as the same state.
    for block in &blocks {
        let wake_count = block
            .rows
            .iter()
            .find(|row| row.label == FRAME_STATS_WAKE_LABEL)
            .and_then(|row| parse_frame_stats_count(&row.count_field))
            .unwrap_or(0);
        assert_eq!(
            wake_count,
            block.wakes(),
            "{SMOKE}: report #{} (header at line {} of {}) counts {wake_count} `wake` sample(s) \
             but reports `wakes={}`. The two are incremented by the same pass over the drain-loop \
             body — `note_wake()` first, then the block the `wake` scope guards — so a mismatch \
             means one of them sits behind a condition the other does not: an early `continue` \
             between them, a `note_wake()` moved inside the liveness gate, or the `wake` scope \
             wrapping only part of the body. A wake count LARGER than the counter also fits a \
             second `wake` region opened somewhere else. File contents were:\n{text}",
            block.index,
            block.line_no,
            stats_path.display(),
            block.wakes()
        );
    }
}
