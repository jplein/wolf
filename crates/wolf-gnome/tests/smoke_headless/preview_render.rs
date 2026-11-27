//! Preview rendering smokes, read from raw pixels: fenced-code highlighting, the code block's
//! background in each appearance, the inline-code baseline grid and the file picker drawn over a
//! live preview.

use super::config::{APPEARANCE_FORCED_DARK, APPEARANCE_FORCED_LIGHT};
use super::preview::{
    preview_theme_band_rows, preview_theme_editor_columns, preview_theme_pane_area,
    preview_theme_preview_columns, preview_theme_scan_rows, PREVIEW_DARK_BG, PREVIEW_LIGHT_BG,
    PREVIEW_OTHER_PANE_QUERY, PREVIEW_QUERY, PREVIEW_READBACK_PREFIX, PREVIEW_THEME_SCENARIO,
};
use super::*;

/// Whether every channel of `a` is within `tolerance` steps of the same channel of
/// `b`, in either direction.
///
/// For a colour this binary NAMES but the renderer blends, where GSK's renderers round
/// the blend differently — see [`PICKOVER_ROW_HIGHLIGHT_TOLERANCE`]. A colour read
/// back out of the same capture, or a flat CSS colour no blend reaches, is compared
/// exactly instead; `ADWAITA_ACCENT_RGB` says why for one of them.
fn rgb_within(a: Rgb8, b: Rgb8, tolerance: u8) -> bool {
    a.0.abs_diff(b.0) <= tolerance
        && a.1.abs_diff(b.1) <= tolerance
        && a.2.abs_diff(b.2) <= tolerance
}

// Highlighted code blocks and the inline-code baseline grid: two separate pixel smokes, since
// a shared exit code would name the wrong feat row and their fixtures are incompatible.

/// The markdown file the code-highlight smoke opens and then previews.
///
/// Nothing in this smoke is OCR'd — its observables are chromatic pixels inside a
/// rendered `pre` block and a `writefile` readback the captive Neovim produces — so
/// this name carries none of the [`SIDEBAR_FILLER_FILES`] hazard and contributes no
/// row to `RELAUNCH_FIXTURE_NAMES`. That mirrors `PREVIEW_THEME_SOURCE_FILE`
/// exactly, which is absent from that table for the same stated reason: a sidebar row
/// is TEXT, and text can satisfy no assertion whose subject is a colour.
const CODEHL_SOURCE_FILE: &str = "cbhl.md";

/// The leading paragraph of [`codehl_source_body`]: NATO words, no markdown syntax.
///
/// ## Why a leading paragraph is MANDATORY
///
/// `web_top` is located as the first row that is entirely [`PREVIEW_DARK_BG`] across
/// the sampled columns. If the document's FIRST block were the `pre`, the first such
/// row would be the 24 px gap BETWEEN the two blocks, and every rectangle derived
/// from `web_top` would be one block too low. A paragraph guarantees a run of pure
/// body background at the top of the document — the same mechanism
/// `preview_document_follows_the_configured_appearance_under_cage` measured with the
/// content's top at absolute row 159. The page's root padding adds a run of its own above
/// the paragraph, so `web_top` is the view's top edge, at 139 by derivation.
///
/// ## Why it carries no markdown syntax
///
/// A heading or a link would put a second colour into the document, and the whole
/// discriminator here is that the only CHROMATIC ink on the page comes from the
/// highlighter. The theme's five dark palette entries are all neutral (r = g = b), so
/// plain prose contributes no chroma at any threshold.
///
/// MEASURED (GTK 4.22.4, libadwaita 1.9.0, WebKitGTK 2.52.4, `cage` 0.3.0, 1280x720):
/// this paragraph WRAPS to two lines, so the first `pre` block starts 72 rows below the
/// content's top rather than 48, and 92 below `web_top`, the root padding above it. Nothing asserts that offset — the blocks are located
/// from the pixels — but a longer paragraph pushes both blocks down, and the scan
/// only reaches [`preview_theme_band_rows`]'s last row (600 at the pinned output).
///
/// ## The 57-row margin, and what it rules out
///
/// The measured second block ends at absolute row 543, so 57 rows remain before that
/// bound binds: two more wrapped lines here still fit (543 + 48 = 591 < 600) and a
/// third does not.
///
/// The same margin is why [`CODEHL_FENCE_BODY`] carries FOUR code lines rather than
/// five. A five-line fence makes each block 168 rows instead of 144, so the second one
/// would end at 591 and leave 9 rows — and one extra wrapped line HERE would then clip
/// it. A clipped control block is not a missing block: it is still exactly one run, so
/// the run count cannot see it, and the smaller rectangle can only LOWER the control's
/// chroma count, which is the silent under-count the equal-height guard in
/// [`preview_highlights_a_fenced_code_block_under_cage`] exists to catch. MEASURED on a
/// six-line candidate: two runs, the second truncated to 153 rows against the first's
/// 192, with 20 % of the control never sampled. Growing this paragraph or that fence
/// spends the margin, so re-measure both if either changes.
const CODEHL_PARAGRAPH: &str =
    "alfa bravo charlie delta echo foxtrot golf hotel india juliett kilo lima.";

/// The body of BOTH fenced blocks in [`codehl_source_body`] — four lines of Go.
///
/// ONE const interpolated twice, so the two blocks carry byte-identical text BY
/// CONSTRUCTION rather than by a coincidence a later edit could break on one side.
/// That identity is what makes block 2 an IN-CAPTURE CONTROL: the two blocks differ
/// in their fence's language and in nothing else, so antialiasing, glyph shapes and
/// every other rendering variable except the highlighter's answer are held constant.
/// Do NOT "simplify" the two fences into two literals — the identity would then be a
/// coincidence one edit could break on one side, which is what this const prevents.
///
/// ## Why THIS Go, and not any four lines of it
///
/// MEASURED against the pinned Neovim with the vendored trees: under this colorscheme
/// Go has only THREE chromatic token colours — chroma 108 (function names and calls,
/// and the builtin types, so `string`, `int`, `len`, `Sprintf`), chroma 89 (struct
/// field members) and chroma 67 (string literals). Keywords, numbers, operators,
/// punctuation, user type names and package identifiers are all `#e0e2ea`, chroma 10,
/// and comments are chroma 9. A fence led by comments or by a `const` block therefore
/// buys almost no chromatic ink: two such candidates of this height measured 528 and
/// 605 chromatic pixels against this one's 1209. Density of CALLS, TYPES and STRING
/// LITERALS is what [`CODEHL_MIN_CHROMA_PX`]'s headroom is bought with; more lines of
/// the wrong tokens buy nothing.
///
/// The height is four code lines because of the scan margin [`CODEHL_PARAGRAPH`]
/// records, not because four is special.
///
/// Line 3 begins with a literal TAB, as `gofmt` writes it. It is spelt `\t` rather than
/// typed, so no editor or formatter can silently turn it into spaces.
///
/// **Do NOT reason about the two blocks' INK masses.** They are not equal — MEASURED
/// 2697 and 2622 non-background pixels despite byte-identical text — because chromatic
/// ink produces more pixels that differ from the background than neutral ink does.
/// The CHROMA comparison below is the sound one; an ink comparison would not be.
const CODEHL_FENCE_BODY: &str = "import \"fmt\"\n\
                                 func Greet(a string, b string) string {\n\
                                 \treturn fmt.Sprintf(\"hi %s and %s\", a, b)\n\
                                 }";

/// The language of the HIGHLIGHTED fence, and the language whose injected child
/// langtree the readback asks the captive Neovim about.
///
/// ## This choice is the single most likely way this smoke gets silently disarmed
///
/// The pinned Neovim ships seven parsers with `highlights` queries — `c`, `lua`,
/// `markdown`, `markdown_inline`, `query`, `vim`, `vimdoc` ([`NVIM_BUILTIN_PARSERS`]).
/// A fence in ANY of those seven would highlight with nothing vendored at all, so the
/// named falsifier (deleting `init.lua`'s environment fallback) would leave this smoke
/// GREEN — a check aimed at the right property, looking somewhere the defect cannot
/// appear. `go` is not one of the seven and IS one of the sixteen `flake.nix` vendors,
/// which is exactly the combination the discriminator needs.
///
/// The smoke asserts the "not a builtin" half mechanically before it launches
/// anything, because the rule and the assertion it can disarm are hundreds of lines
/// apart.
const CODEHL_HL_LANG: &str = "go";

/// The language of the CONTROL fence: a coined string no grammar exists for, on
/// either platform.
///
/// `render_source_html` degrades per block to the plain render when no parser answers,
/// so this block still renders as a `pre` — MEASURED, and load-bearing: an unknown
/// language that produced NO block at all would leave one run where the smoke expects
/// two, and the control would be missing rather than plain.
///
/// It must not be a real language under any spelling. Being absent from the seven
/// builtins is checked mechanically; being absent from `flake.nix`'s sixteen is not,
/// because re-typing that list here would be a second copy to keep in step. The `zzz`
/// prefix is this binary's convention for a coined term precisely so the answer is
/// visible without a lookup.
const CODEHL_PLAIN_LANG: &str = "zzznotalang";

/// The languages the pinned Neovim ships a parser AND a `highlights` query for, with
/// nothing vendored.
///
/// MEASURED against the pinned Neovim with `--clean` (a probe without `--clean` reads
/// the developer's own `~/.local/share/nvim/site/parser/*.so` and reports false
/// positives). Re-typed here rather than imported, because there is nothing to import
/// it from and because the point of the list is to constrain a FIXTURE.
const NVIM_BUILTIN_PARSERS: &[&str] = &[
    "c",
    "lua",
    "markdown",
    "markdown_inline",
    "query",
    "vim",
    "vimdoc",
];

/// The whole body of [`CODEHL_SOURCE_FILE`], in this exact order: the leading
/// paragraph, the [`CODEHL_HL_LANG`] fence, then the [`CODEHL_PLAIN_LANG`] fence.
///
/// The ORDER is fixed and asserted on: run 1 of the located blocks is the highlighted
/// one and run 2 is the control, so swapping the two fences would invert the
/// discriminator and its control while still compiling.
fn codehl_source_body() -> String {
    format!(
        "{CODEHL_PARAGRAPH}\n\n\
         ```{CODEHL_HL_LANG}\n{CODEHL_FENCE_BODY}\n```\n\n\
         ```{CODEHL_PLAIN_LANG}\n{CODEHL_FENCE_BODY}\n```"
    )
}

/// Name suffix of the code-highlight smoke's config directory sibling.
const CODEHL_SUFFIX: &str = "-codehl";

/// The code-highlight smoke's capture.
///
/// Under `XDG_RUNTIME_DIR` rather than in the smoke's cwd, so the app's own
/// `FileSearch` never lists it and it never becomes a tree-sidebar row — which would
/// change the page the editor pane draws for no reason.
const CODEHL_PNG_NAME: &str = "wolf_gnome_code_highlight.png";

/// File the captive Neovim writes its window/buffer list into. See
/// [`CODEHL_PNG_NAME`] for why it lives outside the cwd.
const CODEHL_WINS_NAME: &str = "wolf_gnome_code_highlight_wins.txt";

/// File the captive Neovim writes the INJECTED-LANGTREE answer into.
const CODEHL_LANGTREE_NAME: &str = "wolf_gnome_code_highlight_langtree.txt";

/// The marker the langtree readback's Lua writes when the source buffer's markdown
/// langtree has an injected [`CODEHL_HL_LANG`] child.
///
/// Re-typed as a literal rather than imported from
/// `crates/wolf-ffi/tests/treesitter_highlight.rs`'s `MARK_OK`: each `tests/*.rs` is
/// its own crate, and the two files state the same contract independently on purpose.
const CODEHL_LANGTREE_OK: &str = "OK";

/// The `background-color` `wolf_core`'s `markdown/themes/default.css` gives
/// `body.wolf-markdown pre` in DARK, as `(r, g, b)`.
///
/// RE-TYPED here as a literal, never imported, for the reason [`PREVIEW_DARK_BG`]'s
/// doc comment gives at length: reading the value out of the crate that produced the
/// pixels would make the assertion a comparison with itself.
///
/// It is what LOCATES the two code blocks: a `pre` paints this colour across the
/// document's whole content width, while the 24 px gap between two blocks is 100 %
/// [`PREVIEW_DARK_BG`] and 0 % this, so the gap breaks a run cleanly.
///
/// The colour is the bundled Lighthouse colorscheme's dark `slightHighlightBG`, copied into the
/// theme's fixed palette. It is re-typed, and never read through `Lighthouse::from_env`, because
/// a Lighthouse bump does not change the preview's palette: reading it from the pin would move
/// this expectation with a colour the preview does not follow.
pub(crate) const CODEHL_PRE_BG: Rgb8 = (0x25, 0x25, 0x25);

/// The least CHROMA a pixel must carry to count as highlighter ink.
///
/// `chroma = max(r, g, b) - min(r, g, b)`, so a neutral pixel is 0 whatever its
/// brightness. Every colour in an UNHIGHLIGHTED block is neutral — `#e6e6e6` ink on
/// `#252525`, both r = g = b — and blending two neutral colours channel-independently
/// yields another neutral colour, so an unhighlighted block cannot produce chroma at
/// ALL: the control block MEASURES 0 at every threshold down to `>= 1`.
///
/// ## What this threshold separates is NOT antialiasing
///
/// A HIGHLIGHTED block is not chromatic throughout, so reading this constant as a guard
/// against antialiasing gets it wrong. MEASURED: inside the highlighted block the
/// keywords, numbers, operators, punctuation and plain identifiers are `#e0e2ea` —
/// chroma 10 — and only function names, builtin types, struct fields and string
/// literals carry real colour, at chroma 108, 89 and 67. What this constant separates
/// is therefore real token colour from the near-neutral BULK of the same block, and the
/// histogram cliff between them is sharp: chroma bucket 10 holds 216 pixels and bucket
/// 11 holds 7, a 31x drop. The legal window is [11, 67] — above the near-neutral spike,
/// at or below the weakest real token colour. 32 sits 3.2x above the spike and 2.1x
/// below that weakest colour, near the window's log-midpoint.
///
/// MEASURED chromatic-pixel counts inside the highlighted block at rising thresholds:
/// `>= 1` 2630, `>= 8` 1961, `>= 12` 1544, `>= 32` 1209, `>= 64` 658, maximum chroma
/// 108. Any threshold in [12, 66] leaves a count between 658 and 1544, so no floor in
/// the low hundreds can be flipped by the choice.
///
/// Only ONE of the two ways to get this wrong is self-announcing on this host. Too HIGH
/// — past 67 — and the count collapses towards zero, which fires on the highlighted
/// block in the GREEN run. Too LOW does NOT fire here, because this host's antialiasing
/// is grayscale and its control reads 0 at every threshold; it becomes self-announcing
/// only on a host whose subpixel antialiasing puts colour on glyph edges, where the
/// control's count lifts through [`CODEHL_MAX_CHROMA_PX`]. On this host the [11, 67]
/// window is what protects the choice, not the control.
const CODEHL_CHROMA_MIN: u16 = 32;

/// The fewest chromatic pixels the HIGHLIGHTED block must carry.
///
/// MEASURED 1209 at [`CODEHL_CHROMA_MIN`] over the block's 354 x 144 = 50,976 pixels,
/// so this floor leaves 4.0x of headroom. An ACCEPTANCE CRITERION and not a tuned
/// number: it is 12x above the smallest floor
/// [`codehl_chroma_thresholds_are_separated`] admits, and it stays 2.2x clear at the
/// most pessimistic threshold [`CODEHL_CHROMA_MIN`]'s legal window allows (658 at
/// `>= 64`). A block that lost its highlighting reads 0 rather than a smaller positive
/// number — the control block measures exactly that, in the same capture.
///
/// Do NOT lower it to make a run pass. [`codehl_chroma_thresholds_are_separated`]
/// exists because a floor set below the CONTROL block's own ceiling would let a
/// de-highlighted block clear it, which is the one way this pair of assertions can be
/// disarmed without either of them going red.
const CODEHL_MIN_CHROMA_PX: usize = 300;

/// The most chromatic pixels the CONTROL block may carry.
///
/// MEASURED **0** — at every threshold down to `>= 1`, over all 50,976 pixels of the
/// block. This host's antialiasing is grayscale and contributes no chroma at all, so
/// the separation between the two blocks is unbounded rather than merely sufficient,
/// and the "not at least 5x apart, therefore not shippable" clause the design
/// reserved never fired.
///
/// A small non-zero ceiling rather than an exact 0 because subpixel antialiasing on
/// another host would put a handful of chromatic pixels on any glyph edge, and a
/// smoke that failed on a font-rendering setting would be reporting the wrong thing.
/// Five is far below [`CODEHL_MIN_CHROMA_PX`] and far below what a single coloured
/// token draws.
const CODEHL_MAX_CHROMA_PX: usize = 5;

/// The fewest rows a majority-[`CODEHL_PRE_BG`] run must span to count as a code
/// block.
///
/// TWO baseline units. A `pre` holding [`CODEHL_FENCE_BODY`]'s four lines is
/// 24 (padding) + 4 x 24 (lines) + 24 (padding) = 144 rows, and the gap between the two
/// blocks is 24 rows of pure background. MEASURED: both blocks 144 rows tall (absolute
/// 231..375 and 399..543), separated by a 24-row gap that is 0 % [`CODEHL_PRE_BG`].
///
/// The floor is a third of a real block and many times the one- or two-row artifact at
/// a block's own antialiased edge that it exists to discard, so it discriminates
/// without sitting near either. It is a floor on a run's height and NOT a check that a
/// run is the right height — a run truncated by the scan limit clears it comfortably,
/// which is why the smoke also compares the two runs' heights to each other.
const CODEHL_MIN_BLOCK_ROWS: usize = 48;

/// A pixel's CHROMA: `max(r, g, b) - min(r, g, b)`.
///
/// Computed in `u16` and widened BEFORE the subtraction, mirroring `is_link_blue`'s
/// stated reason: a `u8` subtraction can wrap and silently classify a pixel as the
/// opposite of what it is. Here the wrap would turn a neutral pixel into a maximally
/// chromatic one, which would make the control assertion fail on a good render and —
/// worse — could make the discriminator pass on a de-highlighted one.
fn chroma(pixel: Rgb8) -> u16 {
    let (r, g, b) = pixel;
    let (r, g, b) = (u16::from(r), u16::from(g), u16::from(b));
    r.max(g).max(b) - r.min(g).min(b)
}

/// What one pass over a rectangle found: how many pixels cleared
/// [`CODEHL_CHROMA_MIN`], and the largest chroma seen at all.
///
/// The maximum is carried purely so a failure message can tell two very different
/// states apart: "there is ink, and it is neutral" (the highlighter did not run) from
/// "there is ink, and it is faintly coloured but below the threshold" (the threshold
/// or the palette moved). Without it both read as a bare count and the reader cannot
/// choose between them.
#[derive(Clone, Copy, Debug)]
struct ChromaSample {
    /// Pixels whose chroma is at least [`CODEHL_CHROMA_MIN`].
    count: usize,
    /// The largest chroma anywhere in the rectangle.
    max: u16,
}

/// Counts the chromatic pixels inside `rect`, and the largest chroma in it.
///
/// Takes a [`SampleRect`] rather than four `usize`s for the reason that struct's own
/// doc comment gives.
fn chroma_sample(capture: &Capture, rect: SampleRect) -> ChromaSample {
    let mut sample = ChromaSample { count: 0, max: 0 };
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let c = chroma(capture.pixel(x, y));
            if c >= CODEHL_CHROMA_MIN {
                sample.count += 1;
            }
            sample.max = sample.max.max(c);
        }
    }
    sample
}

/// A maximal run of rows that pass one colour test, [`majority_colour_runs`]'s or
/// [`exact_colour_runs`]'s, half-open: `y0 <= y < y1`.
#[derive(Clone, Copy, Debug)]
struct BlockRun {
    /// First row of the run.
    y0: usize,
    /// One past the last row of the run.
    y1: usize,
}

impl BlockRun {
    /// How many rows the run spans.
    fn rows(self) -> usize {
        self.y1 - self.y0
    }

    /// The run crossed with `columns`, as a rectangle the samplers take.
    fn rect(self, columns: (usize, usize)) -> SampleRect {
        SampleRect {
            x0: columns.0,
            x1: columns.1,
            y0: self.y0,
            y1: self.y1,
        }
    }
}

/// Whether pixels exactly `colour` are a STRICT MAJORITY of row `y`'s sampled columns.
///
/// A strict majority rather than "any" or "all". "Any" would call a single stray pixel
/// a block; "all" would be defeated by the code glyphs themselves, which are ink
/// ON the block's background and so are not the background colour.
fn row_is_majority(capture: &Capture, rect: SampleRect, y: usize, colour: Rgb8) -> bool {
    let width = rect.x1 - rect.x0;
    let hits = (rect.x0..rect.x1)
        .filter(|&x| capture.pixel(x, y) == colour)
        .count();
    hits * 2 > width
}

/// Every maximal run of majority-`colour` rows inside `rect`, in top-to-bottom order,
/// with runs shorter than `min_rows` discarded.
///
/// This is how the two code blocks are LOCATED, with no magic constant: the blocks are
/// wherever the renderer put them, and the 24 px inter-block gap — 100 %
/// [`PREVIEW_DARK_BG`], 0 % [`CODEHL_PRE_BG`] — is what separates them. The
/// `min_rows` filter is what stops a single antialiased row at a block's own edge from
/// being reported as a third block.
fn majority_colour_runs(
    capture: &Capture,
    rect: SampleRect,
    colour: Rgb8,
    min_rows: usize,
) -> Vec<BlockRun> {
    let mut runs: Vec<BlockRun> = Vec::new();
    let mut open: Option<usize> = None;
    for y in rect.y0..rect.y1 {
        if row_is_majority(capture, rect, y, colour) {
            if open.is_none() {
                open = Some(y);
            }
        } else if let Some(y0) = open.take() {
            runs.push(BlockRun { y0, y1: y });
        }
    }
    if let Some(y0) = open.take() {
        runs.push(BlockRun { y0, y1: rect.y1 });
    }
    runs.retain(|run| run.rows() >= min_rows);
    runs
}

/// Every maximal run of rows inside `rect` carrying at least `min_px` pixels exactly
/// `colour`, in top-to-bottom order, with runs shorter than `min_rows` discarded.
///
/// A per-row COUNT rather than [`majority_colour_runs`]'s majority: an inline code chip is
/// a small fraction of a band's width, so it is never a majority of any row, while its flat
/// background still puts tens of exact pixels on every row it covers.
fn exact_colour_runs(
    capture: &Capture,
    rect: SampleRect,
    colour: Rgb8,
    min_px: usize,
    min_rows: usize,
) -> Vec<BlockRun> {
    let row_hits = |y: usize| {
        (rect.x0..rect.x1)
            .filter(|&x| capture.pixel(x, y) == colour)
            .count()
    };
    let mut runs: Vec<BlockRun> = Vec::new();
    let mut open: Option<usize> = None;
    for y in rect.y0..rect.y1 {
        if row_hits(y) >= min_px {
            if open.is_none() {
                open = Some(y);
            }
        } else if let Some(y0) = open.take() {
            runs.push(BlockRun { y0, y1: y });
        }
    }
    if let Some(y0) = open.take() {
        runs.push(BlockRun { y0, y1: rect.y1 });
    }
    runs.retain(|run| run.rows() >= min_rows);
    runs
}

/// The FIRST row inside `rect` in which pixels exactly `colour` are a strict majority
/// of the sampled columns, or `None`.
///
/// Distinct from [`Capture::first_uniform_row`], which requires EVERY column to match:
/// that is right for locating a background edge, and wrong for locating a hairline
/// whose own antialiasing may leave a column or two off-colour.
fn first_majority_row(capture: &Capture, rect: SampleRect, colour: Rgb8) -> Option<usize> {
    (rect.y0..rect.y1).find(|&y| row_is_majority(capture, rect, y, colour))
}

/// The CODE-HIGHLIGHT scenario, run as a single `sh -c` string inside ONE `cage`:
/// `PREVIEW_THEME_SCENARIO`'s single-launch body with two readbacks instead of one.
///
/// ONE launch, at a forced `dark` appearance. Unlike the preview-appearance smoke, the
/// appearance is not this smoke's SUBJECT — it is a precondition for the two theme
/// colours the samplers compare against — so "the pair is what discriminates" does not
/// apply here and a second launch would buy nothing but runtime.
///
/// ## Exit codes, and why there are only two
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — a readback the captive Neovim was told to write never appeared, so the
///   assertion that reads it cannot be evaluated at all.
///
/// `12` ("still alive after `:qa!`") and `13` ("exited non-zero") are DELIBERATELY
/// absent, and their absence is not an oversight. Nothing here depends on a clean
/// exit: there is no second launch to isolate from the first, and no persisted state
/// any assertion reads back, so the scenario ends with `PREVIEW_SCENARIO`'s
/// best-effort `kill` + `wait` rather than a `:qa!` handshake. Adding those codes would
/// mean asserting on a shutdown this smoke makes no claim about.
///
/// ## Both readbacks are taken AFTER the capture, and that is load-bearing
///
/// Each is a typed Ex command, drawn by the window's command-line strip. Taken before
/// `grim`, the typed text would be in the PNG — and the second one is a 400-character
/// `:lua` line, which would redraw a large part of the window.
const CODEHL_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event;
# PREVIEW_SCENARIO's doc comment carries the measurement behind it (~255 ms for a
# cold open, about 110 ms of that spent spawning the WebProcess before Started
# fires). This spend is a cold open. Do not trim it.
PREVIEW_SETTLE=2.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome code-highlight smoke"

wait_file() {
    f=$1
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if [ -s "$f" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome code-highlight smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: open the SOURCE in the window's ONE pane ------------------------
#
# One pane is what makes the geometry deterministic: the other-pane Preview below
# splits it, a split of ONE pane can only produce two, and the new pane is the RIGHT
# half -- so the derived rectangles mean what they say.
wtype -- ":e $SRC_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: drive PREVIEW IN ANOTHER PANE through the command palette --------
#
# Ctrl+Shift+P, the query, Enter. The OTHER-PANE command and not plain Preview:
# Preview binds in place, so from this window's one pane it would leave no second
# pane and the derived rectangles below would name nothing. Neither has an
# accelerator row of its own, so the palette and the :Wolf* Ex commands are their
# only two routes.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_OTHER_PANE_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$PREVIEW_SETTLE"

# --- Step 3: the capture, with the preview pane live -------------------------
grim "$OUT_PNG"

# --- Step 4: NEOVIM's own answer about what its windows hold -----------------
#
# Joined with a COMMA rather than a space because a preview scratch's own name
# contains one.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$WINS')"
wtype -k Return
wait_file "$WINS" || exit 14

# --- Step 5: NEOVIM's own answer about the SOURCE buffer's LANGTREE ----------
#
# The SOURCE buffer is found through b:wolf_preview_source_bufnr, which
# wolf_ensure_preview_buf sets on the preview scratch. `vim.fn.bufnr('<file>')` is
# WRONG here and must not be substituted: the scratch is named "Preview: <file>", so
# that pattern matches TWO buffers and bufnr() answers -1.
#
# The answer is the injected CHILD LANGTREE for the fenced language, primary check
# children()[lang] with a child:lang() fallback -- the same expression
# crates/wolf-ffi/tests/treesitter_highlight.rs uses. It is NOT
# `vim.treesitter.highlighter.active[buf]`: that reads true even with no wolf
# init.lua at all, because the pinned Neovim starts treesitter on markdown by
# itself, so it would pass under this smoke's own falsifier.
wtype -- ":lua local s=-1 for _,b in ipairs(vim.fn.tabpagebuflist()) do local v=vim.b[b].wolf_preview_source_bufnr if v~=nil then s=v end end local m='NO' if s>0 then local ok,p=pcall(vim.treesitter.get_parser, s) if ok and p then pcall(function() p:parse(true) local k=p:children() if k['$HL_LANG']~=nil then m='OK' else for _,c in pairs(k) do if c:lang()=='$HL_LANG' then m='OK' end end end end) end end vim.fn.writefile({m..' src='..s}, '$LANGTREE')"
wtype -k Return
wait_file "$LANGTREE" || exit 14

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// A fenced code block in the rendered preview is SYNTAX HIGHLIGHTED, with an
/// identical unknown-language block beside it that is not.
///
/// ## What it proves that nothing else can
///
/// That the vendored treesitter parsers reach the RUNNING GTK application and put
/// colour on the screen. `crates/wolf-ffi/tests/code_block_highlight.rs` proves that
/// `wolf_core` produces styled spans for a `go` fence; it says nothing about whether
/// a preview pane shows them, and it stages its own sibling trees in a temp directory
/// rather than going through the delivery route the application uses. This smoke
/// launches the built binary, opens a real preview, and reads the pixels.
///
/// ## The two blocks are the SAME TEXT, and that is the design
///
/// [`CODEHL_FENCE_BODY`] is interpolated into both fences, so the only difference
/// between them is the fence's language. Block 2 is therefore an IN-CAPTURE CONTROL
/// rather than a second measurement: it is rendered by the same engine, in the same
/// document, in the same run, at the same font, and every rendering variable except
/// the highlighter's answer is held constant across the pair.
///
/// ## Chroma, not "more than one colour"
///
/// "A highlighted block contains more than one ink colour" cannot be counted as
/// distinct RGB values: antialiasing produces hundreds of intermediates and the count
/// would be large for a plain block too. The rigorous form of the same claim is
/// [`chroma`]: every colour in an unhighlighted block is NEUTRAL, and blending two
/// neutral colours channel-independently yields a neutral colour, so an unhighlighted
/// block cannot produce chroma at all. Treesitter's token colours are chromatic.
///
/// ## Row B's evidence, and the expression that is NOT it
///
/// Claim 8 is the only place in this repository where the vendored trees are observed
/// inside the running application. It asks the captive Neovim whether the SOURCE
/// buffer's markdown langtree has an injected child for [`CODEHL_HL_LANG`] — a fact
/// Neovim produces, that no pixel and no OCR term can satisfy, and that is MEASURED to
/// answer NO when the gate does not fire.
///
/// `vim.treesitter.highlighter.active[buf] ~= nil` is the expression NOT used, and it
/// was the obvious candidate. MEASURED under three conditions with `--clean`: bare
/// nvim with no wolf `init.lua` at all, wolf `init.lua` with the gate NOT firing, and
/// wolf `init.lua` with the gate firing — it reads true in ALL THREE, because the
/// pinned Neovim starts treesitter on markdown by itself. It is a tautology, and an
/// assertion on it would have passed under this smoke's own named falsifier. The
/// injected langtree reads NO / NO / OK across the same three.
///
/// ## Claims 6, 7 and 8 are judged TOGETHER, and that is not a style choice
///
/// A chain of `assert!`s panics on the first one that fails, so a claim that only ever
/// fails alongside an earlier one has NO verbatim red of its own — nobody has ever read
/// its failure text, and nothing proves it can produce one. That is claim 8's position:
/// MEASURED, BOTH named falsifiers make the langtree read `NO src=2`, because an
/// injected child langtree needs the compiled parser as well as the injections query.
/// So every edit that reddens claim 8 reddens claim 6 first.
///
/// The three claims are therefore accumulated and reported in one combined failure,
/// each naming itself and its feat-list row. That matters because claims 6 and 8 are
/// DIFFERENT rows — Row A's preview colour and Row B's editor-side injections — and a
/// shared failure that named only the first would name the wrong one, which is the same
/// argument that made these two smokes separate in the first place. Row A also has
/// `crates/wolf-ffi/tests/code_block_highlight.rs` corroborating it; Row B's editor half
/// has no other evidence anywhere, so its own red text is the only text there is.
///
/// The preconditions (rows 0 to 5 below) are NOT accumulated: each one is what makes the
/// next meaningful, so evaluating past a failed precondition would report readings taken
/// from a rectangle that means nothing. They panic where they stand, and the code calls
/// them ASSERTIONS; rows 6, 7 and 8 are the accumulated ones and the code calls them
/// CLAIMS, so a message can be traced to its shape without counting.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. Readings are from actual runs at GTK 4.22.4, libadwaita 1.9.0,
/// WebKitGTK 2.52.4 and `cage` 0.3.0 on a 1280x720 output.
///
/// | # | assertion (0–5) or claim (6–8) | kind | falsifying edit | measured |
/// | --- | --- | --- | --- | --- |
/// | 0 | [`CODEHL_HL_LANG`] is not one of [`NVIM_BUILTIN_PARSERS`], and the two fence languages differ | **tripwire on the fixture** — no `src/` falsifier by construction | re-point the fence at `c` or `lua` | n/a. It runs before the compositor. Without it, a fence in one of the seven builtins would highlight with nothing vendored and claim 6 would stay green under its own falsifier. |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | the PNG is present and decodes at exactly `CAGE_OUTPUT_W_EXPECTED` x `CAGE_OUTPUT_H_EXPECTED` | precondition — and what makes every derived rectangle mean what it says | none; do not invent one | n/a |
/// | 3 | `tabpagebuflist()`, sorted, is the preview scratch plus the source | precondition — a preview pane exists, and exactly two panes do | none; do not invent one | green: `Preview: cbhl.md,cbhl.md` |
/// | 4 | `web_top` is locatable | precondition — the web view's own top edge, self-located from the pixels | none; do not invent one | green: absolute row 159 measured, at the content's top; 139 by derivation, the view's top above the root padding, not re-measured |
/// | 5 | exactly TWO majority-[`CODEHL_PRE_BG`] runs of at least [`CODEHL_MIN_BLOCK_ROWS`] rows, of EQUAL height | precondition — BOTH fences rendered as `pre` blocks, and NEITHER was clipped by the scan limit. The run COUNT cannot see a clipped block: a truncated run is still exactly one run, and a control rect short of its subject can only LOWER claim 7's count, which would make the control read stronger than it is. | none; do not invent one | green: rows 231..375 and 399..543, both 144 tall, 24-row gap at 0 % `CODEHL_PRE_BG`, 57 rows of margin below. MEASURED on a rejected six-line fixture: two runs, the second clipped to 153 rows against 192, 20 % of the control unsampled. |
/// | 6 | run 1 carries at least [`CODEHL_MIN_CHROMA_PX`] chromatic pixels | **discriminator** — the vendored parsers reach the rendered document (feat-list row "Syntax-highlighted code blocks") | in `runtime/nvim/init.lua`, revert the PARSERS probe to a bare sibling `uv.fs_stat` (leave the queries fallback). `parser/go.so` is then off the runtimepath, the block renders plain, and this count collapses to the control's level. ISOLATED against the FFI suite, and this is not obvious: all three `wolf-ffi` treesitter tests stay GREEN, because none of them reaches `init.lua`'s environment fallback — two stage real siblings in a temp directory and the third loads the `.so` by absolute path. It is NOT isolated from claim 8, which reddens with it. | green: 1209 at threshold 32, max chroma 108, over 50,976 pixels |
/// | 7 | run 2 carries at most [`CODEHL_MAX_CHROMA_PX`] chromatic pixels | **in-capture control** — the chroma in run 1 comes from the highlighter and not from the renderer | none of its own. On a host with subpixel antialiasing it would fire in the GREEN run if `CODEHL_CHROMA_MIN` were ever set too low; on this host, whose antialiasing is grayscale, that failure mode is invisible and the threshold's legal window is what guards it instead. | green: **0**, at every threshold down to `>= 1`, over all 50,976 pixels |
/// | 8 | the langtree readback's first token is [`CODEHL_LANGTREE_OK`] | **discriminator** — the vendored trees fire `init.lua`'s gate inside the running application (feat-list row "Vendored treesitter plugins and parsers", its editor half) | in `runtime/nvim/init.lua`, delete the environment fallback from the QUERIES probe (leave the parsers one). The outer gate then does not fire, so NEITHER tree lands on the runtimepath and this reads NO. Claim 6 reddens under this edit too, and claim 6's OWN falsifier reddens this one — MEASURED, the parsers-probe edit also reads `NO src=2`, because an injected child langtree needs the parser as well as the injections query. Neither falsifier isolates it, which is exactly why the three claims are accumulated instead of chained. | green: `OK src=2` |
///
/// ## The conventions it obeys
///
/// A fresh temp working directory ([`make_temp_cwd`], mandatory — the captive
/// Neovim's swap directory is keyed on it); [`require_smoke_env`]'s cage slot and runtime directory for the whole body; one
/// `sh -c` string through [`cage_command`], the single environment helper and the only
/// place the compositor is spawned; and a config directory of its own pinning the
/// appearance the two theme colours are measured at.
///
/// It asserts no OCR term, so it contributes no row to `ASSERTED_OCR_TERMS`, and its
/// one fixture is read as text by nothing — the sidebar draws its NAME, and a name
/// cannot satisfy a claim about a colour — so it contributes no row to
/// `RELAUNCH_FIXTURE_NAMES` either. That is `PREVIEW_THEME_SOURCE_FILE`'s stated
/// precedent, which is absent from that table for the same reason.
#[test]
fn preview_highlights_a_fenced_code_block_under_cage() {
    const SMOKE: &str = "preview_highlights_a_fenced_code_block_under_cage";

    // Assertion 0 (tripwire on the fixture): before launch, since the rule and the assertion it can
    // disarm sit hundreds of lines apart.
    assert!(
        !NVIM_BUILTIN_PARSERS.contains(&CODEHL_HL_LANG),
        "{SMOKE}: the highlighted fence's language {CODEHL_HL_LANG:?} is one of \
         {NVIM_BUILTIN_PARSERS:?} — the parsers the pinned Neovim ships with a `highlights` query \
         and NOTHING vendored. A fence in one of those seven highlights on a bare checkout, so \
         claim 6 below would stay GREEN under its own named falsifier (deleting `init.lua`'s \
         environment fallback) and this smoke would prove nothing about the vendoring at all. Pick \
         a language that is in `flake.nix`'s sixteen and NOT in this list; `go` is the one this \
         fixture was measured with."
    );
    assert_ne!(
        CODEHL_HL_LANG, CODEHL_PLAIN_LANG,
        "{SMOKE}: both fences name the same language, so the document carries two highlighted \
         blocks or two plain ones and the in-capture control is a second copy of the \
         discriminator. The control fence must name a language NO grammar exists for."
    );
    assert!(
        !NVIM_BUILTIN_PARSERS.contains(&CODEHL_PLAIN_LANG),
        "{SMOKE}: the control fence's language {CODEHL_PLAIN_LANG:?} is one of \
         {NVIM_BUILTIN_PARSERS:?}, so the control block would be highlighted too and claim 7 \
         would fail on a perfectly good render. The control must name a language nothing can \
         parse."
    );

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let body = codehl_source_body();
    write_relaunch_fixtures(SMOKE, &cwd, &[(CODEHL_SOURCE_FILE, body.as_str())]);

    // Its own config at a forced dark appearance: the asserted colours are the dark palette's, so a
    // re-pin of the shared seeded value would sample a light document while still finding rows.
    let config = smoke_config_dir_at(
        &cwd,
        CODEHL_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_DARK,
        None,
    );

    let out_png = runtime_dir.join(CODEHL_PNG_NAME);
    let wins = runtime_dir.join(CODEHL_WINS_NAME);
    let langtree = runtime_dir.join(CODEHL_LANGTREE_NAME);
    for path in [&out_png, &wins, &langtree] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(CODEHL_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        // Chained AFTER `cage_command`, deliberately winning over the shared config
        // directory it set — the same override other smokes make for `HOME`.
        .env("WOLF_CONFIG_DIR", &config)
        .env("SRC_FILE", CODEHL_SOURCE_FILE)
        .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
        .env("HL_LANG", CODEHL_HL_LANG)
        .env("OUT_PNG", &out_png)
        .env("WINS", &wins)
        .env("LANGTREE", &langtree);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless code-highlight scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage code-highlight scenario exited unsuccessfully ({status}). Exit 11: the \
         app died before it was ready for input. Exit 14: one of the two readbacks never appeared — \
         {} (the window list) or {} (the injected langtree); the second is a long typed `:lua` \
         line, so an empty file there means the command line did not execute as typed rather than \
         that treesitter answered anything. Codes 12 and 13 do not exist here on purpose: nothing \
         in this smoke depends on a clean exit, so the scenario ends with a best-effort kill. Any \
         OTHER status is `grim` or `wtype` failing under `set -eu`. Note this assertion says \
         nothing about whether a preview ever opened — the scenario exits 0 whatever the palette \
         did.",
        wins.display(),
        langtree.display()
    );

    // --- Assertion 2 (precondition): the capture decodes at the pinned size ---
    assert_png_present(SMOKE, &out_png);
    let capture = decode_capture(&out_png, SMOKE, "the code-highlight capture");

    // Assertion 3 (precondition): Neovim's own window buffer names, read before any pixel, since a band
    // inside a pane that never became a preview fails claim 6 for nothing.
    let want_windows = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{CODEHL_SOURCE_FILE}"),
            CODEHL_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_windows = read_window_buffer_names(&wins, SMOKE);
    assert_eq!(
        got_windows, want_windows,
        "{SMOKE}: after the Preview leg the captive Neovim's windows hold {got_windows:?}, not \
         {want_windows:?} (both sorted, so window order is not the subject). A SINGLE entry naming \
         {CODEHL_SOURCE_FILE:?} means no preview was bound at all — the palette dispatched \
         nothing, or the {PREVIEW_OTHER_PANE_QUERY:?} query matched no row. A SINGLE entry naming \
         the preview scratch means the IN-PLACE Preview ran rather than the other-pane one, so the \
         preview replaced the window's only pane and there is no right half for the sampled \
         columns. THREE \
         entries mean it split twice. Every rectangle below assumes exactly two side-by-side \
         panes, so none of them means anything until this holds."
    );

    let preview_columns = preview_theme_preview_columns();
    let (preview_x0, preview_x1) = preview_columns;
    let scan_rows = preview_theme_scan_rows();
    let (_, band_y1) = preview_theme_band_rows();

    // Assertion 4 (precondition): self-located from the pixels, so a moved web view moves every
    // rectangle below with it.
    let web_top = capture
        .first_uniform_row(preview_columns, scan_rows, PREVIEW_DARK_BG)
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: no row between {} and {} is entirely PREVIEW_DARK_BG = {PREVIEW_DARK_BG:?} \
                 across columns {preview_x0}..{preview_x1}, so the web view's top edge could not be \
                 located and nothing below has an origin. Two causes: the document is not rendering \
                 at all (an empty `render_preview`, or `about:blank`), or its FIRST block is not the \
                 leading paragraph `CODEHL_PARAGRAPH` puts there — a document that starts with a \
                 `pre` has no run of pure background above it, which is exactly why that paragraph \
                 is mandatory. Capture kept at {}.",
                scan_rows.0,
                scan_rows.1,
                out_png.display()
            )
        });

    // --- Assertion 5 (precondition): BOTH fences rendered as `pre` blocks -----
    let block_scan = SampleRect {
        x0: preview_x0,
        x1: preview_x1,
        y0: web_top,
        y1: band_y1,
    };
    assert!(
        block_scan.y0 < block_scan.y1 && block_scan.y1 <= capture.height,
        "{SMOKE}: the block-scan rectangle {block_scan:?} is degenerate or runs off the bottom of \
         the {}x{} capture. It spans from the located web view top ({web_top}) to \
         `preview_theme_band_rows`'s last row, so this means the located top edge is below that \
         row — far lower than the pane chrome can account for.",
        capture.width,
        capture.height
    );
    let blocks = majority_colour_runs(&capture, block_scan, CODEHL_PRE_BG, CODEHL_MIN_BLOCK_ROWS);
    assert_eq!(
        blocks.len(),
        2,
        "{SMOKE}: found {} majority-CODEHL_PRE_BG run(s) of at least {CODEHL_MIN_BLOCK_ROWS} rows \
         below the web view's top edge (row {web_top}), expected exactly 2: {blocks:?}. ZERO means \
         no code block rendered as a `pre` at all — the document is not the fixture, or \
         `CODEHL_PRE_BG` = {CODEHL_PRE_BG:?} is no longer the `pre` background in \
         `wolf-core`'s `markdown/themes/default.css`, or the appearance is not dark. ONE means the \
         two blocks merged, which needs the 24 px inter-block gap to have disappeared — or means \
         one fence did not render as a block, which for the {CODEHL_PLAIN_LANG:?} one would say \
         `render_source_html` stopped degrading an unknown language to the plain render. MORE than \
         two means something else on the page paints that colour across the sampled width. Both \
         blocks are located from the PIXELS, so this says nothing about where the renderer put \
         them. Capture kept at {}.",
        blocks.len(),
        out_png.display()
    );

    // Same run count and same height: a run clipped by the scan limit is still one run, and a clipped
    // control samples fewer pixels, overstating claim 7 with nothing red.
    assert_eq!(
        blocks[0].rows(),
        blocks[1].rows(),
        "{SMOKE}: the two code blocks are {} and {} rows tall ({blocks:?}), and byte-identical \
         fence bodies must render at identical heights. Three readings, told apart by WHERE the \
         short one ends. Run 2 ending at exactly {} — the block scan's own last row — means the \
         document outgrew the scan: the control block is CLIPPED, so it samples fewer pixels than \
         the block it controls, and since a smaller rectangle can only lower a chroma count the \
         control below would read stronger than it is. The fix is to shorten the fixture — fewer \
         fence lines, or a shorter `CODEHL_PARAGRAPH` — never to raise the scan limit or lower a \
         threshold. Run 2 shorter but ending ABOVE that row means the two fences no longer carry \
         the same text, so the in-capture control has stopped being a control. Run 1 shorter means \
         the FIRST block is the clipped or altered one, which inverts the reading. Capture kept at \
         {}.",
        blocks[0].rows(),
        blocks[1].rows(),
        block_scan.y1,
        out_png.display()
    );

    let hl_rect = blocks[0].rect(preview_columns);
    let plain_rect = blocks[1].rect(preview_columns);
    let hl = chroma_sample(&capture, hl_rect);
    let plain = chroma_sample(&capture, plain_rect);

    // The langtree readback is read HERE, before any claim is judged, so that claim 8 is
    // evaluated from data already in hand rather than after two claims that could panic.
    let raw = read_readback(&langtree, SMOKE);
    let first = raw.split_whitespace().next().unwrap_or("");

    // Claims 6, 7 and 8 are collected into one failure, since claim 8's falsifiers redden 6 first (see
    // the doc comment). Every claim must push on its failing branch, or it passes silently.
    let mut unmet: Vec<String> = Vec::new();

    // Claim 6 (discriminator, Row A): the HIGHLIGHTED block carries chroma.
    if hl.count < CODEHL_MIN_CHROMA_PX {
        unmet.push(format!(
            "CLAIM 6 — feat-list row \"Syntax-highlighted code blocks\". The FIRST code block, the \
             {CODEHL_HL_LANG:?} fence at {hl_rect:?} ({} pixels), carries only {} pixels whose \
             chroma reaches {CODEHL_CHROMA_MIN}, below the {CODEHL_MIN_CHROMA_PX} floor; its \
             largest chroma anywhere was {}. The control block beside it read {} chromatic pixels \
             with a maximum of {}, and the two blocks carry BYTE-IDENTICAL text, so a reading at \
             the control's level means the block was not highlighted rather than that it rendered \
             differently.\n\
             The named cause is the vendored parsers not reaching the captive Neovim: \
             `runtime/nvim/init.lua`'s gate resolves `parsers/` from a sibling directory first and \
             `WOLF_PARSERS_DIR` second, and on Linux only the second exists. CLAIM 8 tells that \
             cause apart from a downstream one, and it is judged in this same run: if CLAIM 8 is \
             ALSO in this message, the gate did not deliver the trees at all and this is the \
             vendoring. If it is NOT, the parser is working and the colour was lost downstream, in \
             `wolf_core`'s `render_source_html` or in the inline `style=\"color:…\"` spans it \
             emits.\n\
             A max chroma near zero with plenty of ink means the block rendered NEUTRAL. A max \
             chroma above {CODEHL_CHROMA_MIN} with a count below the floor means the palette \
             moved rather than the highlighting stopped — a re-measurement, not a regression, and \
             the MEASURED headroom is 4.0x, so one lost token cannot get you here. Do NOT lower \
             this floor: `codehl_chroma_thresholds_are_separated` exists because a floor below \
             the control's ceiling disarms the pair.",
            hl_rect.pixels(),
            hl.count,
            hl.max,
            plain.count,
            plain.max
        ));
    }

    // Claim 7 (in-capture control for claim 6): the plain block is the same text in the same run, so
    // any chroma the engine made on its own would appear in both.
    if plain.count > CODEHL_MAX_CHROMA_PX {
        unmet.push(format!(
            "CLAIM 7 — the IN-CAPTURE CONTROL that makes claim 6 evidence about the highlighter. \
             The SECOND code block, the {CODEHL_PLAIN_LANG:?} fence at {plain_rect:?}, carries {} \
             pixels whose chroma reaches {CODEHL_CHROMA_MIN}, above the {CODEHL_MAX_CHROMA_PX} \
             ceiling (largest chroma {}). The measured value is 0, at every threshold down to 1, \
             so this is a real change and not drift. Two readings, told apart by the size of the \
             number. A count near the highlighted block's {} means a grammar was found for \
             {CODEHL_PLAIN_LANG:?} — which would make it a second measurement rather than a \
             control, and the fix is to rename the fence, never to raise this ceiling. A small \
             count means this host's antialiasing produces subpixel colour fringes where the \
             measurement host's produced none, at which point `CODEHL_CHROMA_MIN` is the constant \
             to re-derive: raise it until the control reads zero again and re-measure the \
             highlighted block against the new threshold.",
            plain.count, plain.max, hl.count
        ));
    }

    // Claim 8 (discriminator, Row B): the injected langtree, from Neovim; no pixel can satisfy it.
    if first != CODEHL_LANGTREE_OK {
        unmet.push(format!(
            "CLAIM 8 — feat-list row \"Vendored treesitter plugins and parsers\", its EDITOR half. \
             The captive Neovim's langtree readback begins {first:?}, not {CODEHL_LANGTREE_OK:?}. \
             Raw contents were {raw:?} (the trailing `src=<n>` is the source buffer the preview \
             scratch's `b:wolf_preview_source_bufnr` named; `src=-1` means NO preview scratch \
             carried that variable, which is a different failure — the readback found no preview \
             at all, and assertion 3 above should have caught it).\n\
             What this asks: after a forced full parse, does the SOURCE buffer's markdown langtree \
             have an injected child for {CODEHL_HL_LANG:?}? That needs BOTH vendored trees on the \
             runtimepath — nvim-treesitter's `queries/markdown/injections.scm` to route the fenced \
             region, and a compiled `parser/{CODEHL_HL_LANG}.so` to parse it — so a NO means \
             `runtime/nvim/init.lua`'s gate did not deliver them. MEASURED: BOTH named falsifiers \
             produce `NO src=2`, the queries one because the outer gate then does not fire at all \
             and neither tree lands, the parsers one because an injected child needs the compiled \
             parser too. Nothing here isolates which probe lost its environment fallback; read \
             `init.lua`'s gate to find out.\n\
             Do NOT replace this with `vim.treesitter.highlighter.active[buf] ~= nil`. MEASURED: \
             that reads true with no wolf `init.lua` at all, because the pinned Neovim starts \
             treesitter on markdown by itself — it is a tautology that would pass under this \
             smoke's own falsifier."
        ));
    }

    assert!(
        unmet.is_empty(),
        "{SMOKE}: {} of this smoke's three claims are unmet, and ALL of them are stated below. \
         They are reported together rather than chained because they belong to different \
         feat-list rows — claim 6 is Row A (the preview shows colour), claim 7 is claim 6's \
         in-capture control, and claim 8 is Row B's editor half, which has no other evidence \
         anywhere in this repository. A chain would panic on the first and name the wrong row. \
         Capture kept at {}; the langtree readback is kept at {}.\n\n{}",
        unmet.len(),
        out_png.display(),
        langtree.display(),
        unmet.join("\n\n")
    );
}

/// The chroma floor and the chroma ceiling are far enough apart that a de-highlighted
/// block cannot clear the floor.
///
/// ## What it closes
///
/// [`CODEHL_MIN_CHROMA_PX`] and [`CODEHL_MAX_CHROMA_PX`] are two numbers a thousand
/// lines from each other, and the pair of assertions they drive is only a
/// discriminator while the floor sits well ABOVE the ceiling. Set the floor at or
/// below what the CONTROL block itself reads, and a block that lost its highlighting
/// entirely would still clear it: both assertions would pass, the smoke would be green,
/// and nothing in it would name the constant that disarmed it.
///
/// Five times is an ACCEPTANCE CRITERION rather than a tuned number: it is the same
/// separation the design required of the two blocks' own measured counts before the
/// pixel assertion was allowed to ship. The measured values clear it with room —
/// 300 >= 25 — and the measured control is 0, so the real separation is unbounded.
///
/// ## Why it asserts on a SHORTFALL rather than on the comparison
///
/// `assert!(CODEHL_MIN_CHROMA_PX >= CODEHL_MAX_CHROMA_PX * 5)` compares two constants,
/// and `clippy::assertions_on_constants` rejects an `assert!` whose whole condition is
/// constant — under `-D warnings` that is a build failure, and suppressing it with an
/// `#[allow]` would suppress the same lint for anything added here later. `assert_eq!`
/// is outside that lint entirely, which is why
/// [`the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`] can compare
/// two constants directly. So the inequality is restated as an equality on the amount
/// by which the floor falls SHORT of the required separation: zero exactly when the
/// floor clears it, and the shortfall itself is a more useful number in the failure
/// than either operand. A `const {}` block would also compile away the lint, but it
/// would turn a named test failure into a compile error and lose the message below —
/// which is the whole point of the guard.
///
/// ## It is a tripwire on a pin, with NO `src/` falsifier by construction
///
/// It guards two constants in this test file, so both of its red runs are edits to
/// THIS file rather than to production code: lower [`CODEHL_MIN_CHROMA_PX`] below five
/// times [`CODEHL_MAX_CHROMA_PX`], or raise the ceiling past a fifth of the floor.
/// That is exactly the shape
/// [`the_shared_smoke_config_pins_the_appearance_every_ocr_smoke_assumes`] has, and it
/// is recorded as such rather than presented as coverage of anything the application
/// does.
///
/// ## What it needs
///
/// NO environment: no compositor, no [`require_smoke_env`], no cage slot, no
/// subprocess. It runs in microseconds and still guards on a machine where the smoke
/// toolchain is unavailable.
#[test]
fn codehl_chroma_thresholds_are_separated() {
    const GUARD: &str = "codehl_chroma_thresholds_are_separated";
    // The multiple of the CONTROL block's ceiling that the HIGHLIGHTED block's floor
    // must reach.
    const SEPARATION: usize = 5;

    let required_floor = CODEHL_MAX_CHROMA_PX * SEPARATION;
    let shortfall = required_floor.saturating_sub(CODEHL_MIN_CHROMA_PX);
    assert_eq!(
        shortfall, 0,
        "{GUARD}: CODEHL_MIN_CHROMA_PX is {CODEHL_MIN_CHROMA_PX} and CODEHL_MAX_CHROMA_PX is \
         {CODEHL_MAX_CHROMA_PX}, so the floor the HIGHLIGHTED block must clear falls {shortfall} \
         pixels short of {SEPARATION} times the ceiling the CONTROL block must stay under \
         ({required_floor}). At that separation \
         `preview_highlights_a_fenced_code_block_under_cage` stops discriminating: a block that \
         lost its highlighting completely reads at the control's level, and if that level clears \
         the floor then BOTH of its chroma claims pass on a document with no colour in it at all \
         — green, with nothing naming the constant that disarmed it. The measured values are 1209 \
         chromatic pixels in the highlighted block and 0 in the control, so restore the floor and \
         the ceiling to values that keep this ratio rather than widening it to admit them."
    );
}

// --- The code block's background in each configured appearance ---------------------------------

/// The markdown file the code-background smoke opens and then previews.
///
/// Nothing in this smoke is OCR'd, so this name contributes no row to
/// `RELAUNCH_FIXTURE_NAMES`, for the reason [`CODEHL_SOURCE_FILE`] states.
const CODEBG_SOURCE_FILE: &str = "pvcodebg.md";

/// The body of the one fenced block in [`codebg_source_body`]: four lines of plain words.
///
/// No info string goes on the fence, so no highlighter is on the path and every span inside
/// the block is the plain render's. No tab and no markdown syntax, so each line's width is its
/// character count at the monospace advance.
///
/// ## Why every line is between 32 and 44 characters
///
/// The lower bound is what lets the smoke see a background painted BEHIND the text. A line's
/// inline box starts left of [`preview_theme_preview_columns`], so a colour painted on it is a
/// strict majority of the sampled columns only when the line reaches well past their midpoint.
/// A short line would leave such a colour a minority on every row, and the `pre` colour would
/// still read as one unbroken run under the falsifier that removes `pre code`'s transparency.
/// The same width puts ink across most of the sampled columns, which is why the smoke bridges
/// short gaps between runs: see [`CODEBG_MAX_INK_GAP_ROWS`].
///
/// The upper bound keeps each line inside the `pre`'s content box. The `pre` has
/// `overflow: auto`, so a longer line would add a horizontal scrollbar, whose track is not the
/// `pre` colour, along the block's bottom edge.
///
/// Assertion 0 checks both bounds before anything launches.
const CODEBG_FENCE_BODY: &str = "alfa bravo charlie delta echo foxtrot\n\
                                 golf hotel india juliett kilo lima mike\n\
                                 november oscar papa quebec romeo sierra\n\
                                 tango uniform victor whiskey xray yankee";

/// The fewest characters a line of [`CODEBG_FENCE_BODY`] may have. See its doc comment.
const CODEBG_LINE_MIN_CHARS: usize = 32;

/// The most characters a line of [`CODEBG_FENCE_BODY`] may have. See its doc comment.
const CODEBG_LINE_MAX_CHARS: usize = 44;

/// The tallest gap, in rows, that [`bridged_majority_runs`] closes between two majority runs of
/// the `pre` colour. A quarter of the 24 px baseline unit.
///
/// The lower side of the margin is the text's own ink. Every fence line reaches past the sampled
/// columns' midpoint (see [`CODEBG_FENCE_BODY`]), so on the densest stroke rows of a line, such
/// as its x-height top, the glyphs can leave the `pre` colour a minority for a row or two even
/// though the block is painted behind them. Without the bridge those rows split one block in two.
///
/// The upper side is what row 6c's falsifier opens. A background painted behind the lines covers
/// each line's whole content area, most of its line box, so the gaps it leaves in the `pre`
/// colour are many times this bridge, and closing ink gaps does not close them.
///
/// Widening the bridge towards a line box disarms row 6c. Each launch's `readings:` line prints
/// the unbridged strips, so measure the gaps there before changing this.
const CODEBG_MAX_INK_GAP_ROWS: usize = 6;

/// The whole body of [`CODEBG_SOURCE_FILE`]: [`CODEHL_PARAGRAPH`], then the fence.
///
/// The leading paragraph is mandatory for the reason [`CODEHL_PARAGRAPH`] gives: `web_top` is
/// the first row of pure body background, and a document that opened with the `pre` would
/// have none above it. The same paragraph puts the block where the code-highlight smoke's first
/// block lands, well clear of the scan's last row.
fn codebg_source_body() -> String {
    format!("{CODEHL_PARAGRAPH}\n\n```\n{CODEBG_FENCE_BODY}\n```")
}

/// Name suffix of run 1's config and state directory siblings.
const CODEBG_DARK_SUFFIX: &str = "-codebg-dark";

/// Name suffix of run 2's config and state directory siblings. Must differ from
/// [`CODEBG_DARK_SUFFIX`].
const CODEBG_LIGHT_SUFFIX: &str = "-codebg-light";

/// Run 1's capture, under `XDG_RUNTIME_DIR` for the reason [`CODEHL_PNG_NAME`] gives.
const CODEBG_DARK_PNG_NAME: &str = "wolf_gnome_code_bg_dark.png";

/// Run 2's capture. See [`CODEBG_DARK_PNG_NAME`].
const CODEBG_LIGHT_PNG_NAME: &str = "wolf_gnome_code_bg_light.png";

/// Run 1's window-list readback. See [`CODEBG_DARK_PNG_NAME`].
const CODEBG_DARK_WINS_NAME: &str = "wolf_gnome_code_bg_wins_1.txt";

/// Run 2's window-list readback. See [`CODEBG_DARK_PNG_NAME`].
const CODEBG_LIGHT_WINS_NAME: &str = "wolf_gnome_code_bg_wins_2.txt";

/// The `background-color` `wolf_core`'s `markdown/themes/default.css` gives
/// `body.wolf-markdown pre` in LIGHT, from its `@media (prefers-color-scheme: light)` block, as
/// `(r, g, b)`. The dark one is [`CODEHL_PRE_BG`].
///
/// The bundled Lighthouse colorscheme's light `slightHighlightBG`, copied into the theme's fixed
/// palette. Re-typed and never read from Lighthouse, for the reason [`CODEHL_PRE_BG`] gives.
const CODEBG_LIGHT_PRE_BG: Rgb8 = (0xff, 0xff, 0xff);

/// One launch's evidence, so the per-launch assertions read as one loop rather than two copies
/// that can drift.
///
/// Named fields rather than a tuple, for the reason `ThemeLaunch`'s doc comment gives: a
/// transposed `pre_bg` / `other_pre_bg` would turn assertion 6 into its own negation while
/// still compiling.
struct CodeBgLaunch<'a> {
    /// How a failure names this launch, matching the scenario's own stderr labels.
    label: &'a str,
    /// The launch's decoded capture.
    capture: &'a Capture,
    /// The body background this launch's appearance should have produced.
    body_bg: Rgb8,
    /// The const name of `body_bg`, for the failure message.
    body_bg_name: &'a str,
    /// The `pre` background this launch's appearance should have produced.
    pre_bg: Rgb8,
    /// The const name of `pre_bg`, for the failure message.
    pre_bg_name: &'a str,
    /// The OTHER appearance's `pre` background.
    other_pre_bg: Rgb8,
    /// The const name of `other_pre_bg`, for the failure message.
    other_pre_bg_name: &'a str,
}

/// The rendered preview's fenced code block is painted `#252525` under `appearance: dark` and
/// `#ffffff` under `appearance: light`, behind every line of its text.
///
/// Both values are the bundled Lighthouse colorscheme's `slightHighlightBG`, copied into the
/// preview theme's fixed palette. [`CODEHL_PRE_BG`] and [`CODEBG_LIGHT_PRE_BG`] state them as
/// literals, so a Lighthouse bump moves neither expectation.
///
/// ## The pair is what discriminates
///
/// For the reason `preview_document_follows_the_configured_appearance_under_cage`'s doc comment
/// gives under its own heading of that name: under `cage` the ambient scheme is host-determined,
/// so either launch alone can be the weak half, and neither may be dropped as redundant.
///
/// ## It runs the appearance smoke's scenario verbatim
///
/// [`PREVIEW_THEME_SCENARIO`] is reused unchanged against this smoke's own fixture, so the two
/// launches are that smoke's two launches, exit codes included. Its cage log therefore carries
/// that scenario's "preview-appearance" labels: a line naming that smoke in this smoke's output
/// is expected, not a sign the wrong test ran. Its fixture is not reused, because
/// `PREVIEW_THEME_SOURCE_BODY` must stay one paragraph for that smoke's band arithmetic and for
/// the packaged smoke that shares it.
///
/// ## The control, and the premise under it
///
/// Assertion 6 reads bridged majority-colour rows across the preview pane. Assertion 5 reads the
/// same rows across the editor pane and requires no run of the launch's `pre` colour there. That is
/// not a formality: orlando fills a fenced block's rows with Lighthouse's `slightHighlightBG`,
/// so an editor running Lighthouse and orlando would draw the source's fence in exactly the
/// colour asserted on. Under this harness the captive Neovim runs its default colorscheme, whose
/// backgrounds are neither value; assertion 5 measures that premise rather than assuming it.
///
/// ## Assertions, classified
///
/// A precondition gets no revert: inventing one would manufacture a discriminator this smoke
/// does not have. The table carries no measured counts: each launch prints its own on a
/// `readings:` line.
///
/// | # | assertion | kind | falsifying edit |
/// | --- | --- | --- | --- |
/// | 0 | [`CODEBG_FENCE_BODY`] is four lines of [`CODEBG_LINE_MIN_CHARS`] to [`CODEBG_LINE_MAX_CHARS`] characters, with no tab | tripwire on the fixture | none in `src/`; a fixture edit that would disarm the falsifier of row 6c |
/// | 1 | `status.success()` | precondition — the two-launch cage scenario ran to completion | none; do not invent one |
/// | 2 | both PNGs present, each decoding at the pinned output size | precondition | none; do not invent one |
/// | 3 | each launch's `tabpagebuflist()`, sorted, is the preview scratch plus the source | precondition — exactly two panes, the right one a preview | none; do not invent one |
/// | 4 | `web_top`, the first row entirely the launch's body background across the preview columns, is locatable | precondition | none; do not invent one |
/// | 5 | no bridged majority run of the launch's `pre` colour, of at least [`CODEHL_MIN_BLOCK_ROWS`] rows, in the editor pane | control, self-checking | none |
/// | 6a | exactly one bridged majority run of it in the preview pane, run 1 (dark) | **discriminator** | in `wolf-core`'s `markdown/themes/default.css`, the dark `body.wolf-markdown pre` back to `#161616`; run 1 reads zero runs |
/// | 6b | the same, run 2 (light) | **discriminator** | the light block's `body.wolf-markdown pre` back to `#eaeaea`; run 2 reads zero runs, run 1 passes |
/// | 6c | the same, behind every line | **discriminator** | delete `background: transparent;` from `body.wolf-markdown pre code` in `wolf-core`'s `markdown/base.css`; the theme's inline `code` background behind each line splits run 1's block into strips shorter than [`CODEHL_MIN_BLOCK_ROWS`], separated by gaps taller than [`CODEBG_MAX_INK_GAP_ROWS`] |
#[test]
fn preview_code_block_background_follows_the_configured_appearance_under_cage() {
    const SMOKE: &str =
        "preview_code_block_background_follows_the_configured_appearance_under_cage";

    // Assertion 0 (tripwire on the fixture): a line outside the bounds disarms row 6c silently.
    let lines: Vec<&str> = CODEBG_FENCE_BODY.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "{SMOKE}: CODEBG_FENCE_BODY has {} lines, not 4: {lines:?}. The block's height is what \
         keeps it clear of the scan's last row below a wrapped CODEHL_PARAGRAPH, and a block \
         clipped there would still read as one run.",
        lines.len()
    );
    for line in &lines {
        let chars = line.chars().count();
        assert!(
            (CODEBG_LINE_MIN_CHARS..=CODEBG_LINE_MAX_CHARS).contains(&chars)
                && !line.contains('\t'),
            "{SMOKE}: the fence line {line:?} is {chars} characters, or carries a tab; every line \
             must be {CODEBG_LINE_MIN_CHARS} to {CODEBG_LINE_MAX_CHARS} characters with no tab. \
             Too short and a background painted behind the text is never a majority of the \
             sampled columns, so assertion 6 stays green with `pre code`'s transparency gone. Too \
             long and the `pre` grows a scrollbar. A tab's width is not its character count."
        );
    }

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    let body = codebg_source_body();
    write_relaunch_fixtures(SMOKE, &cwd, &[(CODEBG_SOURCE_FILE, body.as_str())]);

    // Both launches run at SMOKE_FONT_PT: the appearance is the one variable.
    let config_dark = smoke_config_dir_at(
        &cwd,
        CODEBG_DARK_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_DARK,
        None,
    );
    let config_light = smoke_config_dir_at(
        &cwd,
        CODEBG_LIGHT_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_LIGHT,
        None,
    );
    let state_dark = smoke_state_dir_at(&cwd, CODEBG_DARK_SUFFIX);
    let state_light = smoke_state_dir_at(&cwd, CODEBG_LIGHT_SUFFIX);

    assert_ne!(
        config_dark,
        config_light,
        "{SMOKE}: both runs were pointed at the SAME config directory {}, so they would read the \
         same appearance and one of the two launches could never see its own `pre` colour. The \
         two suffixes ({CODEBG_DARK_SUFFIX:?} and {CODEBG_LIGHT_SUFFIX:?}) must differ.",
        config_dark.display()
    );
    assert_ne!(
        state_dark,
        state_light,
        "{SMOKE}: both runs were pointed at the SAME state directory {}. Launch 1 quits with a \
         preview pane open, so launch 2 would restore that two-pane layout, split a second time \
         and put its preview where none of the sampled columns is aimed. See \
         `smoke_state_dir_at`.",
        state_dark.display()
    );

    let png_dark = runtime_dir.join(CODEBG_DARK_PNG_NAME);
    let png_light = runtime_dir.join(CODEBG_LIGHT_PNG_NAME);
    let wins_dark = runtime_dir.join(CODEBG_DARK_WINS_NAME);
    let wins_light = runtime_dir.join(CODEBG_LIGHT_WINS_NAME);
    for path in [&png_dark, &png_light, &wins_dark, &wins_light] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(PREVIEW_THEME_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("SRC_FILE", CODEBG_SOURCE_FILE)
        .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
        .env("CONFIG_DIR_DARK", &config_dark)
        .env("CONFIG_DIR_LIGHT", &config_light)
        .env("STATE_DIR_DARK", &state_dark)
        .env("STATE_DIR_LIGHT", &state_light)
        .env("OUT_PNG_DARK", &png_dark)
        .env("OUT_PNG_LIGHT", &png_light)
        .env("WINS_DARK", &wins_dark)
        .env("WINS_LIGHT", &wins_light);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless code-background scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage scenario, PREVIEW_THEME_SCENARIO run verbatim, exited unsuccessfully \
         ({status}); its stderr labels this smoke's launches \"preview-appearance\". Exit 11: run \
         1's app (`appearance: {APPEARANCE_FORCED_DARK}`) died before it was ready for input. \
         Exit 14: run 1's window-list readback never appeared at {}. Exit 15/18: the same two \
         for run 2 (`appearance: {APPEARANCE_FORCED_LIGHT}`), whose readback is at {}. \
         {QUIT_APP_EXITS} Any other \
         status is `grim` or `wtype` failing under `set -eu`. This assertion says nothing about \
         whether a preview opened or what it painted.",
        wins_dark.display(),
        wins_light.display()
    );

    // --- Assertion 2 (precondition): both captures decode at the pinned size --
    assert_png_present(SMOKE, &png_dark);
    assert_png_present(SMOKE, &png_light);
    let dark = decode_capture(&png_dark, SMOKE, "run 1 (appearance: dark)");
    let light = decode_capture(&png_light, SMOKE, "run 2 (appearance: light)");

    // Assertion 3 (precondition): Neovim's own window buffer names, read before any pixel, since a
    // pane that never became a preview fails assertion 6 for nothing.
    let want_windows = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{CODEBG_SOURCE_FILE}"),
            CODEBG_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    for (label, path) in [
        ("run 1 (appearance: dark)", &wins_dark),
        ("run 2 (appearance: light)", &wins_light),
    ] {
        let got = read_window_buffer_names(path, SMOKE);
        assert_eq!(
            got, want_windows,
            "{SMOKE}: after {label}'s Preview leg the captive Neovim's windows hold {got:?}, not \
             {want_windows:?} (both sorted). A SINGLE entry naming {CODEBG_SOURCE_FILE:?} means \
             no preview was bound: the palette dispatched nothing, or the \
             {PREVIEW_OTHER_PANE_QUERY:?} query matched no row. A SINGLE entry naming the preview \
             scratch means the in-place Preview ran, so there is no editor pane for assertion 5 \
             and no right half for assertion 6. THREE entries mean it split twice. Every \
             rectangle below assumes exactly two side-by-side panes."
        );
    }

    // Asserts per launch, so a red run reports only the first failing one; the second capture is on
    // disk regardless, so read it before concluding it was green.
    for launch in [
        CodeBgLaunch {
            label: "run 1 (appearance: dark)",
            capture: &dark,
            body_bg: PREVIEW_DARK_BG,
            body_bg_name: "PREVIEW_DARK_BG",
            pre_bg: CODEHL_PRE_BG,
            pre_bg_name: "CODEHL_PRE_BG",
            other_pre_bg: CODEBG_LIGHT_PRE_BG,
            other_pre_bg_name: "CODEBG_LIGHT_PRE_BG",
        },
        CodeBgLaunch {
            label: "run 2 (appearance: light)",
            capture: &light,
            body_bg: PREVIEW_LIGHT_BG,
            body_bg_name: "PREVIEW_LIGHT_BG",
            pre_bg: CODEBG_LIGHT_PRE_BG,
            pre_bg_name: "CODEBG_LIGHT_PRE_BG",
            other_pre_bg: CODEHL_PRE_BG,
            other_pre_bg_name: "CODEHL_PRE_BG",
        },
    ] {
        assert_code_bg_launch(SMOKE, &launch);
    }
}

/// Assertions 4, 5 and 6 for ONE launch of
/// [`preview_code_block_background_follows_the_configured_appearance_under_cage`].
///
/// Split out so the two launches run the identical checks, for the reason
/// `assert_preview_theme_launch` gives.
fn assert_code_bg_launch(smoke: &str, launch: &CodeBgLaunch<'_>) {
    let label = launch.label;
    let capture = launch.capture;
    let preview_columns = preview_theme_preview_columns();
    let (preview_x0, preview_x1) = preview_columns;
    let (editor_x0, editor_x1) = preview_theme_editor_columns();
    let scan_rows = preview_theme_scan_rows();
    let (_, band_y1) = preview_theme_band_rows();

    // Assertion 4 (precondition): self-located from the pixels, so a moved web view moves every
    // rectangle below with it.
    let web_top = capture
        .first_uniform_row(preview_columns, scan_rows, launch.body_bg)
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: {label}: no row between {} and {} is entirely {} = {:?} across columns \
                 {preview_x0}..{preview_x1}, so the web view's top edge could not be located. \
                 Three causes: the document is not rendering (an empty `render_preview`, or \
                 `about:blank`); the other appearance applied, so the body is the other palette's \
                 background; or the document's first block is not the leading paragraph, which \
                 `codebg_source_body` puts there because a `pre` first leaves no pure background \
                 above it.",
                scan_rows.0, scan_rows.1, launch.body_bg_name, launch.body_bg
            )
        });

    let block_scan = SampleRect {
        x0: preview_x0,
        x1: preview_x1,
        y0: web_top,
        y1: band_y1,
    };
    let control_scan = SampleRect {
        x0: editor_x0,
        x1: editor_x1,
        ..block_scan
    };
    for (what, rect) in [("block scan", block_scan), ("control", control_scan)] {
        assert!(
            rect.x0 < rect.x1
                && rect.y0 < rect.y1
                && rect.x1 <= capture.width
                && rect.y1 <= capture.height,
            "{smoke}: {label}: the {what} rectangle {rect:?} is degenerate or falls outside the \
             {}x{} capture. It spans from the located web view top ({web_top}) to \
             `preview_theme_band_rows`'s last row, so the located top is below that row, or the \
             derived columns have crossed.",
            capture.width,
            capture.height
        );
    }

    let bridged = |rect: SampleRect, colour: Rgb8| {
        bridged_majority_runs(
            capture,
            rect,
            colour,
            CODEBG_MAX_INK_GAP_ROWS,
            CODEHL_MIN_BLOCK_ROWS,
        )
    };
    let own = bridged(block_scan, launch.pre_bg);
    let own_strips = majority_colour_runs(capture, block_scan, launch.pre_bg, 1);
    let other = bridged(block_scan, launch.other_pre_bg);
    let control = bridged(control_scan, launch.pre_bg);
    eprintln!(
        "{smoke} readings: {label}: web top {web_top}; own {} bridged runs {own:?}; own strips of \
         any height, unbridged, {own_strips:?}; other {} bridged runs {other:?}; control bridged \
         runs {control:?}",
        launch.pre_bg_name, launch.other_pre_bg_name
    );

    // --- Assertion 5 (control, self-checking): the editor pane paints no block of this colour
    assert!(
        control.is_empty(),
        "{smoke}: {label}: the EDITOR pane, columns {editor_x0}..{editor_x1} over rows \
         {web_top}..{band_y1}, holds bridged majority runs of {} = {:?} of at least \
         {CODEHL_MIN_BLOCK_ROWS} rows, gaps of up to CODEBG_MAX_INK_GAP_ROWS = \
         {CODEBG_MAX_INK_GAP_ROWS} rows closed: {control:?}. So the captive Neovim paints the \
         preview's `pre` colour itself, and a run in the preview pane no longer proves the theme \
         painted it. The likely cause is the editor running Lighthouse with orlando, which fills \
         a fenced block with the same `slightHighlightBG`; a single run spanning the whole scan \
         instead means the panes are the wrong way round. Re-derive the fixture or the columns; \
         do not drop this control.",
        launch.pre_bg_name,
        launch.pre_bg
    );

    // --- Assertion 6 (discriminator): one block of this palette's colour, behind every line
    assert_eq!(
        own.len(),
        1,
        "{smoke}: {label}: the PREVIEW pane, columns {preview_x0}..{preview_x1} over rows \
         {web_top}..{band_y1}, holds {} bridged majority runs of {} = {:?} of at least \
         {CODEHL_MIN_BLOCK_ROWS} rows, gaps of up to CODEBG_MAX_INK_GAP_ROWS = \
         {CODEBG_MAX_INK_GAP_ROWS} rows closed, expected exactly 1: {own:?}. Over the same \
         rectangle the other palette's {} = {:?} reads these bridged runs: {other:?}; and this \
         palette's colour reads these unbridged runs of any height: {own_strips:?}. Read them \
         in this order. (a) One run of the OTHER colour: the other palette's `pre` applied, so \
         the configured appearance did not reach the document or this palette lost its `pre` \
         rule. (b) Several own strips, separated by gaps taller than CODEBG_MAX_INK_GAP_ROWS, \
         and no long run: the `pre` is this colour but something is painted behind each line \
         of text, which is `base.css`'s `pre code` losing `background: transparent` or a \
         highlight span gaining a background. (c) Nothing of either colour: `wolf-core`'s \
         `markdown/themes/default.css` no longer gives `body.wolf-markdown pre` this colour, or \
         no block rendered. (d) More than one long run: something else on the page paints this \
         colour across the sampled width, or a line's ink starved more consecutive rows than \
         the bridge closes, which shows as a gap of a few rows inside the block in the own \
         strips. Re-measure those strips before widening CODEBG_MAX_INK_GAP_ROWS, because \
         widening it towards a line box disarms the falsifier that removes `pre code`'s \
         transparency.",
        own.len(),
        launch.pre_bg_name,
        launch.pre_bg,
        launch.other_pre_bg_name,
        launch.other_pre_bg
    );
}

/// Every run of majority-`colour` rows inside `rect`, as [`majority_colour_runs`] reads them,
/// with each gap of at most `max_gap` rows merged into the run above it, and only then the runs
/// shorter than `min_rows` discarded.
///
/// The merge comes before the height filter because two ink-split halves of one block are each
/// a fragment of a real block: filtering first could drop a short half and leave the block read
/// as shorter, or as missing. See [`CODEBG_MAX_INK_GAP_ROWS`] for the margin on `max_gap`.
fn bridged_majority_runs(
    capture: &Capture,
    rect: SampleRect,
    colour: Rgb8,
    max_gap: usize,
    min_rows: usize,
) -> Vec<BlockRun> {
    let mut runs: Vec<BlockRun> = Vec::new();
    for run in majority_colour_runs(capture, rect, colour, 1) {
        match runs.last_mut() {
            Some(last) if run.y0 - last.y1 <= max_gap => last.y1 = run.y1,
            _ => runs.push(run),
        }
    }
    runs.retain(|run| run.rows() >= min_rows);
    runs
}

// --- Row C: inline code grows no line box, and a heading has a baseline below -----

/// The markdown file the baseline-grid smoke opens and then previews.
///
/// Nothing here is OCR'd either, so this name contributes no row to
/// `RELAUNCH_FIXTURE_NAMES` for the reason [`CODEHL_SOURCE_FILE`] does not.
const CODEGRID_SOURCE_FILE: &str = "cgrid.md";

/// The whole body of [`CODEGRID_SOURCE_FILE`]: four one-line paragraphs, each carrying
/// exactly ONE inline code element, with an h3, an h4 and an h6 between them, each
/// heading carrying one code element of its own, then a thematic break.
///
/// ## Every part of this shape is load-bearing
///
/// **Four paragraphs and three headings, not one paragraph.** The defect the code
/// element's `line-height: 0` removes is CUMULATIVE: each line carrying inline code was
/// 26 px rather than 24, so the error grows by 2 px per preceding code-carrying block. One
/// paragraph would separate the two states by 2 px; the four here separate them by 8
/// before the headings' lines add theirs, which is well outside any antialiasing question.
///
/// **A heading between each pair of paragraphs.** Each one is the subject of the
/// baseline-below claim, that the paragraph after it starts one baseline below the
/// heading's box. The levels are h3, h4 and h6: h3 is the one level with 48 px above, h4
/// and h6 are the 24 px levels at both ends of their font-size range, and h5 differs
/// from h4 only in its font size, which no assertion here reads.
///
/// **Every heading carries a code chip, well right of [`CODEGRID_CHIP_SPLIT_X_PX`].**
/// The chip is the heading box's landmark. Heading ink is white, but at 14 px a semibold
/// stem may never cover a whole pixel, so exact-colour heading ink is unreliable; the
/// chip's flat background is not. The chip adds nothing to the heading's line box,
/// because the code element declares `line-height: 0`. Each heading's chip follows at
/// least 20 characters of heading text, and each paragraph's follows one short word,
/// which is what lets one column split the two kinds.
///
/// **The first paragraph opens the document.** Its line box starts at the content's top,
/// since the first block's top margin is reset, so its chip's top row is the origin every
/// other chip is measured from, and its readings stand as measured.
///
/// **One code element per block, and each block fits ONE line.** A block that wrapped
/// would add a line box that carries no code, and the arithmetic below counts
/// code-carrying lines.
///
/// **The trailing thematic break is the RULER.** `base.css` gives `hr` a 24 px box
/// whose 1 px hairline is its first pixel row, and the theme colours that hairline
/// through `border-top-color` — so the rule's top edge is locatable to a single pixel
/// row, across the full content width. Nothing else in this stylesheet is.
///
/// **A blank line before it.** Without one, CommonMark reads `---` as a setext
/// underline for the paragraph above and there is no rule at all.
///
/// The words are NATO and the code spans are NATO, so no part of the document is an
/// `ASSERTED_OCR_TERMS` entry, nor is any run of adjacent words with the spaces dropped —
/// this smoke asserts none, but a rendered body is page text and the rule is the file's,
/// not the smoke's.
const CODEGRID_SOURCE_BODY: &str = "alfa `bravo` charlie.\n\n\
                                    ### mike november oscar `papa`\n\n\
                                    delta `echo` foxtrot.\n\n\
                                    #### quebec romeo sierra `tango`\n\n\
                                    golf `hotel` india.\n\n\
                                    ###### uniform victor whiskey xray `yankee`\n\n\
                                    kilo `lima` zulu.\n\n\
                                    ---";

/// Name suffix of the baseline-grid smoke's config directory sibling.
const CODEGRID_SUFFIX: &str = "-codegrid";

/// The baseline-grid smoke's capture. Under `XDG_RUNTIME_DIR`, for the reason
/// [`CODEHL_PNG_NAME`] gives.
const CODEGRID_PNG_NAME: &str = "wolf_gnome_code_grid.png";

/// File the captive Neovim writes its window/buffer list into.
const CODEGRID_WINS_NAME: &str = "wolf_gnome_code_grid_wins.txt";

/// The `border-top-color` `wolf_core`'s `markdown/themes/default.css` gives
/// `body.wolf-markdown hr` in DARK, as `(r, g, b)`.
///
/// RE-TYPED here as a literal, never imported, for the reason [`PREVIEW_DARK_BG`]'s
/// doc comment gives: reading the value out of the crate that produced the pixels
/// would make the assertion a comparison with itself, and it would hold for whatever
/// the stylesheet became.
const CODEGRID_HR_COLOUR: Rgb8 = (0x3a, 0x3a, 0x3a);

/// How far below the top of the page's content the rule's hairline must sit: the web view's
/// own top edge plus the root padding `preview_width::root_style` gives the page.
///
/// FIFTEEN baseline units, DERIVED. The arithmetic, which is the whole subject of the smoke:
/// with the inline code element declaring `line-height: 0`, each block is one 24 px line
/// box, and adjacent margins collapse to the larger of the two:
///
/// | block | box | why |
/// | --- | --- | --- |
/// | p0 | 0..24 | first block, top margin reset |
/// | h3 | 72..96 | 24 + max(24, 48) |
/// | p1 | 120..144 | 96 + the h3's 24 below |
/// | h4 | 168..192 | 144 + max(24, 24) |
/// | p2 | 216..240 | 192 + 24 |
/// | h6 | 264..288 | 240 + 24 |
/// | p3 | 312..336 | 288 + 24 |
/// | rule | 360 | 336 + max(24, 24) |
///
/// With h3 to h6 at 0 below, the paragraphs start at 96, 168 and 240 and the rule lands at
/// 288. Without the code element's `line-height: 0`, each line carrying code is taller than
/// 24 px, because the line box is the UNION of the body face's box and the monospace code
/// box and the two disagree on (ascent - descent): 2 px per paragraph, measured, and an
/// unmeasured amount per heading. The rule then lands at least 8 px low.
///
/// Absolute row 519 with the content's top at 159, MEASURED. The feared "no row matches the
/// rule's colour exactly" outcome, which a fractional device-pixel landing would produce,
/// does not occur at a whole-baseline offset: MEASURED, the hairline matches 354 of 354
/// sampled columns EXACT on a single row.
const CODEGRID_HR_OFFSET_PX: usize = 360;

/// The `background-color` `wolf_core`'s `markdown/themes/default.css` gives
/// `body.wolf-markdown code` in DARK — the inline code CHIP — as `(r, g, b)`.
///
/// RE-TYPED, for the reason [`CODEGRID_HR_COLOUR`] gives.
const CODEGRID_CHIP_COLOUR: Rgb8 = (0x2b, 0x2b, 0x2b);

/// How many rows below the top of the page's content the chip band covers: the FIRST
/// paragraph's line box and nothing else.
///
/// One baseline unit, so the band ends well before the next chip-bearing block, the h3,
/// whose box begins at 72 (see [`CODEGRID_HR_OFFSET_PX`]). Measuring one block's chip
/// rather than all seven is deliberate: bands summed would let surviving chips cover for
/// one that vanished.
///
/// MEASURED: the first paragraph's chip is the run 162..182, 20 rows, inside its 24 px line
/// box. With the content's top at 159 that is offsets 3..23, entirely inside this band. The
/// other paragraphs' chips are MEASURED at 282..302, 378..398 and 474..494, each at the same
/// inset in its own line box.
const CODEGRID_CHIP_BAND_PX: usize = 24;

/// The fewest exactly-[`CODEGRID_CHIP_COLOUR`] pixels the first paragraph's band must
/// carry.
///
/// MEASURED **743** in the sampled band, at 20–50 exact pixels per row across the 20
/// rows the chip occupies. The per-row figure varies because GLYPH INK sits on top of
/// the chip: the rows through the middle of the letterforms carry the FEWEST exact
/// pixels (20–30) and the chip's first and last rows carry 38, which is the opposite of
/// what a rounded-corner explanation would predict.
///
/// The floor is set well below 743 rather than at it, per this binary's discipline of
/// picking a floor with headroom rather than the measured value: 6.2x, which is what
/// keeps the assertion from failing on a host whose glyph rasterization differs. The
/// falsifier — removing `background-color` from the code rule in `wolf-core`'s
/// `markdown/themes/default.css` — takes the count to **4**, so the discrimination is
/// not marginal at any floor in this range.
///
/// That measured 4 also settles the question the WIDE band raised, and it settles it
/// affirmatively: a band reaching further left than the rule's could in principle pick
/// up `#2b2b2b` from the EDITOR half and mask a genuine collapse, which would make this
/// assertion measure the wrong thing. With the chip's background removed the whole band
/// reads 4 stray pixels, so it carries no contamination worth a floor of 120 — the
/// argument that a wider band can only add to a floor is MEASURED here rather than
/// merely reasoned.
const CODEGRID_MIN_CHIP_PX: usize = 120;

/// How far inside the preview pane's LEFT and RIGHT edges the chip band starts.
///
/// [`BAND_X_INSET_PX`] cannot be reused, and the reason is the one
/// `LINK_SEARCH_INSET_X_PX` records: it is 80 px, while the rendered document's first
/// glyph sits 22 px right of the pane boundary, so an 80 px inset starts 58 px INSIDE
/// the content. The first paragraph's code span begins only a few characters into that
/// line, so an 80 px inset would cut the chip roughly in half and the counted floor
/// would depend on exactly where a proportional face breaks.
///
/// Ten pixels is `LINK_SEARCH_INSET_X_PX`'s value and clears the same thing: both pane
/// boundaries. What the inset buys here is that the band cannot reach the OTHER pane,
/// whose background is the editor's and is a colour this assertion takes no position on.
///
/// That the wide band adds only chip pixels, and not contamination that could mask a
/// collapse, is MEASURED rather than argued — see [`CODEGRID_MIN_CHIP_PX`], which
/// records what this band reads with the chip's background removed. The narrow
/// [`preview_theme_preview_columns`] stays the right band for the rule below, where the
/// measured 354-of-354 exact match was taken.
const CODEGRID_CHIP_INSET_X_PX: f64 = 10.0;

/// The columns the chip band samples: the whole PREVIEW half, inset by
/// [`CODEGRID_CHIP_INSET_X_PX`] on both sides.
///
/// Derived at the pinned output size: columns 776..1270.
fn codegrid_chip_columns() -> (usize, usize) {
    let (area_x, _, area_w, _) = preview_theme_pane_area();
    let output_w = exact_f64(CAGE_OUTPUT_W_EXPECTED, "CAGE_OUTPUT_W_EXPECTED");
    let x0 = pixel_index(
        area_x + area_w / 2.0 + CODEGRID_CHIP_INSET_X_PX,
        "the chip band's first column",
    );
    let x1 = pixel_index(
        output_w - CODEGRID_CHIP_INSET_X_PX,
        "the chip band's last column",
    );
    (x0, x1)
}

/// One baseline unit of `wolf-core`'s `markdown/base.css`, in px: the 24 px body
/// line-height that every vertical margin and line box in the document is a multiple of.
///
/// RE-TYPED, for the reason [`CODEGRID_HR_COLOUR`] gives.
const CODEGRID_BASELINE_PX: usize = 24;

/// `(heading, pitch px)` for each heading of [`CODEGRID_SOURCE_BODY`], in document order:
/// the distance from the top row of the chip of the paragraph above the heading to the top
/// row of the chip of the paragraph below it.
///
/// DERIVED, from the table under [`CODEGRID_HR_OFFSET_PX`]: 24 for the paragraph's line,
/// plus the collapsed margin above the heading, plus 24 for the heading's box, plus the
/// heading's 24 below. The h3 is 24 + max(24, 48) + 24 + 24 = 120; the h4 and the h6 are
/// 24 + max(24, 24) + 24 + 24 = 96. Measured chip top to chip top, the chips' common inset
/// inside their line boxes cancels out.
const CODEGRID_HEADING_PITCHES_PX: &[(&str, usize)] = &[("h3", 120), ("h4", 96), ("h6", 96)];

/// The column, in px right of the preview pane's left boundary (`area_x + area_w / 2`),
/// that divides the paragraphs' chips from the headings' chips.
///
/// DERIVED, not measured. The rendered document's first glyph sits about 22 px right of the
/// boundary. Each paragraph's code span follows one short word at 16 px, so its chip ends by
/// about 120; each heading's follows at least 20 characters of heading text at 14 to 20 px,
/// so its chip starts past about 210. 170 sits in the gap with some 50 px to either side.
///
/// Two bands rather than one are what keeps a heading's chip and the next paragraph's chip
/// from merging into one run when the heading has no space below, where the two chips are
/// only 2 or 3 rows apart.
const CODEGRID_CHIP_SPLIT_X_PX: f64 = 170.0;

/// The fewest exactly-[`CODEGRID_CHIP_COLOUR`] pixels a row of one band needs to count as
/// a chip row in [`exact_colour_runs`].
///
/// MEASURED for the first paragraph's chip: a stray reading of 4 pixels across its whole
/// band with the chip unpainted, and at least 20 on every row the chip occupies. 6 sits
/// between the two with room on both sides: 4 pixels across the whole band means no single
/// row can reach 6.
const CODEGRID_CHIP_ROW_MIN_PX: usize = 6;

/// The fewest consecutive chip rows [`exact_colour_runs`] keeps as one chip.
///
/// MEASURED: the smallest chip, the h6's at 0.875 x 14 = 12.25 px, spans 18 rows (428..446),
/// and the first paragraph's spans 20. 10 keeps both while dropping a run of a few rows
/// that antialiased heading ink could produce.
const CODEGRID_CHIP_MIN_ROWS: usize = 10;

/// The column [`CODEGRID_CHIP_SPLIT_X_PX`] names, as an absolute output index.
///
/// Derived at the pinned output size: column 936.
fn codegrid_chip_split_column() -> usize {
    let (area_x, _, area_w, _) = preview_theme_pane_area();
    pixel_index(
        area_x + area_w / 2.0 + CODEGRID_CHIP_SPLIT_X_PX,
        "the chip bands' dividing column",
    )
}

/// The BASELINE-GRID scenario: [`CODEHL_SCENARIO`] without the langtree readback.
///
/// The two are separate strings rather than one with a conditional leg, for the reason
/// this binary gives about exit codes carrying meaning: a shared body would need a branch,
/// and a branch that silently skipped its readback would leave a smoke green with one of
/// its two subjects unobserved. What they share is their SHAPE, and the shared shape is
/// documented on [`CODEHL_SCENARIO`] rather than restated here.
///
/// Exit codes, the same two and for the same reasons: `11` the app died before it
/// was ready for input, `14` the window-list readback never appeared. `12` and `13` are
/// deliberately absent — nothing here depends on a clean exit.
const CODEGRID_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100
# See CODEHL_SCENARIO for the measurement behind PREVIEW_SETTLE; this spend is a
# cold open too.
PREVIEW_SETTLE=2.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome baseline-grid smoke"

wait_file() {
    f=$1
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if [ -s "$f" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome baseline-grid smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: open the SOURCE in the window's ONE pane ------------------------
wtype -- ":e $SRC_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: drive PREVIEW IN ANOTHER PANE through the command palette --------
#
# The OTHER-PANE command and not plain Preview: Preview binds in place, so from this
# window's one pane it would leave no second pane and the bands below would have no
# right half to sample.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_OTHER_PANE_QUERY"
sleep "$SETTLE"
wtype -k Return
sleep "$PREVIEW_SETTLE"

# --- Step 3: the capture, with the preview pane live -------------------------
grim "$OUT_PNG"

# --- Step 4: NEOVIM's own answer about what its windows hold -----------------
#
# AFTER the capture: this typed command line is drawn by the cmdline strip, and the
# capture must not carry it.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$WINS')"
wtype -k Return
wait_file "$WINS" || exit 14

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// Seven rendered lines carrying inline code sit on the 24 px baseline grid, the paragraph
/// after each of an h3, an h4 and an h6 starts one baseline below the heading's box, and
/// the code chip is still painted.
///
/// ## What it proves that nothing else can
///
/// `markdown_render.rs` pins the DECLARED values — that `body.wolf-markdown code`
/// declares `line-height: 0`, and each heading level's margins — and that is the floor.
/// It cannot say what the engine did with them. This smoke measures LAID-OUT boxes out of
/// a real WebKitGTK render: the distance from the document's own top edge to a rule seven
/// code-carrying blocks below it, and the distances between the self-located chips of the
/// paragraphs and headings in between. Nothing else in either suite measures a laid-out
/// box at all.
///
/// ## Why the rule is the ruler
///
/// A locator has to be findable to one pixel row and has to span the content width, or
/// "the first row that matches" is a statement about where a glyph happened to fall.
/// `base.css` gives `hr` a 24 px box with `border: 0` and a 1 px `border-top`, and
/// `box-sizing: border-box` is inherited, so the hairline IS the box's first pixel row;
/// the theme colours it through `border-top-color`, which is a colour nothing else in
/// this document paints. MEASURED: 354 of 354 sampled columns match it EXACTLY, on a
/// single row, in every run.
///
/// ## Why the chips locate the line boxes
///
/// Every paragraph's chip sits at the same inset inside its line box, since the four share
/// a face and a size, so the distance between two paragraphs' chip tops is the distance
/// between their line boxes, and the first paragraph's line box starts at the content's
/// top. A heading's chip has an inset of its own, so only its MIDDLE row is used, and only
/// to say which 24 px line box it is in. That the middle row of a heading's chip lies
/// inside the heading's line box is MEASURED: the middles are rows 244, 340 and 437, at
/// offsets 85, 181 and 278, inside the boxes at offsets 72..96, 168..192 and 264..288. It
/// holds because of the faces' metrics: the code element's content area, at 0.875 of the
/// heading's size, sits around the heading's baseline, so its middle falls within a few rows
/// of the line box's middle.
///
/// ## The assertions have DIFFERENT falsifiers, and their ORDER is what keeps them apart
///
/// Assertions 7 and 8 are geometry and assertion 5 is paint. Losing a heading's
/// `margin-bottom` moves the heading's box into the next paragraph's baseline, which 7a
/// sees before the pitch it also shortens; losing the code element's `line-height: 0` grows
/// every code-carrying line alike, so 7a holds and 7b's pitch goes off the grid; growing a
/// margin moves a pitch or the rule by a whole baseline; removing the chip's
/// `background-color` empties the chip band and changes no geometry. Each falsifier's
/// first red is a different assertion, so the first red names its cause.
///
/// ## Assertion 5 is what stops a documented claim from rotting silently
///
/// "The zero does not collapse the chip" is the fact the whole one-line CSS change
/// rests on — an inline box's CONTENT area, which is what its background paints over,
/// comes from its font's metrics rather than from `line-height`, which governs only the
/// line box. That is engine behaviour, so it was eyeballed on one capture at one
/// WebKitGTK version, and specs/markdown-stylesheet.md#inline-code-leading asserts it as
/// measured fact. An observation asserted in a document and pinned by nothing is an
/// observation that stops being true without anybody noticing. Assertions 6 and 7 rest on
/// it too: they locate every block by its chip.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert. Readings are from actual runs at GTK 4.22.4,
/// libadwaita 1.9.3, WebKitGTK 2.54.1 and `cage` 0.3.1 on a 1280x720 output, or are
/// marked derived. Every row range is half-open, as [`BlockRun`] prints it: 162..182 is rows
/// 162 to 181. The IDs name the falsifying edits, so a run's report can cite them.
///
/// | # | assertion | kind | falsifying edit | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | the PNG is present and decodes at the pinned output size | precondition | none; do not invent one | n/a |
/// | 3 | `tabpagebuflist()`, sorted, is the preview scratch plus the source | precondition — a preview pane exists, and exactly two panes do | none; do not invent one | n/a |
/// | 4 | `web_top` is locatable, and a row matching [`CODEGRID_HR_COLOUR`] exists below it | precondition — both ends of the measured distance were found | none; do not invent one | content top 159, measured. Rule at absolute 519, measured. `web_top`, the view's own top, is 139 by derivation, 159 less `EDITOR_PADDING_PX`, not re-measured |
/// | 5 | the first paragraph's band carries at least [`CODEGRID_MIN_CHIP_PX`] exactly-[`CODEGRID_CHIP_COLOUR`] pixels | **discriminator** — `line-height: 0` removed the line box's union term and NOT the chip's background | G1f: remove `background-color` from the dark `body.wolf-markdown code` rule in `wolf-core`'s `markdown/themes/default.css`. ISOLATED, and that is the reason it exists: removing a background changes no geometry, so assertion 8 and the declared-value tests stay green. It comes before 6, which would otherwise redden first on a chipless capture. | green: 743 in the band, at 20–50 exact pixels per row over the chip's 20 rows. Red (G1f): **4**, which is also what shows the wide band carries no contamination from the editor half. |
/// | 6 | [`exact_colour_runs`] finds exactly 4 chips left of [`CODEGRID_CHIP_SPLIT_X_PX`] and 3 right of it, between the content's top and the rule | precondition — every block's chip was located, and the bands split paragraphs from headings | none; do not invent one | measured: paragraph chips at absolute 162..182, 282..302, 378..398 and 474..494; heading chips at 232..256, 329..352 and 428..446 |
/// | 7a | the middle row of each heading's chip lies in the baseline that ends one baseline above the next paragraph's line box | **discriminator** — each heading is followed by one baseline | G1a, G1b, G1c: `margin-bottom: 24px;` → `margin-bottom: 0;` in `body.wolf-markdown h3`, `h4` or `h6` of `markdown/base.css`. Each reddens its own heading's 7a first, and every heading before it passes: the next paragraph moves up 24 onto that heading's box, while each earlier box keeps its baseline below. NOT isolated: `headings_declare_their_spec_margins` in `markdown_render.rs` reddens too | measured: middles at offsets 85, 181 and 278, inside the windows at 72..96, 168..192 and 264..288 |
/// | 7b | the chip-top distance across each heading is its [`CODEGRID_HEADING_PITCHES_PX`] entry | **discriminator** — every code-carrying line is one baseline, and every collapsed margin is its declared size | G1d: delete `line-height: 0;` from `body.wolf-markdown code` in `markdown/base.css`; the h3's pitch goes off the grid while its 7a holds, since every paragraph's chip moves with its line box. G1g: `margin-top: 24px;` → `margin-top: 48px;` in `body.wolf-markdown h4`; the h4's pitch reads 120, while its 7a holds, since the heading and the paragraph after it both move 24. Neither is isolated from `markdown_render.rs`, whose declared-value tests redden too, and the PAIRING is the diagnosis — a red smoke over a GREEN declared-value test says the engine stopped honouring the declaration | measured: 120, 96, 96. G1d reads 122 for the h3, and G1g reads 120 for the h4 |
/// | 8 | `hr_row - (web_top + EDITOR_PADDING_PX)` is exactly [`CODEGRID_HR_OFFSET_PX`] | **discriminator** — the whole document stacks to fifteen baselines | G1e: `margin: 24px 0 0;` → `margin: 48px 0 0;` in `body.wolf-markdown hr`; it reads 384, and every pitch is unaffected | measured: 360. G1e reads 384 |
#[test]
fn preview_inline_code_keeps_the_baseline_grid_under_cage() {
    const SMOKE: &str = "preview_inline_code_keeps_the_baseline_grid_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    let cwd = make_temp_cwd();
    write_relaunch_fixtures(SMOKE, &cwd, &[(CODEGRID_SOURCE_FILE, CODEGRID_SOURCE_BODY)]);

    // Its own config at a forced dark appearance: both asserted colours are the dark palette's.
    let config = smoke_config_dir_at(
        &cwd,
        CODEGRID_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_DARK,
        None,
    );

    let out_png = runtime_dir.join(CODEGRID_PNG_NAME);
    let wins = runtime_dir.join(CODEGRID_WINS_NAME);
    for path in [&out_png, &wins] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(CODEGRID_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("WOLF_CONFIG_DIR", &config)
        .env("SRC_FILE", CODEGRID_SOURCE_FILE)
        .env("PREVIEW_OTHER_PANE_QUERY", PREVIEW_OTHER_PANE_QUERY)
        .env("OUT_PNG", &out_png)
        .env("WINS", &wins);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless baseline-grid scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage baseline-grid scenario exited unsuccessfully ({status}). Exit 11: the \
         app died before it was ready for input. Exit 14: the window-list readback at {} never \
         appeared. Codes 12 and 13 do not exist here on purpose: nothing in this smoke depends on \
         a clean exit, so the scenario ends with a best-effort kill. Any OTHER status is `grim` or \
         `wtype` failing under `set -eu`.",
        wins.display()
    );

    // --- Assertion 2 (precondition): the capture decodes at the pinned size ---
    assert_png_present(SMOKE, &out_png);
    let capture = decode_capture(&out_png, SMOKE, "the baseline-grid capture");

    // --- Assertion 3 (precondition): a preview pane exists --------------------
    let want_windows = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{CODEGRID_SOURCE_FILE}"),
            CODEGRID_SOURCE_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_windows = read_window_buffer_names(&wins, SMOKE);
    assert_eq!(
        got_windows, want_windows,
        "{SMOKE}: after the Preview leg the captive Neovim's windows hold {got_windows:?}, not \
         {want_windows:?} (both sorted). A SINGLE entry naming {CODEGRID_SOURCE_FILE:?} means no \
         preview was bound at all; a SINGLE entry naming the preview scratch means the IN-PLACE \
         Preview ran rather than the other-pane one, so there is no right half for the sampled \
         columns to be inside. Every rectangle below assumes exactly two side-by-side panes."
    );

    let preview_columns = preview_theme_preview_columns();
    let (preview_x0, preview_x1) = preview_columns;
    let scan_rows = preview_theme_scan_rows();
    let (_, band_y1) = preview_theme_band_rows();

    // --- Assertion 4a (precondition): the web view's own top edge -------------
    let web_top = capture
        .first_uniform_row(preview_columns, scan_rows, PREVIEW_DARK_BG)
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: no row between {} and {} is entirely PREVIEW_DARK_BG = {PREVIEW_DARK_BG:?} \
                 across columns {preview_x0}..{preview_x1}, so the web view's top edge could not be \
                 located and the measured distance has no origin. Either the document is not \
                 rendering, or its first block is not a paragraph — this fixture's is, and the rule \
                 it ends with is the only full-width block in it. Capture kept at {}.",
                scan_rows.0,
                scan_rows.1,
                out_png.display()
            )
        });

    // Assertion 4b (precondition): searched from below `web_top`, so the top edge is never the rule.
    let rule_scan = SampleRect {
        x0: preview_x0,
        x1: preview_x1,
        y0: web_top + 1,
        y1: band_y1,
    };
    assert!(
        rule_scan.y0 < rule_scan.y1 && rule_scan.y1 <= capture.height,
        "{SMOKE}: the rule-scan rectangle {rule_scan:?} is degenerate or runs off the bottom of \
         the {}x{} capture, so the located web view top ({web_top}) is below \
         `preview_theme_band_rows`'s last row.",
        capture.width,
        capture.height
    );
    let hr_row = first_majority_row(&capture, rule_scan, CODEGRID_HR_COLOUR).unwrap_or_else(|| {
        panic!(
            "{SMOKE}: no row between {} and {} has CODEGRID_HR_COLOUR = {CODEGRID_HR_COLOUR:?} as a \
             strict majority of columns {preview_x0}..{preview_x1}, so the rule's hairline could \
             not be located and assertion 8 has nothing to measure to. The measured match is 354 of \
             354 columns EXACT, so a near miss is not the expected failure here. Causes, in order: \
             the fixture's trailing `---` did not render as a thematic break (a missing blank line \
             above it makes CommonMark read it as a setext underline instead); the theme stopped \
             colouring `hr` through `border-top-color` — a `background-color` there would fill the \
             whole 24 px box rather than draw a hairline, which would still be found but would put \
             the located row at the box's own top edge anyway; or the rule was pushed below row {} \
             by a document taller than this fixture. Capture kept at {}.",
            rule_scan.y0,
            rule_scan.y1,
            rule_scan.y1,
            out_png.display()
        )
    });

    let content_top = web_top
        + pixel_index(
            wolf_gnome::editor_inset::EDITOR_PADDING_PX,
            "the page's root padding",
        );

    // Assertion 5 (discriminator): the chip's background changes no geometry, so only this notices it.
    let chip_columns = codegrid_chip_columns();
    let chip = SampleRect {
        x0: chip_columns.0,
        x1: chip_columns.1,
        y0: content_top,
        y1: content_top + CODEGRID_CHIP_BAND_PX,
    };
    assert!(
        chip.x0 < chip.x1
            && chip.y0 < chip.y1
            && chip.x1 <= capture.width
            && chip.y1 <= capture.height,
        "{SMOKE}: the chip band {chip:?} is degenerate or falls outside the {}x{} capture. It is \
         derived from CAGE_OUTPUT_*_EXPECTED, OVERLAY_ORIGIN_*_EXPECTED and \
         CODEGRID_CHIP_INSET_X_PX, so this means two of those have crossed.",
        capture.width,
        capture.height
    );
    let chip_hits = capture.count_exactly(chip, CODEGRID_CHIP_COLOUR);
    let rule_row = SampleRect {
        y0: hr_row,
        y1: hr_row + 1,
        ..rule_scan
    };
    let rule_hits = capture.count_exactly(rule_row, CODEGRID_HR_COLOUR);
    let chip_rows: Vec<usize> = (chip.y0..chip.y1)
        .map(|y| {
            let row = SampleRect {
                y0: y,
                y1: y + 1,
                ..chip
            };
            capture.count_exactly(row, CODEGRID_CHIP_COLOUR)
        })
        .collect();
    eprintln!(
        "{SMOKE} readings: content top {content_top}; rule row {hr_row}, exact on {rule_hits} of \
         {} columns; first paragraph's band {chip_hits} exact chip pixels, per row from row \
         {content_top}: {chip_rows:?}",
        rule_scan.x1 - rule_scan.x0
    );
    assert!(
        chip_hits >= CODEGRID_MIN_CHIP_PX,
        "{SMOKE}: the FIRST paragraph's line box (band {chip:?}, {} pixels) carries {chip_hits} \
         pixels that are exactly CODEGRID_CHIP_COLOUR = {CODEGRID_CHIP_COLOUR:?}, below the \
         {CODEGRID_MIN_CHIP_PX} floor. The measured value is 743, at 20–50 per row across the 20 \
         rows the chip occupies.\n\
         A count in SINGLE DIGITS — 4 measured with the chip's background removed — means no chip \
         is painted at all. The NAMED cause: `body.wolf-markdown code` in `wolf-core`'s \
         `markdown/themes/default.css` lost its `background-color`. The rule offset of assertion \
         8 is unaffected in that case — removing a background changes no geometry — and this \
         assertion is the only one in either suite that can see it. The other cause is the engine \
         sizing the inline box's CONTENT area from `line-height: 0`, which is the engine \
         behaviour this assertion exists to pin: an inline box's content area — what its \
         background paints over — comes from the font's metrics, not from `line-height`; if that \
         ever stops holding, the fix is a stylesheet change and NOT a lower floor.\n\
         A count well above single digits but under the floor means the chip is painted but \
         smaller than it was: the code element's `padding` or its `font-size` shrank. Capture \
         kept at {}.",
        chip.pixels(),
        out_png.display()
    );

    // --- Assertion 6 (precondition): every block's chip, located ----------------
    let split_x = codegrid_chip_split_column();
    let paragraph_band = SampleRect {
        x0: chip_columns.0,
        x1: split_x,
        y0: content_top,
        y1: hr_row,
    };
    let heading_band = SampleRect {
        x0: split_x,
        x1: chip_columns.1,
        y0: content_top,
        y1: hr_row,
    };
    assert!(
        paragraph_band.x0 < paragraph_band.x1
            && heading_band.x0 < heading_band.x1
            && content_top < hr_row,
        "{SMOKE}: the chip bands {paragraph_band:?} and {heading_band:?} are degenerate. Either \
         CODEGRID_CHIP_SPLIT_X_PX has left the chip columns {chip_columns:?}, or the rule's \
         hairline (row {hr_row}) was found above the content's top ({content_top}), which means \
         the located row is not the fixture's rule."
    );
    let paragraph_runs = exact_colour_runs(
        &capture,
        paragraph_band,
        CODEGRID_CHIP_COLOUR,
        CODEGRID_CHIP_ROW_MIN_PX,
        CODEGRID_CHIP_MIN_ROWS,
    );
    let heading_runs = exact_colour_runs(
        &capture,
        heading_band,
        CODEGRID_CHIP_COLOUR,
        CODEGRID_CHIP_ROW_MIN_PX,
        CODEGRID_CHIP_MIN_ROWS,
    );
    eprintln!(
        "{SMOKE} readings: paragraph chips {paragraph_runs:?}; heading chips {heading_runs:?}"
    );
    assert!(
        paragraph_runs.len() == CODEGRID_HEADING_PITCHES_PX.len() + 1
            && heading_runs.len() == CODEGRID_HEADING_PITCHES_PX.len(),
        "{SMOKE}: found {} chip runs left of column {split_x} (paragraph band {paragraph_band:?}): \
         {paragraph_runs:?}, and {} right of it (heading band {heading_band:?}): {heading_runs:?}. \
         The fixture has {} paragraphs, each with its chip left of the split, and {} headings, \
         each with its chip right of it, so the bands must hold exactly those counts for the \
         assertions below to pair each heading with the paragraphs around it.\n\
         Too FEW in one band, with too many in the other, means a chip straddles or crossed \
         CODEGRID_CHIP_SPLIT_X_PX: the fixture's horizontal layout differs from that constant's \
         derivation. Too few in both means chips merged vertically or fell under \
         CODEGRID_CHIP_ROW_MIN_PX or CODEGRID_CHIP_MIN_ROWS. A block that wrapped to two lines \
         reads as an extra run. None of these is a margin failure, and none is fixed by \
         editing a margin. Capture kept at {}.",
        paragraph_runs.len(),
        heading_runs.len(),
        CODEGRID_HEADING_PITCHES_PX.len() + 1,
        CODEGRID_HEADING_PITCHES_PX.len(),
        out_png.display()
    );

    // --- Assertion 7 (discriminator): one baseline below each heading ----------
    let origin = paragraph_runs[0].y0;
    for (i, (&(level, pitch), heading)) in CODEGRID_HEADING_PITCHES_PX
        .iter()
        .zip(&heading_runs)
        .enumerate()
    {
        let above = paragraph_runs[i];
        let below = paragraph_runs[i + 1];
        // The paragraph's line-box top: every paragraph's chip sits at the first one's inset.
        let next_top = content_top + (below.y0 - origin);
        let middle = (heading.y0 + heading.y1) / 2;

        // 7a: the heading's line box is the baseline ending one baseline above that paragraph.
        assert!(
            middle + 2 * CODEGRID_BASELINE_PX >= next_top
                && middle + CODEGRID_BASELINE_PX < next_top,
            "{SMOKE}: the {level}'s chip (rows {heading:?}) has its middle at row {middle}, outside \
             the baseline that ends one baseline above the next paragraph's line box, rows {}..{} \
             (that paragraph's line box starts at row {next_top}: content top {content_top} plus \
             its chip's distance {} below the first paragraph's chip at row {origin}).\n\
             Middle at or below row {} is the NAMED failure: the {level} has no space below, so \
             its box sits directly on the paragraph after it. Its `margin-bottom` in \
             `wolf-core`'s `markdown/base.css` is 0 rather than one baseline; \
             `headings_declare_their_spec_margins` in `markdown_render.rs` says whether the \
             declaration or the engine moved, and specs/markdown-stylesheet.md#baseline-grid \
             gives every heading 24px below.\n\
             Middle above row {} means more than one baseline sits between the {level} and the \
             paragraph after it: the heading's margin-bottom grew, or the paragraph gained a top \
             margin that outcollapses it. Capture kept at {}.",
            next_top - 2 * CODEGRID_BASELINE_PX,
            next_top - CODEGRID_BASELINE_PX,
            below.y0 - origin,
            next_top - CODEGRID_BASELINE_PX,
            next_top - 2 * CODEGRID_BASELINE_PX,
            out_png.display()
        );

        // 7b: chip top to chip top across the heading, a whole number of declared baselines.
        let got = below.y0 - above.y0;
        assert_eq!(
            got,
            pitch,
            "{SMOKE}: the paragraphs either side of the {level} have their chips' top rows {got} \
             px apart (rows {} and {}), not {pitch}: 24 for the paragraph's line, the collapsed \
             margin above the {level}, 24 for its box and its 24 below, as \
             CODEGRID_HEADING_PITCHES_PX derives. Its 7a above passed, so the {level}'s box is one \
             baseline above the paragraph after it.\n\
             A reading of {} is a top margin that collapsed SMALLER than declared, and {} one \
             that GREW, such as the {level}'s `margin-top` in `wolf-core`'s `markdown/base.css` \
             one baseline above its declared value; `headings_declare_their_spec_margins` names \
             which level's declaration moved.\n\
             A reading that is not a multiple of 24 is a LINE BOX that grew: \
             `body.wolf-markdown code` in `markdown/base.css` lost its `line-height: 0`, so each \
             line carrying code is the union of the body face's box and the monospace code box, \
             which disagree on (ascent - descent). Read this beside `markdown_render.rs`'s \
             `inline_code_declares_zero_leading_so_it_cannot_grow_a_line_box`: RED there too \
             means the declaration went missing, which is the ordinary case; GREEN there means \
             the declaration is present and the ENGINE stopped honouring it, a WebKitGTK change \
             and a completely different investigation. Do not adjust the pitch to make a run \
             pass. Capture kept at {}.",
            above.y0,
            below.y0,
            pitch - CODEGRID_BASELINE_PX,
            pitch + CODEGRID_BASELINE_PX,
            out_png.display()
        );
    }

    // --- Assertion 8 (discriminator): the whole document, fifteen baselines ----
    let offset = hr_row - content_top;
    assert_eq!(
        offset,
        CODEGRID_HR_OFFSET_PX,
        "{SMOKE}: the rule's hairline sits {offset} px below the content's top (absolute row \
         {hr_row}, content top {content_top}: the web view's top {web_top} plus the page's root \
         padding), not {CODEGRID_HR_OFFSET_PX}. Every pitch across a heading passed above, so \
         the paragraphs and headings between the first chip and the last stack as derived.\n\
         The fixture is four one-line paragraphs and three one-line headings, every one carrying \
         one inline code element, then the rule: p3's line box ends 336 px down, and its 24 px \
         bottom margin collapses with the rule's 24 px top margin to put the rule's box at 336 + \
         24 = {CODEGRID_HR_OFFSET_PX}. A reading of 384 is the rule's own top margin at 48 \
         rather than 24 (`body.wolf-markdown hr` in `wolf-core`'s `markdown/base.css`), or the \
         last paragraph's bottom margin grown to 48. A reading larger by a constant with every \
         pitch intact can also mean the root style's padding-top is not EDITOR_PADDING_PX, \
         which moves the first paragraph's chip and the rule together: that chip's top row, \
         {origin} here, is MEASURED 3 px below the content's top when the padding holds.\n\
         Any other offset that is not a multiple of 24 is a third state: re-derive the \
         arithmetic in CODEGRID_HR_OFFSET_PX before touching this constant, and do not adjust it \
         to make a run pass. Capture kept at {}.",
        out_png.display()
    );
}

// A picker over a live preview: the box is located from pixels, its colour read from the capture.
// notes/crates/wolf-gnome/tests/smoke_headless/preview_render.rs.md#picker-over-preview-geometry

/// Picker-over-preview smoke: the file opened in the SPLIT pane and then replaced by
/// its own preview.
///
/// A fixture of its own rather than `PREVIEW_SOURCE_FILE`, deliberately. The
/// markdown-preview smoke chooses its heading for `tesseract`'s sake; this smoke's
/// control half counts that heading's WHITE PIXELS. The two answer to different
/// constraints, and sharing one file would let a change made for OCR legibility move
/// a pixel floor a thousand lines away with nothing naming the connection.
///
/// Registered in `RELAUNCH_FIXTURE_NAMES` — not because a sidebar row could satisfy
/// an assertion here (this smoke asserts no OCR term at all) but because that table's
/// generic loop is what stops a later rename turning this name into page text that
/// breaks somebody ELSE's smoke. `LINK_SOURCE_FILE` is registered for exactly that
/// reason.
pub(crate) const PICKOVER_SOURCE_FILE: &str = "pvovsrc.md";

/// Heading written into [`PICKOVER_SOURCE_FILE`], and the ONLY source of white pixels
/// anywhere in this smoke's sampled rectangle.
///
/// An `h1`, and that is load-bearing rather than stylistic: `wolf_core`'s
/// `markdown/themes/default.css` gives `h1` the pure white [`PICKOVER_HEADING_INK`] in
/// dark while `body` text is `#d4d4d4`, so a count of exactly white pixels is a count
/// of rendered heading glyphs and of nothing else on the page.
///
/// ELEVEN capital letters, and the count is part of the measurement: the MEASURED 1,976
/// white pixels inside the sampled rectangle is a reading of that many glyphs at the
/// stylesheet's 32 px. A shorter heading puts fewer pixels there; a longer one runs past
/// the rectangle's right edge and adds none.
pub(crate) const PICKOVER_SOURCE_HEADING: &str = "# ZZOVERROMEO";

/// The paragraph under [`PICKOVER_SOURCE_HEADING`].
///
/// It is `#d4d4d4` body text, so it contributes no white pixel and cannot help the
/// control's ink floor. What it does is keep the rendered document from being a lone
/// heading on an otherwise blank page, and it is deliberately SHORT so that every row
/// of the sampled rectangle below it is pure background — which is what the 94.05 %
/// dark reading rests on. Lengthen it and that fraction falls; the fix would be a
/// shorter paragraph, never a lower floor.
pub(crate) const PICKOVER_SOURCE_PARAGRAPH: &str = "victor whiskey juliett mike, papa quebec.";

/// Picker-over-preview smoke: the file left in the STARTING pane, which the editor
/// grid draws throughout both captures.
///
/// Nothing asserts on it. It exists so the window holds TWO panes when Preview is
/// driven — which is what puts the preview in the right half, see the smoke's doc
/// comment for that chain — and so [`pickover_fill_reference_band`] has an EDITOR pane
/// under it rather than a second preview.
pub(crate) const PICKOVER_OTHER_FILE: &str = "pvovoth.md";

/// Heading written into [`PICKOVER_OTHER_FILE`]. Drawn by the editor grid, which no
/// rectangle in this smoke samples, so it is asserted on in neither capture.
pub(crate) const PICKOVER_OTHER_HEADING: &str = "# ZZOVERUNIFORM";

/// Typed into the file picker's search entry once Ctrl+Shift+O has opened it.
///
/// A prefix of [`PICKOVER_SOURCE_FILE`] and of nothing else in the temp cwd — not the
/// other fixture, not any [`SIDEBAR_FILLER_FILES`] entry — so the ranked list narrows
/// to exactly ONE row. That is the state the 100 % fill reading was taken in: one row
/// leaves the rest of the box's body empty, while a query matching several files would
/// push row text down it and could put glyphs inside the sampled rectangle, lowering
/// the fill fraction on a perfectly good render.
///
/// **No Enter is ever pressed.** Both `picker_*_under_cage` smokes activate a row and
/// screenshot what Neovim then loaded; this one photographs the picker itself, which
/// is what makes it the first smoke in this binary to capture a transient overlay while
/// it is still up.
const PICKOVER_QUERY: &str = "pvovsrc";

/// File the captive Neovim writes its window/buffer list into, BEFORE either capture.
///
/// Under `XDG_RUNTIME_DIR` rather than in the temp cwd, so the app's own `FileSearch`
/// never lists it: a new sidebar row and a new picker candidate appearing between the
/// two captures would change the page both are read from, and the picker's row count
/// is exactly what [`PICKOVER_QUERY`] is chosen to control.
///
/// Registered in `RELAUNCH_FIXTURE_NAMES` all the same, for the reason the terminal
/// drawer's runtime-dir readbacks are: its ABSOLUTE path is echoed in the typed
/// `:call writefile(...)` command line, which the cmdline strip draws in the very
/// capture below. That strip sits outside every rectangle this smoke reads, so the
/// registration is a tripwire rather than a live hazard.
pub(crate) const PICKOVER_WINS_NAME: &str = "wolf_gnome_pickover_wins.txt";

/// The CONTROL capture — the preview live, no overlay open.
///
/// Under `XDG_RUNTIME_DIR` for [`PICKOVER_WINS_NAME`]'s reason. Retained after the
/// run: every threshold below is a reading of these two files, so a failure is
/// diagnosed by looking at them rather than by re-running.
const PICKOVER_CONTROL_PNG_NAME: &str = "wolf_gnome_pickover_control.png";

/// The SAMPLE capture — the same window with the file picker open over it. See
/// [`PICKOVER_CONTROL_PNG_NAME`].
const PICKOVER_SAMPLE_PNG_NAME: &str = "wolf_gnome_pickover_sample.png";

/// Name suffix of this smoke's config directory sibling.
const PICKOVER_SUFFIX: &str = "-pickover";

/// The `color` `wolf_core`'s `markdown/themes/default.css` gives
/// `body.wolf-markdown h1` in DARK, as `(r, g, b)`.
///
/// RE-TYPED here as a literal, never imported, for the reason [`PREVIEW_DARK_BG`]'s
/// doc comment gives at length: reading the value out of the crate that produced the
/// pixels would make the assertion a comparison with itself, and would hold for
/// whatever the stylesheet became.
///
/// It is pure white, and nothing else these captures can contain is: the document's
/// background is [`PREVIEW_DARK_BG`], its body text `#d4d4d4`, the editor pane's
/// background `(20, 22, 27)` and the picker's own surface the colour
/// [`pickover_fill_reference_band`] reads back. MEASURED inside the rectangle assertions
/// 7 and 8 sample: 1,976 of these pixels on the control capture and 0 on the sample.
pub(crate) const PICKOVER_HEADING_INK: Rgb8 = (0xff, 0xff, 0xff);

/// The colour the overlay box reads when `overlay_style::apply` reaches nothing: the
/// `GtkListBox`'s own `@view_bg_color`, as `(r, g, b)`.
///
/// Assertion 4 reads the box's fill EMPIRICALLY and says nothing about which colour it
/// is, which is what keeps a theme literal out of this smoke — and is also what makes it
/// satisfiable by ANY opaque surface. The overlay stylesheet is what turns the list's own
/// background transparent so the box's `@popover_bg_color` surface shows through; with
/// `apply`'s body emptied the band still reads one flat opaque colour, still absent from
/// the control, and every other half of assertion 4 holds. This inequality is the half
/// that does not, and it is the only assertion in this binary that can tell "the overlay
/// stylesheet resolved" from "some widget under it painted something opaque".
///
/// MEASURED `(29, 29, 32)` with `overlay_style::apply`'s body emptied: uniform across all
/// 32,000 pixels of [`pickover_fill_reference_band`] and absent from the control, so that
/// build reaches and passes assertion 8.
///
/// A `!=` against the UNSTYLED value rather than an `==` against the styled one,
/// deliberately, and the cost is stated rather than hidden. Both values belong to
/// libadwaita rather than to this crate, so an `==` would redden on any libadwaita that
/// re-tints `@popover_bg_color` — a failure that says nothing about this front end. The
/// price of the `!=` is that a libadwaita which re-tints `@view_bg_color` degrades this
/// guard to a no-op SILENTLY. That is the accepted weakness of this line, not an
/// oversight: it fails safe rather than loud, and the thing to check when this smoke
/// stops discriminating the stylesheet is whether this literal is still what an unstyled
/// box reads.
const PICKOVER_UNSTYLED_FILL: Rgb8 = (0x1d, 0x1d, 0x20);

/// The least of the SAMPLED rectangle that must be [`PREVIEW_DARK_BG`] in the CONTROL
/// capture.
///
/// MEASURED 0.9405 across its 79,800 pixels with R's left and top 20 px in, the remaining
/// 5.95 % being the heading's and the paragraph's ink. This floor leaves fourteen points of
/// headroom, which is more than a fixture whose paragraph grew by a line could
/// consume and far above anything a rectangle aimed at the wrong region could reach:
/// the editor pane's background reads `(20, 22, 27)` and the overlay box's own fill
/// `(54, 54, 58)`, so neither contributes a single pixel to this count. Both are read
/// off the retained captures rather than named anywhere this smoke asserts — see
/// [`pickover_fill_reference_band`] for why the fill is empirical.
pub(crate) const PICKOVER_CONTROL_MIN_DARK_FRACTION: f64 = 0.80;

/// The fewest [`PICKOVER_HEADING_INK`] pixels the SAMPLED rectangle must carry in the
/// CONTROL capture.
///
/// An ACCEPTANCE CRITERION rather than a tuned number. What it separates is "the web
/// view rendered the document" from "the web view is there and blank", and a blank
/// view reads 0 rather than a smaller positive number — the same shape the code-block
/// smoke's control has.
///
/// MEASURED 1,976 for [`PICKOVER_SOURCE_HEADING`]'s eleven-letter `h1` at the
/// stylesheet's 32 px, so this floor leaves 3.95x. It is the same 1,976 the un-inset
/// rectangle carries: the heading is the document's first block and the corner
/// [`PICKOVER_CORNER_INSET_PX`] trims is at the bottom-right, so the two are nowhere
/// near each other and the inset costs this count nothing.
pub(crate) const PICKOVER_CONTROL_MIN_INK_PX: usize = 500;

/// The least of the SAMPLED rectangle that must be the picker box's own fill colour in
/// the SAMPLE capture.
///
/// The rectangle assertions 7 and 8 read is R inset by [`PICKOVER_CORNER_INSET_PX`] on
/// the right and the bottom, which puts the box's rounded corner outside it altogether,
/// and started below the picker's selected row. MEASURED **exactly 1.0000000000** — all
/// 79,800 of its pixels, with R's left and top 20 px in, are the one colour
/// `(54, 54, 58)` and the histogram of everything else is EMPTY.
///
/// The floor is not raised to that measurement, and the gap is reserved rather than
/// leftover: the slack is for a list row that renders taller than it does today, which
/// would push row text down the box and into the sampled rectangle. It is NOT a tolerance
/// for partial occlusion — that is what the two exact-zero assertions beside this one
/// answer, and neither of them may be softened into a fraction to make a run pass. Nor
/// may this floor be lowered: a fraction down in the nineties is the shadow band or the
/// preview itself inside the sampled rectangle, which is a finding about where R's edges
/// were located rather than a threshold to retune.
const PICKOVER_SAMPLE_MIN_FILL_FRACTION: f64 = 0.98;

/// The fewest columns the picker box's visible surface must span INSIDE the web view.
///
/// 298 by derivation (the web view's left edge at its pane's, 767, the box's right edge one
/// past 1064), not re-measured: 278 was measured with that edge at 787, the pane's padding
/// in. So this floor leaves 1.4x or more. It is a discriminator and not a sanity check: a box that
/// is on screen but not over the preview reads 0 here, because the only thing that
/// bounds its visible right edge is then the web view's own left edge.
///
/// It measures R's TRUE width, before the [`PICKOVER_CORNER_INSET_PX`] trim assertions 7
/// and 8 sample through — this is the floor that says the box really does cover the
/// preview, so it may not be read on a rectangle the test has already shrunk. Passing it
/// is also what makes that trim safe: a span of at least 200 is over sixteen times the
/// inset, so the subtraction that takes it cannot go negative.
///
/// The P1b red capture carries exactly that 0. Read off the retained red PNG:
/// `web_top` 159, `web_left` 787 and `box_right` 787 (139, 767 and 767 by derivation with
/// the view reaching its pane's edge and its strip), so R is degenerate at zero area
/// and this width reads 0 against the floor. That run reddened assertion 4 first and
/// never reached here, which is why the 0 is quoted from the PNG rather than from a
/// failure message — and why this floor is a discriminator rather than decoration.
///
/// The fill-side scan that locates `box_right` (see [`PICKOVER_EDGE_MIN_NON_FILL_PX`])
/// preserves that 0, and the reason is worth writing down because it is what stops the
/// scan being a self-fulfilling one. The fill colour a build with no visible box reads
/// back is the EDITOR pane's own `(20, 22, 27)`, and the web view's first column is not
/// that colour on any of the rows sampled — so the scan leaves the fill at its own first
/// column, `box_right` collapses to `web_left`, and this width is 0.
const PICKOVER_MIN_OVERLAP_W_PX: usize = 200;

/// The fewest rows the picker box's visible surface must span inside the web view.
///
/// 332 by derivation (the web view's top edge under the strip, at 139, the box's bottom edge
/// at 471), not re-measured: 312 was measured with that edge at 159. So this floor leaves
/// 1.6x or more. See [`PICKOVER_MIN_OVERLAP_W_PX`]; the two are separate
/// constants so a failure names the axis that collapsed, and its last paragraph applies
/// here unchanged — this is R's true height, and clearing the floor is what makes the
/// vertical half of the [`PICKOVER_CORNER_INSET_PX`] subtraction safe.
///
/// A box that got SHORTER is not this floor's to catch, and the smoke does not rely on it
/// for that. The bottom edge is scanned from the reference band's last row downward, so a
/// box ending above that row reddens assertion 4 — the band would straddle the box's own
/// boundary and stop being one flat colour — long before a row scan could report it.
const PICKOVER_MIN_OVERLAP_H_PX: usize = 200;

/// How many pixels of a scanned line must NOT be the box's fill colour before that line
/// counts as PAST the box's opaque surface.
///
/// The two edges read in the sample capture are located from the fill side rather than
/// from the preview's background arriving, and this threshold is the second half of that
/// formulation. Two properties of the box defeat the simpler alternatives:
///
/// * The box carries an outset shadow, drawn OUTSIDE its border box and unclipped. The
///   shadow band is near [`PREVIEW_DARK_BG`] without being exactly it, so a scan for the
///   first line that is exactly the preview's background walks through the band and
///   reports an edge four columns and five rows past the opaque surface — dragging some
///   2,600 pixels the box never painted into R.
/// * The box's corners are rounded, so the rows just above its bottom edge are mostly
///   fill and partly not. A scan that stopped at the first line carrying ANY not-fill
///   pixel would stop on its own first row, because the row scan starts at the reference
///   band's last row and the arc already reaches it — and an edge that reports the scan's
///   own starting row whatever the box does measures nothing at all.
///
/// MEASURED separation off the retained sample capture at the pinned output size. On the
/// row axis: the arc's rows, 460 through 470, carry 1 to 11 not-fill pixels each across
/// R's 278 columns, while the first row below the box carries all 278. On the column axis
/// it is wider still: every column of the surface carries 0 not-fill pixels across the
/// reference band's 160 rows, and the first column past the surface carries all 160. Any
/// value from 12 to 159 separates both readings; this one sits about 3x above the arc's
/// worst row and 5x below the narrower of the two full lines.
///
/// The arc this threshold scans PAST is the same one [`PICKOVER_CORNER_INSET_PX`] then
/// removes from the sampled rectangle. The two are not redundant: this one is what lets
/// the edges be located at all, and that one is what lets the readings taken inside them
/// be exact.
const PICKOVER_EDGE_MIN_NON_FILL_PX: usize = 32;

/// How far R is pulled IN from the box's right and bottom edges before assertions 7 and
/// 8 sample it, in output pixels.
///
/// It is the `border-radius` declared for the overlay box in
/// `overlay_style::OVERLAY_CSS_DATA`, re-typed as a literal here — that module is private
/// to the crate, so an integration test cannot import it, and the same re-typing rule
/// [`PREVIEW_DARK_BG`] states would apply anyway. The coupling to a stylesheet number is
/// the point of the constant rather than an accident of it: a radius change that reddened
/// this smoke is a REAL signal, and the number to re-derive it from is named in one place.
///
/// ## Why any inset at all
///
/// R's right and bottom edges name the box's OPAQUE surface, and the box has ROUNDED
/// corners, so a rectangle spanning its full straight extent is genuinely not occluded at
/// the bottom-right one. That is geometry rather than noise: the preview shows through
/// there, and no compositing change can stop it. Insetting by the radius is what makes
/// assertion 8's two exact zeros a statement about OCCLUSION rather than about how the
/// shadow happens to tint the arc.
///
/// ## Why the radius and not one pixel
///
/// MEASURED off the retained sample capture: the arc's intrusion into R has bbox
/// `x 1054..1064, y 460..470` — 38 pixels inside a corner square of side 11 — and exactly
/// ONE of them, `(1064, 470)`, reads exactly [`PREVIEW_DARK_BG`]. A one-pixel inset
/// therefore passes, and passes for an accidental reason: the arc's other 37 exposed
/// pixels escape the equality only because the shadow tints them to 24/25/26, so a change
/// to the shadow's alpha moves them onto or off the preview's exact value and reddens
/// assertion 8 for a cause nobody could attribute to compositing. This
/// inset clears the whole 11 px reach with one pixel of margin per axis, which puts the
/// arc outside the sampled rectangle entirely.
///
/// ## Where it may NOT be applied
///
/// Only after both halves of assertion 6 have run. Under the P1b paint-order falsifier
/// `box_right == web_left`, so an inset taken first yields `x1 < x0` and panics on the
/// underflow inside [`SampleRect::pixels`] instead of reddening assertion 6 with its own
/// diagnostic. See [`PICKOVER_MIN_OVERLAP_W_PX`] for the span that makes the subtraction
/// unconditionally safe once that assertion has passed.
const PICKOVER_CORNER_INSET_PX: usize = 12;

/// The colour libadwaita paints behind the picker's SELECTED list row, as `(r, g, b)`.
///
/// Assertion 9 finds the selected row by this colour and measures how many rows tall the
/// run of it is. That height is the only reading in this binary that can see the
/// `padding: 8px 10px` `overlay_style::OVERLAY_CSS_DATA` puts on `list > row` — the half
/// of the menu appearance [`PICKOVER_UNSTYLED_FILL`] says nothing about, because the box's
/// surface is the same colour whether its rows are menu-sized or packed.
///
/// MEASURED `(54, 74, 101)` off the retained sample capture, bounding box
/// `x 471..1058, y 117..150`: the full width of the box's 588 px content area, and 34 rows
/// tall. That capture was drawn by GSK's GL renderer; under its cairo renderer, which GTK
/// falls back to where no GL is available, the same row reads `(53, 73, 100)`, one step
/// low on every channel. Assertion 9 therefore compares within
/// [`PICKOVER_ROW_HIGHLIGHT_TOLERANCE`] rather than exactly.
///
/// It is a libadwaita value rather than one this crate owns — Adwaita's `:selected`
/// treatment is the accent blended into the popover background — and the cost of naming it
/// is stated rather than hidden, the way [`PICKOVER_UNSTYLED_FILL`]'s is. The degradation
/// runs the OTHER way round from that constant's, which is the useful half of the
/// difference: a libadwaita that re-tints `:selected` by more than the tolerance matches no
/// pixel, so assertion 9 reddens with a height of ZERO rather than quietly stopping
/// discriminating. Loud, but saying nothing about this front end — which is why zero is the
/// first reading that assertion's message names and tells apart from a padding regression.
const PICKOVER_ROW_HIGHLIGHT: Rgb8 = (0x36, 0x4a, 0x65);

/// How many steps per channel a pixel may differ from [`PICKOVER_ROW_HIGHLIGHT`] and still
/// count as the selected row in assertion 9's scan, through [`rgb_within`].
///
/// One, because that is the whole of the measured disagreement between GSK's two
/// renderers on this blend: `(54, 74, 101)` under GL and `(53, 73, 100)` under cairo. The
/// highlight is a blend of the accent into the popover background, and the two renderers
/// round it differently; nothing about the row's height depends on which one drew it.
///
/// It may not grow to make a run pass. Nothing else the walk crosses is near the
/// highlight — the box's fill reads `(54, 54, 58)`, twenty steps away on green — but a
/// wider tolerance starts to accept a re-tinted `:selected` as the row, which is the one
/// degradation [`PICKOVER_ROW_HIGHLIGHT`] is written to make loud.
/// [`row_highlight_tolerance_admits_the_cairo_reading_and_nothing_near`] pins both edges.
///
/// The fill side of the scan stays EXACT: that colour is read out of the same capture by
/// assertion 4, so it is already in whichever renderer's rounding drew the frame.
const PICKOVER_ROW_HIGHLIGHT_TOLERANCE: u8 = 1;

/// The fewest rows tall the picker's SELECTED list row may render.
///
/// MEASURED 34 rows with the shipped `padding: 8px 10px` on `list > row`, and 22 rows with
/// that one declaration removed from `overlay_style::OVERLAY_CSS_DATA`. The gap is 12 and
/// not the declaration's own 16 because the 8 px REPLACES the 2 px per side that Adwaita's
/// own `list > row` already carries, so the two builds differ by `2 * (8 - 2)`.
///
/// ## Why a floor and not an equality
///
/// A row's height is its padding plus its label's line height, and a line height is a FONT
/// metric. An `== 34` would therefore redden on any font, scale or theme change that moved
/// the label by a pixel while the padding this smoke is about stayed exactly as declared —
/// a failure nobody could attribute to this front end. What discriminates is a floor placed
/// strictly between the two measured heights, and the two are 12 rows apart with no reading
/// in between.
///
/// 28 is their midpoint, so neither side is favoured: 6 rows above the 22 the falsifier
/// produces (27 % of it) and 6 rows below the 34 the shipped stylesheet produces (18 % of
/// it). The row may lose 6 of its 34 rows to a font metric before this floor speaks, and
/// the padding cannot be dropped without it speaking.
///
/// It may not be lowered to make a run pass. A height at 22 is the declaration reaching the
/// row not at all; a height between 22 and this floor is it reaching the row partially —
/// the wrong axis, or a `list > row` selector that stopped matching.
const PICKOVER_MIN_SELECTED_ROW_H_PX: usize = 28;

/// How many pixels of the scanned column span a row must carry of one colour before
/// assertion 9's scan calls the row that colour.
///
/// The span scanned is [`pickover_fill_reference_band`]'s own 200 columns, which assertion
/// 4 has already established are inside the box, and which sit inside the selected row's
/// measured `x 471..1058` extent with 29 columns to spare on the left and over 350 on the
/// right. A row of the box's empty area below the selection reads all 200 as the fill. A row of
/// the selection itself reads those 200 minus whatever its LABEL covers — one left-aligned
/// `gtk::Label` holding the entry's display path, so its glyphs sit at the row's left edge
/// and punch holes in the count rather than emptying it.
///
/// ## What the readings are
///
/// MEASURED over the 189 rows the walk examines, off the retained sample capture. The
/// selection's 34 rows carry 149 to 200 highlight pixels and zero fill. The 155 empty rows
/// carry zero highlight and exactly 200 fill. The row the walk stops at — the search entry —
/// carries zero of both.
///
/// Nothing lands in between: not one row reads between 1 and 148 of either colour, so every
/// threshold in `1..=149` classifies all 189 rows identically, and 64 is one arbitrary point
/// inside a gap of 149. The F3 build holds the same shape — 22 highlight rows reading 156 to
/// 200, against 167 rows reading zero highlight and 200 fill.
///
/// ## So it is not load-bearing, and a wider label does not reach it
///
/// The 34 highlight rows split 22 at the full 200 and 12 at 149 to 198, the dip being the
/// label's glyphs. The thinnest of them clears this threshold by 85 px, so a label would have
/// to cover another 85 of the 200 scanned columns before that row stopped classifying as the
/// selection. A longer display path is not a reason to retune this number.
///
/// The axis says the same. Between the shipped build and F3 the highlight's bounding box
/// keeps its width, `x 471..1058`, and moves only vertically, `y 117..150` against
/// `y 117..138`: the highlight stretches to the list's width whatever the horizontal padding
/// does, so the count this threshold reads is taken along the axis that does not move.
///
/// The guard stays even so. A count threshold is the right shape for a classifier that has to
/// tell a row of the selection from a row carrying a few stray pixels of its colour, and wide
/// margins on this page are not a reason to compare against zero instead.
const PICKOVER_ROW_CLASSIFY_MIN_PX: usize = 64;

/// The band the picker box's own fill colour is READ OUT OF, in absolute output
/// pixels.
///
/// ## Why the colour is read rather than named
///
/// The box's surface is a theme value this crate does not own — `overlay_style`'s
/// stylesheet names `@popover_bg_color` and libadwaita decides what that resolves to —
/// and its geometry lives in three private `picker.rs` constants this binary may not import
/// (see this section's header). Both problems have one answer: take the colour out of
/// the SAMPLE capture at a band that is inside the box, and prove in the CONTROL
/// capture that the same colour is not there when the box is closed. The pair is what
/// makes the reading mean "the picker's surface" rather than "whatever is at (500,
/// 300)".
///
/// What an empirical reading cannot say is WHICH surface it found, so any opaque widget
/// under the box satisfies it. [`PICKOVER_UNSTYLED_FILL`] is the one inequality that
/// closes that, and its doc comment states how far it closes it.
///
/// ## Where the four numbers come from
///
/// MEASURED on this host at the pinned 1280x720 output: **32,000 of 32,000** pixels
/// are one colour with the picker open, and **0 of 32,000** are that colour with it
/// closed. Each edge answers to a constraint rather than to a preference. The band is
/// inside the box on both axes. It is LEFT of the pane area's horizontal midpoint, so
/// the EDITOR pane is what lies under it — which is what makes the closed reading 0
/// and, under a build that hid the box, makes this the assertion that fires rather
/// than one about the preview. And it starts well below the box's search entry and its
/// first list row, so no widget text falls inside it.
///
/// The smoke checks the two structural halves it can check — that the band is inside
/// the capture, and left of that midpoint — so a change to the output size or the
/// overlay origin is a named failure rather than a silently relocated band.
fn pickover_fill_reference_band() -> SampleRect {
    SampleRect {
        x0: 500,
        x1: 700,
        y0: 300,
        y1: 460,
    }
}

/// The PICKER-OVER-PREVIEW scenario, run as a single `sh -c` string inside ONE `cage`:
/// `PREVIEW_SCENARIO`'s split-and-preview opening, then `PICKER_SCENARIO`'s
/// Ctrl+Shift+O chord and typed query, with a `grim` on EITHER side of that chord.
///
/// ## One launch, two captures, and why the order is fixed
///
/// The control must be taken while the same process, the same web view and the same
/// rendered document are on screen as the sample — otherwise the two readings describe
/// two different windows and the pair proves nothing. So there is one launch, and the
/// only thing that happens between the two `grim` calls is the chord and the query.
///
/// ## The readback comes BEFORE both captures, which is the opposite of every sibling
///
/// `PREVIEW_SCENARIO` and `PREVIEW_THEME_SCENARIO` both take their
/// `tabpagebuflist()` readback AFTER `grim`, because the typed `:call writefile(...)`
/// is drawn by the cmdline strip and their assertions read OCR terms off the whole
/// page. Here nothing is OCRed and every sampled rectangle sits inside the preview
/// pane, far above the strip — so the readback is taken FIRST, where it belongs: it is
/// the precondition that stops both captures being a statement about an empty pane,
/// and a precondition established after the evidence is not a precondition.
///
/// ## The two settles around the chord, and what they measured
///
/// No other smoke in this binary captures a transient overlay while it is open, so there
/// was no sibling whose timing could be copied. The reconnaissance drove this recipe by
/// hand and found one `SETTLE` after the chord (for the box to map and its entry to
/// take focus) and one after the query (for the streamed traversal to rank and the
/// list to settle) sufficient — which is also what `PICKER_SCENARIO` spends before
/// its Enter. MEASURED: that budget held first time and across four executions with no
/// flake, and the symptom a short settle would produce never appeared once — the sample
/// capture's sampled rectangle read EXACTLY zero [`PREVIEW_DARK_BG`] every time. The
/// whole smoke costs 12.25 s, reproducible to the centisecond across three green runs.
///
/// That symptom is still the one to look for if the budget ever does go short, and it
/// is specific rather than a mystery: that rectangle in the sample capture reads
/// [`PREVIEW_DARK_BG`] rather than the picker's fill — which is also exactly what a
/// paint-order regression reads like, so the two are told apart by re-running rather
/// than by the numbers.
///
/// ## Exit codes
///
/// * `11` — the app died before it was ready for input (the shared convention).
/// * `14` — the window-list readback never appeared, so the precondition that makes
///   both captures mean anything cannot be checked at all.
///
/// `12` and `13` do not exist here, for [`CODEHL_SCENARIO`]'s stated reason: nothing
/// in this smoke depends on a clean exit, so it ends with `PREVIEW_SCENARIO`'s
/// best-effort `kill` + `wait` rather than a `:qa!` handshake. Adding them would mean
/// asserting on a shutdown this smoke makes no claim about — and the picker is still
/// open at that point, which is a state no other smoke quits from.
const PICKOVER_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100
# PREVIEW_SETTLE -- budgeted FROM THE CALL, not from the web view's Started event;
# PREVIEW_SCENARIO's doc comment carries the measurement behind it (~255 ms for a
# cold open, about 110 ms of that spent spawning the WebProcess before Started
# fires). This spend is a cold open. Do not trim it.
PREVIEW_SETTLE=2.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome picker-over-preview smoke"

wait_file() {
    f=$1
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        if [ -s "$f" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome picker-over-preview smoke: readback file $f never appeared (or stayed empty)." >&2
    return 1
}

# --- Step 1: the OTHER file in the starting pane -----------------------------
#
# It stays there for the whole run, so the LEFT half of the pane area is an editor
# pane in both captures -- which is what the fill-colour reference band is read over.
#
# The block cursor is deliberately NOT parked. The usual `o` + Escape park exists so
# an inverted block does not corrupt an OCR'd row; nothing here is OCRed, and every
# rectangle this smoke samples is inside the PREVIEW pane, where `draw_pane` skips
# the grid, the scrollbar and the cursor entirely.
wtype -- ":e $OTHER_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 2: split, and open the SOURCE in the new pane ----------------------
#
# The `<C-w>v` CHORD, never a typed `:vsplit`: the init.lua keymap is what emits the
# wolf_layout Split intent the front-end binds a new pane from, and `apply_split`
# puts the NEW pane second, i.e. on the right, whatever nvim's 'splitright' says.
wtype -M ctrl -k w -m ctrl
wtype -k v
sleep "$SETTLE"

wtype -- ":e $SRC_FILE"
wtype -k Return
sleep "$SETTLE"

# --- Step 3: drive PREVIEW through the command palette -----------------------
#
# Ctrl+Shift+P, the query, Enter. The IN-PLACE Preview here, deliberately: it replaces
# the pane the source is in, which is the RIGHT one. See the smoke's doc comment for
# the whole chain.
wtype -M ctrl -M shift -k p -m shift -m ctrl
sleep "$SETTLE"
wtype -- "$PREVIEW_QUERY"
sleep "$SETTLE"
wtype -k Return

# The COLD open budget: the blocking open_preview RPC, the wake that carries the
# wolf_preview notification, the lazily-created WebView's WebProcess spawn, the
# blocking render_preview and the full load_html.
sleep "$PREVIEW_SETTLE"

# --- Step 4: NEOVIM's own answer, BEFORE either capture ----------------------
#
# Taken first on purpose, unlike every sibling preview scenario: it is the
# precondition that stops both captures being a statement about an empty pane. The
# typed line is drawn by the cmdline strip, which sits below every rectangle read
# here. Joined with a COMMA rather than a space because a preview scratch's own name
# contains one.
wtype -- ":call writefile([join(map(tabpagebuflist(), {_, b -> fnamemodify(bufname(b), ':t')}), ',')], '$WINS')"
wtype -k Return
wait_file "$WINS" || exit 14

# Let the cmdline strip clear before the control capture, so the two captures differ
# in the overlay and in nothing else.
sleep "$SETTLE"

# --- Step 5: the CONTROL capture, with NO overlay open -----------------------
grim "$OUT_PNG_CONTROL"

# --- Step 6: open the FILE PICKER over it ------------------------------------
#
# Ctrl+Shift+O is the `win.go-to-file` accelerator (Ctrl+Shift+P is the palette).
# `-M <mod>` / `-m <mod>` press and release a modifier and `-k <key>` taps a key
# while they are held, so this is "hold ctrl, hold shift, tap o, release, release".
wtype -M ctrl -M shift -k o -m shift -m ctrl

# One settle for the box to map and its search entry to take focus. MEASURED
# sufficient across four executions with no flake; see the doc comment above.
sleep "$SETTLE"

# The query, so the streamed results rank down to a single row.
wtype -- "$PICKER_QUERY"

# One settle for that ranking to land. MEASURED sufficient, as above.
sleep "$SETTLE"

# --- Step 7: the SAMPLE capture, with the picker STILL OPEN ------------------
#
# No Enter, deliberately: activating a row would close the box, and the box is the
# subject. This is the only `grim` in this binary taken while a transient overlay is
# up on purpose.
grim "$OUT_PNG_SAMPLE"

# Best-effort teardown: the app may already be gone.
set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The file picker's floating box composites OVER a live Markdown preview: the pixels
/// where the two regions overlap are the picker's, and the preview's are gone.
///
/// ## What it proves that nothing else does
///
/// The three transient overlays are children of the WINDOW-level `gtk::Overlay`
/// (`picker.rs`, `palette.rs`, `buffer_picker.rs`), while a
/// preview pane's `webkit6::WebView` is added to that PANE's own overlay
/// (`pane_view.rs`). The claim that the first therefore paints above the second was an
/// argument from construction with no test behind it, and
/// `notes/wolf-gnome/window-structure.md` named a residual risk beside it — a WebKit rendering
/// through a Wayland subsurface, which would sit outside GTK's ordering entirely. This smoke is
/// the observation: the same window, photographed twice, with nothing between the two frames but the chord that
/// opens the box.
///
/// MEASURED against that residual risk before this smoke was written, on a fixture
/// that forced an accelerated-compositing layer (`transform: translateZ(0)`,
/// `will-change: transform` and a running `@keyframes` rotation — visibly animating,
/// so unquestionably promoted): the overlap rectangle read 74.99 % of the layer's own
/// red with no overlay and **0.00 % red / 100.00 % picker fill** with the box open. A
/// promoted compositing layer is inside the GTK scene and is occluded by the sibling
/// overlay child, so the risk does not fire on this stack.
///
/// ## Why the FILE PICKER, and not the palette or the buffer picker
///
/// All three are the same feat-list row and all three are added to the same overlay in
/// the same way, but only the file picker has a deterministic box: `picker.rs` sets a
/// height request as well as a width, while `palette.rs` and `buffer_picker.rs` set
/// width and margin only, so their boxes grow with their row count. The palette's
/// bottom edge measured y 465 with today's twenty commands and MOVES whenever a
/// command is added — which would make a band derived from it silently wrong rather
/// than loudly wrong. The other two are recorded as sharing the structure rather than
/// asserted separately.
///
/// The macOS half of the same row covers the same three pickers: `PickerPanel`'s own
/// header gives the reason (the window server always composites a child window in
/// front of its parent's content, including the Markdown-preview `WKWebView`), and
/// `WolfWindowController` holds exactly three hosts. Project search is a sidebar panel
/// on both platforms, so it is outside the row.
///
/// ## No OCR, and the trap that rules it out
///
/// "The picker's rows are on screen" looks like the cheap version of this test and is
/// worthless here. The rows are FILENAMES, and the tree sidebar renders every one of
/// them by name on every capture — so that assertion is satisfied with the picker
/// CLOSED, which is precisely the vacuity `tests/README.md`'s fixture-naming rule
/// exists to prevent. And OCR cannot say WHERE a word is, so no OCR assertion can
/// distinguish a box drawn over the preview from one drawn beside it. Pixels are the
/// only instrument that answers the question, which is why this smoke asserts no OCR
/// term and contributes no `ASSERTED_OCR_TERMS` row.
///
/// ## The two rectangles, and why every edge of them is located from the pixels
///
/// R is the intersection of the web view with the picker box, and S is R pulled in from
/// the box's right and bottom edges by [`PICKOVER_CORNER_INSET_PX`]. Assertion 6
/// measures R, because "the box covers the preview by at least 200 x 200" is a claim
/// about the real overlap. Assertions 7 and 8 both read S, because the box's corner is
/// ROUNDED and the preview genuinely shows through R at that corner — see that
/// constant for why the inset is the declared radius rather than a tuned number.
///
/// S also starts on the first row at or below `web_top` whose columns carry no pixel of
/// [`PICKOVER_ROW_HIGHLIGHT`], within [`PICKOVER_ROW_HIGHLIGHT_TOLERANCE`]. With the web
/// view reaching its strip, the picker's selected row lies inside R's top rows, and that
/// row is the box's own surface but not its fill. S must hold one colour, so it skips the
/// row. When the preview is painted over the box no highlight reaches S's columns, S starts
/// at `web_top`, and assertion 8's zeros fire as before.
///
/// R's four edges come from two captures, each edge read where that boundary is actually
/// visible — which is not the same capture for all four, and that asymmetry is the
/// point:
///
/// * `web_top` and `web_left` can only be read from the CONTROL capture, because the
///   box covers both of those boundaries in the sample.
/// * `box_right` and `box_bottom` can only be read from the SAMPLE capture, because
///   there is no box in the control.
///
/// The two CONTROL edges are located as a transition TO [`PREVIEW_DARK_BG`]: the first
/// fully-dark row below the pane's chrome, and the first fully-dark column right of the
/// pane midpoint. The two SAMPLE edges are located the other way round — the first
/// column right of the web view's left edge, and the first row below the reference band,
/// that stops being the box's own fill. That asymmetry is forced rather than chosen: the
/// box carries an outset shadow which is NEITHER colour, so a scan for the dark side
/// arriving reports an edge several lines past the surface and puts pixels the box never
/// painted inside R. [`PICKOVER_EDGE_MIN_NON_FILL_PX`] carries the threshold those two
/// scans use and the rounded corner it exists for.
///
/// The fill they scan for is the colour assertion 4 read out of the sample and proved
/// absent from the control, so R's right and bottom edges are where the BOX's surface
/// stops rather than where some flat colour does.
///
/// Derived at the pinned output size, with the web view reaching its pane's left edge and
/// its strip: **R = x 767..1065, y 139..471, 98,936 pixels**. A row scan of a kept sample
/// capture read the selected row across S's columns at rows 139..150, so
/// **S = x 767..1053, y 151..459, 88,088 pixels**, derived. These are not re-measured. The
/// readings below were measured off this smoke's own retained captures with R's left and
/// top edges 20 px further in, at R = x 787..1065, y 159..471 and S = x 787..1053,
/// y 159..459, 79,800 pixels.
///
/// S is constructed only AFTER both halves of assertion 6 have passed, and the ordering
/// is load-bearing rather than tidy: under the P1b falsifier `box_right == web_left`, so
/// an inset taken any earlier gives `x1 < x0` and panics on the underflow inside
/// `SampleRect::pixels` in place of assertion 6's diagnostic. Clearing the 200-pixel
/// floors is what makes the two subtractions unconditionally safe.
///
/// Nothing in `picker.rs` is read to get there, and nothing in `src/` changes for this
/// smoke to exist. That property is worth keeping: a test that imported the box's
/// geometry would assert that the box is where its own constants put it.
///
/// Assertion 9 reads neither rectangle, and deliberately: the box measures 600 x 400
/// whatever its rows do, so no reading of R or S is an observable of the row metrics.
/// S's top follows the selected row's bottom, but no reading of S tells the row's height,
/// and under F3 the row ends above `web_top` by derivation, which leaves S at R's top. It
/// scans [`pickover_fill_reference_band`]'s columns UPWARD from that band instead, and its
/// region is bounded by the box rather than by a literal — see the comment at the
/// assertion for the walk and what stops it.
///
/// ## The pair is what discriminates, and neither capture may be dropped
///
/// The property under test is GTK's composition rather than a line of this crate's
/// code, so no small edit disables it cleanly and there is no clever falsifier to
/// build the smoke around. What there is instead is a control: the SAME rectangle,
/// read on a frame with no overlay open, must be the preview — a floor of the
/// document's background AND a floor of its heading's ink. Without it the sample half
/// is satisfied by any box drawn anywhere over anything, including over a web view
/// that never rendered, which is the same shape
/// `preview_document_follows_the_configured_appearance_under_cage`'s control band
/// has and documents.
///
/// ## Assertions, classified
///
/// A precondition gets NO revert: inventing one would manufacture a discriminator this
/// smoke does not have. Every cell of the `measured` column is a reading of this
/// smoke's own retained captures at the pinned 1280x720 output, re-derived
/// independently of the reconnaissance's by-hand figures, which were taken against a
/// different fixture heading and read differently.
///
/// | # | assertion | kind | falsifying edit | measured |
/// | --- | --- | --- | --- | --- |
/// | 1 | `status.success()` | precondition — the cage scenario ran to completion | none; do not invent one | n/a |
/// | 2 | both PNGs present, each decoding at exactly `CAGE_OUTPUT_W_EXPECTED` x `CAGE_OUTPUT_H_EXPECTED` | precondition — and what makes every derived rectangle below mean what it says | none; do not invent one | n/a |
/// | 3 | `tabpagebuflist()`, sorted, is the preview scratch plus the other file | precondition — a preview pane exists and exactly two panes do, established BEFORE either capture | none; do not invent one | green: the preview scratch for [`PICKOVER_SOURCE_FILE`] plus [`PICKOVER_OTHER_FILE`], and nothing else |
/// | 4 | the reference band is ONE colour in the sample, that colour appears ZERO times in the same band in the control, and it is none of [`PREVIEW_DARK_BG`], [`PICKOVER_HEADING_INK`] or [`PICKOVER_UNSTYLED_FILL`] | precondition in its first two halves — they make the fill EMPIRICAL rather than a pinned theme value, and say the colour belongs to the box rather than to the pane under it — and **discriminator** in the unstyled-fill half, which is the only reading in this binary that separates "the overlay stylesheet resolved" from "an opaque widget under the box painted something" | F2, below: empty the body of `overlay_style::apply`. It is also where a build that hid the box ENTIRELY reddens — P1b below is exactly that build, and **this is the assertion P1b's red fired on**, second half | green: 32,000 / 32,000 open, 0 / 32,000 closed, the fill `(54, 54, 58)`. red (P1b): the band is uniform in BOTH captures and the colour is the same `(20, 22, 27)` in both, so the absence half read 32,000. red (F2): the uniformity and absence halves both hold and the fill reads exactly [`PICKOVER_UNSTYLED_FILL`] `(29, 29, 32)`, which is the line that fires and the only one that does |
/// | 5 | all four edges of R are locatable | precondition — the web view's top and left in the control by the preview background arriving, the box's right and bottom in the sample by its own fill leaving | none; do not invent one | green: 787..1065 x 159..471 measured; 767..1065 x 139..471 derived |
/// | 6 | R — the TRUE overlap, before any inset — spans at least [`PICKOVER_MIN_OVERLAP_W_PX`] columns and [`PICKOVER_MIN_OVERLAP_H_PX`] rows | **discriminator, geometrical** — the box's VISIBLE surface reaches into the web view's own columns and rows at all. Passing it is also what makes S's subtraction safe, which is why S is built after this row rather than before it | P1b, below — NOT reached, because assertion 4 fired first. Carried through by hand off the red PNG it would have read `overlap_w` = 0 against the 200 floor | green: 278 x 312 measured; 298 x 332 derived |
/// | 7 | in the CONTROL, S is at least [`PICKOVER_CONTROL_MIN_DARK_FRACTION`] `PREVIEW_DARK_BG` and carries at least [`PICKOVER_CONTROL_MIN_INK_PX`] [`PICKOVER_HEADING_INK`] pixels | **paired control** — S really is inside a web view that really rendered the document. Without it the sample half holds over a blank pane. It reads S and not R because assertion 8 reads S: a control over a different rectangle would not cover the region the discriminator samples. | drop the Preview leg from the scenario, or blank the render (`WolfCore::render_preview` answering the empty string it returns on an RPC failure). The first was TAKEN and reddens assertion 3, the louder and more correct answer — so the ink floor here has **no captured red**. The only edit that reaches it is the second, and that `src/` change has not been run against this smoke. A known gap, recorded rather than implied. | green: 0.9405 dark of the measured S's 79,800 pixels, 1,976 white |
/// | 8 | in the SAMPLE, S holds EXACTLY zero `PREVIEW_DARK_BG`, EXACTLY zero [`PICKOVER_HEADING_INK`], and at least [`PICKOVER_SAMPLE_MIN_FILL_FRACTION`] of the fill colour assertion 4 read back | **discriminator** — the fully-occluded part of the overlap is the picker's surface and nothing of the preview survives in it | P1b, below — NOT reached, and it would have been vacuous if it had been: the red's R is degenerate at zero area | green: 0, 0, and a fill fraction of exactly 1.0000 across the measured S's 79,800 pixels; the derived S is 88,088 pixels. The inset puts the box's corner arc outside S and the row scan puts the selected row above it, because S must hold one colour: it holds `(54, 54, 58)`, and the histogram of everything else is empty |
/// | 9 | in the SAMPLE, the run of [`PICKOVER_ROW_HIGHLIGHT`] rows inside the box is at least [`PICKOVER_MIN_SELECTED_ROW_H_PX`] rows tall | **discriminator** — the SELECTED ROW is menu-sized, which is the half of the overlay appearance no other assertion here reads. Assertion 4's fill inequality sees the box's surface and is satisfied whatever the rows measure | F3, below: remove `padding: 8px 10px` from `list > row` in `overlay_style::OVERLAY_CSS_DATA` | green: 34 rows, bbox `x 471..1058, y 117..150`. red (F3): 22 rows |
///
/// ## The falsifiers
///
/// **P1b, the one taken.** After `overlay.add_overlay(&picker_box)` in `picker::build`,
/// add `picker_box.insert_before(overlay, overlay.first_child().as_ref());`. A pure
/// paint-order change and nothing else. Note the ABSENT `&` on `overlay`: that
/// parameter is already a `&gtk::Overlay`, so passing `&overlay` does not compile at
/// all — it fails `E0277` on the doubly-borrowed widget and reddens nothing. The line
/// written above is the one that builds, and it is written out here so the next person
/// applying this falsifier does not spend ten minutes on the borrow.
///
/// **Say plainly what it cannot do: it cannot separate "over the preview" from "on
/// screen at all".** It moves the box below the window overlay's MAIN child, and that
/// child paints across the whole window, so the box is hidden everywhere rather than
/// only where the web view is.
///
/// **Assertion 4 is what fired, and the honest reading of that is not softened.** The
/// colour read back out of the reference band was `(20, 22, 27)` — the EDITOR pane's
/// own background — and it was IDENTICAL in both captures, so the absence half of
/// assertion 4 fired. The box was therefore not on screen ANYWHERE, not merely absent
/// from over the preview. **The red says "no box", not "box below the web view"**,
/// which is exactly the limitation stated above rather than a targeted result.
///
/// What would have fired next is worth recording, because it is what says assertion 6
/// is not decoration. Carried through by hand off the same red PNG: `web_top` 159,
/// `web_left` 787, `box_right` 787, or 139, 767 and 767 by derivation — so `overlap_w` = 0
/// against its 200 floor, and R is
/// degenerate at zero area. Assertion 8 was never reached, and on a zero-area rectangle
/// it would have been vacuous if it had been.
///
/// **P1a, recorded and NOT taken.** Move the `webkit6::WebView` from the pane's own
/// overlay to the WINDOW-level one — `notes/wolf-gnome/window-structure.md`'s explicitly
/// rejected alternative.
/// That inverts exactly the relation under test, which is what P1b cannot do. It also
/// changes the view's GEOMETRY, since a window-level overlay child is laid out against
/// the window rather than against the pane, so its red is not isolated: R would move as
/// well as invert and the reading would not be attributable. It is written down here,
/// with that cost stated, so a later change takes it deliberately rather than
/// rediscovering it.
///
/// **P1c, rejected.** Making `PickerHandle::open` a no-op reddens assertions 4 and 6
/// while proving nothing about z-order — it says only that a box that was never shown
/// is not on screen.
///
/// **F2, and the reason assertion 4 carries a third inequality.** Empty the body of
/// `overlay_style::apply`: no class is added and no provider installed, so the overlay
/// chrome reaches nothing. MEASURED on that build: the reference band still reads one
/// flat opaque colour, still absent from the control, and every OTHER assertion in this
/// smoke passes — assertion 8 included. The colour is the `GtkListBox`'s own
/// `@view_bg_color`, which the overlay stylesheet turns transparent, so the box's surface
/// in that build belongs to a widget underneath rather than to the overlay chrome. The
/// [`PICKOVER_UNSTYLED_FILL`] inequality is the line that fires on it, and that constant's
/// doc comment states how far the guard reaches and where it stops.
///
/// **F1, its narrower sibling.** Mistype `@popover_bg_color` inside the stylesheet's
/// data. That reddens assertion 4's ABSENCE half — the band reads the editor pane's
/// `(20, 22, 27)` in both captures — but only because the rule that makes the list
/// transparent outlives the erroring declaration. A typo that took the whole block with
/// it would read exactly like F2, which is why F1 is recorded as the narrow falsifier
/// rather than as the one this assertion rests on.
///
/// **F3, and the reason assertion 9 exists.** Remove `padding: 8px 10px` from the
/// `list > row` block of `overlay_style::OVERLAY_CSS_DATA`, leaving every other
/// declaration in place. That is the LINE-HEIGHT half of the overlay appearance, and F2
/// says nothing about it: the box's surface is [`PICKOVER_UNSTYLED_FILL`] or it is not,
/// whatever the rows inside it measure. MEASURED on that build off a fresh sample capture:
/// the selected row's [`PICKOVER_ROW_HIGHLIGHT`] run drops from 34 rows to 22.
///
/// Assertion 9 is the only one that reddens on it, at 22 rows against its 28 floor, and
/// assertions 1 through 8 stay green. That every one of those eight was evaluated is
/// structural rather than assumed: assertion 9's `assert!` is the last in the test, so a
/// panic there is reached only by running and passing all eight above it.
///
/// Their holding is structural too — nothing above assertion 9 reads a row metric, and a
/// shorter row leaves MORE of the box empty, so it can only push assertion 8's fill fraction
/// up.
///
/// **The control's own falsifier is dropping the Preview leg**, and TAKEN it reddens
/// assertion 3 — the readback — rather than assertion 7, which is the louder and more
/// correct answer and the predicted one: without a preview pane the rectangle is not a
/// statement about a preview at all, and the readback says so in one line before a
/// pixel is counted.
///
/// The consequence is a gap, and it is written down rather than left implied:
/// **assertion 7's ink floor has no captured red.** The only edit that reaches it is
/// blanking `WolfCore::render_preview` with the pane still bound — a `src/` edit applied
/// and reverted the way F1 and F2 are, which nobody has run. Until somebody does, the ink
/// floor rests on its green reading and on the dark-fraction half beside it.
///
/// ## The conventions it obeys
///
/// A fresh temp working directory ([`make_temp_cwd`], mandatory — the captive Neovim's
/// swap directory is keyed on it); [`require_smoke_env`]'s cage slot and runtime directory for the whole body; one `sh -c`
/// string through [`cage_command`], the single environment helper and the only place
/// the compositor is spawned; a config directory of its own pinning the appearance the
/// two theme colours are measured at; and two exit codes naming two causes.
///
/// It asserts no OCR term, so it contributes no row to `ASSERTED_OCR_TERMS`. Its
/// three files ARE registered in `RELAUNCH_FIXTURE_NAMES`, which is the
/// `LINK_SOURCE_FILE` precedent rather than the `PREVIEW_THEME_SOURCE_FILE` one:
/// nothing here can be satisfied by a sidebar row, but the generic loop is what stops
/// a later rename turning one of these names into page text that breaks somebody
/// else's smoke.
#[test]
fn file_picker_covers_the_live_preview_under_cage() {
    const SMOKE: &str = "file_picker_covers_the_live_preview_under_cage";
    const CONTROL_LABEL: &str = "the control capture (preview live, no overlay)";
    const SAMPLE_LABEL: &str = "the sample capture (file picker open)";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");

    // A fresh temp cwd, mandatory: the captive Neovim's swap directory is keyed on it.
    let cwd = make_temp_cwd();
    let source_body = format!("{PICKOVER_SOURCE_HEADING}\n\n{PICKOVER_SOURCE_PARAGRAPH}");
    write_relaunch_fixtures(
        SMOKE,
        &cwd,
        &[
            (PICKOVER_SOURCE_FILE, source_body.as_str()),
            (PICKOVER_OTHER_FILE, PICKOVER_OTHER_HEADING),
        ],
    );

    // Its own config at a forced dark appearance: the asserted colours are the dark palette's, so a
    // re-pin of the shared seeded value would sample a light document while still finding edges.
    let config = smoke_config_dir_at(
        &cwd,
        PICKOVER_SUFFIX,
        SMOKE_FONT_PT,
        APPEARANCE_FORCED_DARK,
        None,
    );

    let control_png = runtime_dir.join(PICKOVER_CONTROL_PNG_NAME);
    let sample_png = runtime_dir.join(PICKOVER_SAMPLE_PNG_NAME);
    let wins = runtime_dir.join(PICKOVER_WINS_NAME);
    for path in [&control_png, &sample_png, &wins] {
        clear_evidence(path, SMOKE);
    }

    let mut cmd = cage_command(PICKOVER_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        // Chained AFTER `cage_command`, deliberately winning over the shared config
        // directory it set — the code-highlight smoke's precedent.
        .env("WOLF_CONFIG_DIR", &config)
        .env("SRC_FILE", PICKOVER_SOURCE_FILE)
        .env("OTHER_FILE", PICKOVER_OTHER_FILE)
        .env("PREVIEW_QUERY", PREVIEW_QUERY)
        .env("PICKER_QUERY", PICKOVER_QUERY)
        .env("OUT_PNG_CONTROL", &control_png)
        .env("OUT_PNG_SAMPLE", &sample_png)
        .env("WINS", &wins);

    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless picker-over-preview scenario");

    // --- Assertion 1 (precondition): the scenario ran to completion -----------
    assert!(
        status.success(),
        "{SMOKE}: the cage picker-over-preview scenario exited unsuccessfully ({status}). Exit 11: \
         the app died before it was ready for input, which is a LAUNCH failure and says nothing \
         about the picker or the preview. Exit 14: the window-list readback never appeared at {} — \
         keys are not reaching Neovim, or the typed Ex command did not execute, so this is a \
         keyboard failure rather than a compositing one and the keyboard-only smokes are where to \
         look first. Codes 12 and 13 do not exist here on purpose: nothing in this smoke depends on \
         a clean exit, and the picker is still open when the scenario kills the app. Any OTHER \
         status is `grim` or `wtype` failing under `set -eu`. Note this assertion says nothing \
         about whether a preview ever opened or whether the picker ever appeared — the scenario \
         exits 0 whatever either of them did.",
        wins.display()
    );

    // Assertion 2 (precondition): presence first, so "grim wrote nothing" is not a decode error.
    assert_png_present(SMOKE, &control_png);
    assert_png_present(SMOKE, &sample_png);
    let control = decode_capture(&control_png, SMOKE, CONTROL_LABEL);
    let sample = decode_capture(&sample_png, SMOKE, SAMPLE_LABEL);

    // Assertion 3 (precondition): Neovim's buffer names, written before either `grim` and read before
    // any pixel, since a non-preview pane fails assertion 7 for nothing.
    let want_windows = {
        let mut want = vec![
            format!("{PREVIEW_READBACK_PREFIX}{PICKOVER_SOURCE_FILE}"),
            PICKOVER_OTHER_FILE.to_string(),
        ];
        want.sort();
        want
    };
    let got_windows = read_window_buffer_names(&wins, SMOKE);
    assert_eq!(
        got_windows, want_windows,
        "{SMOKE}: after the Preview leg the captive Neovim's windows hold {got_windows:?}, not \
         {want_windows:?} (both sorted, so window order is not the subject). Entries naming \
         {PICKOVER_SOURCE_FILE:?} and {PICKOVER_OTHER_FILE:?} with NO preview scratch mean no \
         preview was bound at all — the palette dispatched nothing, `shell::perform` has no \
         `OpenPreview` arm, or the {PREVIEW_QUERY:?} query matched no row. A SINGLE entry means the \
         `<C-w>v` split never happened, so the in-place bind replaced the window's only pane \
         and the preview is not where every rectangle below \
         assumes it is. THREE entries mean it split twice. The whole geometry below is derived on \
         the assumption of exactly two side-by-side panes with the preview in the RIGHT one, so \
         none of it means anything until this holds."
    );

    // Assertion 4 (precondition): the fill colour, read from a uniform band absent from the control.
    // See notes/crates/wolf-gnome/tests/smoke_headless/preview_render.rs.md#picker-fill-colour
    let reference = pickover_fill_reference_band();
    let (area_x, _, area_w, _) = preview_theme_pane_area();
    let pane_left = pixel_index(area_x, "the pane area's left edge");
    let pane_mid_x = pixel_index(area_x + area_w / 2.0, "the pane area's horizontal midpoint");
    assert!(
        reference.x0 >= pane_left
            && reference.x1 <= pane_mid_x
            && reference.x0 < reference.x1
            && reference.y0 < reference.y1
            && reference.x1 <= control.width
            && reference.y1 <= control.height,
        "{SMOKE}: the fill-colour reference band {reference:?} is degenerate, or is no longer \
         inside the EDITOR half of the pane area (columns {pane_left}..{pane_mid_x}, derived from \
         CAGE_OUTPUT_*_EXPECTED and OVERLAY_ORIGIN_*_EXPECTED), or falls outside the {}x{} capture. \
         The band must sit over the editor pane: that is what makes its CLOSED reading zero, and a \
         band that drifted into the preview half would read the document's background as the \
         picker's fill. Re-derive it against a retained capture; do not nudge the literals to make \
         a run pass.",
        control.width,
        control.height
    );

    let fill = sample.pixel(reference.x0, reference.y0);
    let fill_hits = sample.count_exactly(reference, fill);
    assert_eq!(
        fill_hits,
        reference.pixels(),
        "{SMOKE}: in {SAMPLE_LABEL}, only {fill_hits} of the reference band {reference:?}'s {} \
         pixels are {fill:?} — the colour at its own top-left corner — so the band is not one flat \
         surface and no single pixel of it can stand for the picker's fill. A count near zero means \
         the picker box is not covering this band at all: either the chord never opened it, or the \
         box moved. A count that is a large FRACTION of the band means the band straddles an edge \
         — the box's own boundary, or a list row's highlight reaching further down than it did — \
         and the band is what to re-derive, not this assertion. Captures kept at {} and {}.",
        reference.pixels(),
        control_png.display(),
        sample_png.display()
    );

    let control_fill_hits = control.count_exactly(reference, fill);
    assert_eq!(
        control_fill_hits,
        0,
        "{SMOKE}: the colour {fill:?} read out of {SAMPLE_LABEL} also appears {control_fill_hits} \
         times inside the SAME band {reference:?} of {CONTROL_LABEL}, where no overlay is open. It \
         is therefore not the picker's surface but something the window draws either way, and \
         assertion 8 below would be satisfied by a frame in which the picker never appeared. Two \
         causes read IDENTICALLY here, and only the retained PNGs separate them. Either the chord \
         did not open the box in the sample run, so both captures show the editor pane — check \
         that Ctrl+Shift+O still reaches `win.go-to-file`. Or the box opened and is painted under \
         something that hides it everywhere: that is what the P1b paint-order falsifier did, and \
         the colour it produced here was the editor pane's own `(20, 22, 27)` in BOTH captures, \
         indistinguishable from a chord that never arrived except by looking for the box in the \
         sample PNG. Captures kept at {} and {}.",
        control_png.display(),
        sample_png.display()
    );

    assert_ne!(
        fill, PREVIEW_DARK_BG,
        "{SMOKE}: the picker's fill colour read back as {fill:?}, which is exactly PREVIEW_DARK_BG \
         — the previewed document's own background. The two colours must differ or assertion 8's \
         'zero PREVIEW_DARK_BG' and 'a floor of fill' halves are the same statement negated, and \
         the smoke could not distinguish a box that covers the preview from one that does not."
    );
    assert_ne!(
        fill, PICKOVER_HEADING_INK,
        "{SMOKE}: the picker's fill colour read back as {fill:?}, which is exactly \
         PICKOVER_HEADING_INK — the rendered heading's own colour. Assertion 8 counts both inside \
         one rectangle, so they must differ for its zero-ink half to mean anything."
    );
    assert_ne!(
        fill,
        PICKOVER_UNSTYLED_FILL,
        "{SMOKE}: the overlay box's surface read back as {fill:?}, which is exactly \
         PICKOVER_UNSTYLED_FILL — the `GtkListBox`'s own `@view_bg_color`, the colour the box \
         paints when `overlay_style::apply` reaches it with nothing. The overlay stylesheet is \
         what turns that list background transparent so the box's own `@popover_bg_color` surface \
         shows through, so reading the list's colour here means the chrome was never applied: the \
         provider did not install, the `wolf-overlay` class did not land on this box, or the \
         stylesheet failed to parse before its background declaration. Every other half of this \
         assertion holds in that state — the band is uniform and the colour is absent from the \
         control, because the list widget is opaque either way — and so does assertion 8, which is \
         why this inequality exists at all: without it the smoke measures 'an opaque box covers \
         the preview' and nothing about the overlay chrome. Read the other way round, this is a \
         libadwaita value rather than one this crate owns, so a libadwaita that re-tinted \
         `@view_bg_color` would make this line stop discriminating rather than fail — check that \
         first if the overlay chrome regresses and this smoke stays green. Captures kept at {} and \
         {}.",
        control_png.display(),
        sample_png.display()
    );

    // Assertion 5 (precondition): each edge of R from the capture where it shows; the sample's by the
    // fill leaving, since the shadow is neither colour. See PICKOVER_EDGE_MIN_NON_FILL_PX.
    let preview_columns = preview_theme_preview_columns();
    let scan_rows = preview_theme_scan_rows();
    let (band_y0, band_y1) = preview_theme_band_rows();

    let web_top = control
        .first_uniform_row(preview_columns, scan_rows, PREVIEW_DARK_BG)
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: in {CONTROL_LABEL}, no row between {} and {} is entirely PREVIEW_DARK_BG \
                 = {PREVIEW_DARK_BG:?} across columns {}..{}, so the web view's own top edge could \
                 not be located and R has no top. Two causes: the document is not rendering at all \
                 (an empty `render_preview`, or `about:blank`), or its FIRST block is not a \
                 paragraph — a document opening with a full-width `hr` or table has no run of pure \
                 background at its top. Assertion 3 already passed, so a preview pane DOES exist. \
                 Capture kept at {}.",
                scan_rows.0,
                scan_rows.1,
                preview_columns.0,
                preview_columns.1,
                control_png.display()
            )
        });

    let web_left = control
        .first_uniform_column(
            SampleRect {
                x0: pane_mid_x,
                x1: control.width,
                y0: band_y0,
                y1: band_y1,
            },
            PREVIEW_DARK_BG,
        )
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: in {CONTROL_LABEL}, no column between the pane midpoint ({pane_mid_x}) \
                 and the right edge of the {}-wide capture is entirely PREVIEW_DARK_BG = \
                 {PREVIEW_DARK_BG:?} across rows {band_y0}..{band_y1}, so the web view's own left \
                 edge could not be located and R has no left. Those rows are the lower half of the \
                 pane, which the fixture is deliberately too short to reach, so ink there means the \
                 document grew — shorten PICKOVER_SOURCE_PARAGRAPH. A preview that is in the LEFT \
                 half instead would also read this way; assertion 3 cannot see which half. Capture \
                 kept at {}.",
                control.width,
                control_png.display()
            )
        });

    let box_right = sample
        .first_column_past_surface(
            SampleRect {
                x0: web_left,
                x1: sample.width,
                y0: reference.y0,
                y1: reference.y1,
            },
            fill,
            PICKOVER_EDGE_MIN_NON_FILL_PX,
        )
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: in {SAMPLE_LABEL}, no column between the web view's left edge \
                 ({web_left}) and the right edge of the {}-wide capture carries as many as \
                 {PICKOVER_EDGE_MIN_NON_FILL_PX} pixels that are not the box's fill {fill:?} \
                 across rows {}..{}, so the box's own right edge could not be located and R has no \
                 right. What that means is that the box's surface never stops before the output \
                 does: it would have to be wide enough to reach the right edge of the screen, \
                 which no `PICKER_BOX_WIDTH` this window can be laid out at allows. A colour that \
                 the whole pane paints would read this way too, but assertion 4 above has already \
                 excluded it by finding this colour nowhere in the control. Capture kept at {}.",
                sample.width,
                reference.y0,
                reference.y1,
                sample_png.display()
            )
        });

    // Assertion 6 (discriminator): before the bottom-edge scan, which panics on an empty column range;
    // this width reads 0 when the box is on screen but not over the preview.
    let overlap_w = box_right.saturating_sub(web_left);
    assert!(
        overlap_w >= PICKOVER_MIN_OVERLAP_W_PX,
        "{SMOKE}: the picker box's visible surface spans only {overlap_w} column(s) inside the web \
         view (its right edge located at {box_right}, the view's left edge at {web_left}), fewer \
         than the {PICKOVER_MIN_OVERLAP_W_PX} floor; the derived span is 298. A reading of ZERO is \
         the named one and it means the box's visible right edge coincides with the web view's \
         left edge — the preview is painted OVER the box rather than under it, so the box stops \
         being visible exactly where the view begins. A box that is on screen NOWHERE reads 0 here \
         too — P1b's red PNG carries exactly that — which is why assertion 4 above, where such a \
         build reddens first, is what tells the two apart. A small non-zero reading means the box \
         moved left, or the view moved right, and the two barely meet — or that something inside \
         the box paints across the rows this edge is scanned on, which stops the fill-side scan \
         early and reads as a box narrower than it is. The retained PNG separates the three. None \
         of them is answered by lowering this floor. Captures kept at {} and {}.",
        control_png.display(),
        sample_png.display()
    );

    let box_bottom = sample
        .first_row_past_surface(
            (web_left, box_right),
            (reference.y1, band_y1),
            fill,
            PICKOVER_EDGE_MIN_NON_FILL_PX,
        )
        .unwrap_or_else(|| {
            panic!(
                "{SMOKE}: in {SAMPLE_LABEL}, no row between {} and {band_y1} carries as many as \
                 {PICKOVER_EDGE_MIN_NON_FILL_PX} pixels that are not the box's fill {fill:?} \
                 across columns {web_left}..{box_right}, so the box's own bottom edge could not be \
                 located and R has no bottom. The box would have to extend past row {band_y1} — \
                 well beyond the measured 471 — or the pane below it would have to be painting the \
                 box's own colour. A box whose bottom edge is ABOVE row {} cannot read this way: \
                 the reference band spans rows {}..{}, so a box that short leaves the band \
                 straddling its own boundary and assertion 4 fires on it long before this scan \
                 sees it. Capture kept at {}.",
                reference.y1,
                reference.y1,
                reference.y0,
                reference.y1,
                sample_png.display()
            )
        });

    let overlap_h = box_bottom.saturating_sub(web_top);
    assert!(
        overlap_h >= PICKOVER_MIN_OVERLAP_H_PX,
        "{SMOKE}: the picker box's visible surface spans only {overlap_h} row(s) inside the web \
         view (its bottom edge located at {box_bottom}, the view's top edge at {web_top}), fewer \
         than the {PICKOVER_MIN_OVERLAP_H_PX} floor; the derived span is 332. ZERO means the box \
         ends at or above where the view begins, so the two do not overlap vertically at all. A \
         small non-zero reading means the box got shorter, or the web view starts lower down the \
         pane than it did. Captures kept at {} and {}.",
        control_png.display(),
        sample_png.display()
    );

    // S is R inset past the rounded corner and started below the selected row, so it holds one
    // colour; shared by assertions 7 and 8. Built after 6, whose 200 px spans keep it in range.
    let sampled_x1 = box_right - PICKOVER_CORNER_INSET_PX;
    let sampled_y1 = box_bottom - PICKOVER_CORNER_INSET_PX;
    let sampled_y0 = (web_top..sampled_y1)
        .find(|&y| {
            !(web_left..sampled_x1).any(|x| {
                rgb_within(
                    sample.pixel(x, y),
                    PICKOVER_ROW_HIGHLIGHT,
                    PICKOVER_ROW_HIGHLIGHT_TOLERANCE,
                )
            })
        })
        .unwrap_or(sampled_y1);
    let sampled = SampleRect {
        x0: web_left,
        x1: sampled_x1,
        y0: sampled_y0,
        y1: sampled_y1,
    };
    assert!(
        sampled.y0 < sampled.y1,
        "{SMOKE}: in {SAMPLE_LABEL}, every row of the overlap from the web view's top ({web_top}) \
         down to {sampled_y1} carries a pixel of the selected row's PICKOVER_ROW_HIGHLIGHT = \
         {PICKOVER_ROW_HIGHLIGHT:?} across columns {web_left}..{sampled_x1}, so the sampled \
         rectangle {sampled:?} has no rows. A row scan of a kept sample read the selected row at rows \
         139..150, so S starts at 151. A whole overlap that scans as the selected row means the box's own fill resolved to \
         the selection colour — the reading assertion 9 calls far above 34 — which is a finding \
         about the overlay stylesheet, not about where the box is painted. Captures kept at {} and \
         {}.",
        control_png.display(),
        sample_png.display()
    );
    assert!(
        sampled.x1 <= control.width
            && sampled.y1 <= control.height
            && sampled.x1 <= sample.width
            && sampled.y1 <= sample.height,
        "{SMOKE}: the sampled rectangle {sampled:?} runs off the edge of a capture (control {}x{}, \
         sample {}x{}). Its top and left are located from the pixels of one capture and its right \
         and bottom from the other's, then pulled in, so this means a located edge is past the far \
         side of the other capture — which cannot happen while both decode at the same pinned \
         size, and assertion 2 has already checked that.",
        control.width,
        control.height,
        sample.width,
        sample.height
    );

    // Assertion 7 (paired control): S is a live preview, so 8 is not vacuous. Two floors, since a blank
    // view satisfies "dark"; it reads S because assertion 8 does.
    let control_dark = control.count_exactly(sampled, PREVIEW_DARK_BG);
    let control_dark_fraction = band_fraction(
        control_dark,
        sampled.pixels(),
        "the control capture's dark-background fraction",
    );
    assert!(
        control_dark_fraction >= PICKOVER_CONTROL_MIN_DARK_FRACTION,
        "{SMOKE}: in {CONTROL_LABEL}, only {control_dark_fraction:.4} of the sampled rectangle \
         {sampled:?} ({} pixels) is PREVIEW_DARK_BG = {PREVIEW_DARK_BG:?}, below the \
         {PICKOVER_CONTROL_MIN_DARK_FRACTION} floor; the measurement is 0.9405. This is the CONTROL \
         half of the pair, and its failure means the rectangle is not inside a live web view — so \
         the sample half below, which was not evaluated, would have proved nothing whatever it \
         read. Three readings, told apart by the size of the number. Near ZERO: the rectangle is \
         over the editor pane (background `(20, 22, 27)`) rather than the preview, so the two panes \
         are the wrong way round or the located edges are wrong. Somewhat below the floor: the \
         document grew and its ink now fills rows this rectangle spans — shorten \
         PICKOVER_SOURCE_PARAGRAPH rather than lowering the floor. Near zero with the ink floor \
         below ALSO failing: the web view is showing something that is not the document at all. \
         Captures kept at {} and {}.",
        sampled.pixels(),
        control_png.display(),
        sample_png.display()
    );

    let control_ink = control.count_exactly(sampled, PICKOVER_HEADING_INK);
    assert!(
        control_ink >= PICKOVER_CONTROL_MIN_INK_PX,
        "{SMOKE}: in {CONTROL_LABEL}, the sampled rectangle {sampled:?} carries only {control_ink} \
         pixels that are exactly PICKOVER_HEADING_INK = {PICKOVER_HEADING_INK:?}, below the \
         {PICKOVER_CONTROL_MIN_INK_PX} floor; the measurement is 1,976. The dark-background floor \
         above just passed, so the rectangle IS the web view — which leaves a view that is showing \
         a BLANK document. A count of ZERO is the named reading and means exactly that: \
         `WolfCore::render_preview` answered the empty string it returns on an RPC failure, or the \
         document rendered without its heading. That is the state this control exists to rule out, \
         because a picker box drawn over a blank view satisfies every sample-side reading below. A \
         SMALL non-zero count instead means the heading rendered but is narrower or shorter than \
         measured — a fixture change, and the answer is to restore an eleven-letter `h1`, never to \
         lower this floor. Captures kept at {} and {}.",
        control_png.display(),
        sample_png.display()
    );

    // Assertion 8 (discriminator): exact zeros, as S skips the shadow, the corner and the selected
    // row, so it holds one colour. Measured 0, 0 and 1.0000 fill; S is 88,088 pixels by derivation.
    let sample_dark = sample.count_exactly(sampled, PREVIEW_DARK_BG);
    let sample_ink = sample.count_exactly(sampled, PICKOVER_HEADING_INK);
    let sample_fill = sample.count_exactly(sampled, fill);
    let sample_fill_fraction = band_fraction(
        sample_fill,
        sampled.pixels(),
        "the sample capture's picker-fill fraction",
    );
    assert!(
        sample_dark == 0
            && sample_ink == 0
            && sample_fill_fraction >= PICKOVER_SAMPLE_MIN_FILL_FRACTION,
        "{SMOKE}: in {SAMPLE_LABEL}, the sampled rectangle {sampled:?} ({} pixels) reads \
         {sample_dark} pixels of PREVIEW_DARK_BG = {PREVIEW_DARK_BG:?} (needs exactly 0), \
         {sample_ink} pixels of PICKOVER_HEADING_INK = {PICKOVER_HEADING_INK:?} (needs exactly 0), \
         and {sample_fill_fraction:.4} of the picker's own fill {fill:?} (needs >= \
         {PICKOVER_SAMPLE_MIN_FILL_FRACTION}). The measurements are 0, 0 and exactly 1.0000, read \
         when R's left and top edges sat 20 px further in; S is 88,088 pixels by derivation at \
         the current edges. This rectangle is the overlap trimmed on its right and bottom by \
         PICKOVER_CORNER_INSET_PX = {PICKOVER_CORNER_INSET_PX}, which puts the box's rounded \
         corner outside it, and started below the selected row, whose PICKOVER_ROW_HIGHLIGHT is \
         the box's surface but not its fill. S must hold one colour, and so it holds the fill and \
         nothing else. Assertion 7 \
         above passed, so this rectangle IS a live preview with the document rendered in it, and \
         the only question left is what covers it.\n\
         The named cause is the picker box no longer painting above the preview pane's web view. \
         The three transient overlays are children of the WINDOW-level `gtk::Overlay` while a \
         preview's `webkit6::WebView` is a child of the PANE's own overlay, and it is the ordering \
         between those two that this reading is about — check `picker::build`'s \
         `overlay.add_overlay(&picker_box)` and `pane_view`'s `p.host.add_overlay(&view)` before \
         anything else.\n\
         Three readings, told apart by which of the three numbers moved. Dark near the rectangle's \
         full size with the fill near zero: the preview is painted over the box entirely. A partial \
         dark count with a partial fill fraction: the box is above the view for part of the \
         rectangle only, which is what a WebKit rendering through its own Wayland SUBSURFACE would \
         look like — that is `notes/wolf-gnome/window-structure.md`'s named residual risk, \
         measured NOT to fire on this \
         stack even against a deliberately promoted compositing layer, so a reading of this shape \
         is a finding worth recording rather than a threshold to retune. Both zeros holding with \
         the fill fraction low: the box is on top but is drawing something other than its flat \
         surface across the rectangle — a second list row below the selected one is one benign \
         version, and PICKOVER_QUERY is what controls the row count; a stylesheet whose `border-radius` grew \
         past PICKOVER_CORNER_INSET_PX is the other, and it shows as a bite out of this \
         rectangle's bottom-right corner in the retained PNG, answered by re-typing that constant \
         from the stylesheet. Neither is answered by lowering \
         PICKOVER_SAMPLE_MIN_FILL_FRACTION. Captures kept at {} and {}.",
        sampled.pixels(),
        control_png.display(),
        sample_png.display()
    );

    // Assertion 9 (discriminator): the selected row is menu-sized, reading `list > row`'s padding.
    // See notes/crates/wolf-gnome/tests/smoke_headless/preview_render.rs.md#picker-selected-row-scan
    let mut walk_stopped_at = 0;
    let mut selected_row_h = 0;
    let mut run = 0;
    for y in (0..reference.y0).rev() {
        let highlight_px = (reference.x0..reference.x1)
            .filter(|&x| {
                rgb_within(
                    sample.pixel(x, y),
                    PICKOVER_ROW_HIGHLIGHT,
                    PICKOVER_ROW_HIGHLIGHT_TOLERANCE,
                )
            })
            .count();
        let fill_px = (reference.x0..reference.x1)
            .filter(|&x| sample.pixel(x, y) == fill)
            .count();
        // `highlight_px == 0`, not a threshold, so one blended boundary row cannot end the walk early.
        if fill_px < PICKOVER_ROW_CLASSIFY_MIN_PX && highlight_px == 0 {
            walk_stopped_at = y + 1;
            break;
        }
        if highlight_px >= PICKOVER_ROW_CLASSIFY_MIN_PX {
            run += 1;
            selected_row_h = selected_row_h.max(run);
        } else {
            run = 0;
        }
    }

    assert!(
        selected_row_h >= PICKOVER_MIN_SELECTED_ROW_H_PX,
        "{SMOKE}: in {SAMPLE_LABEL}, the picker's selected list row measures \
         {selected_row_h} row(s) of PICKOVER_ROW_HIGHLIGHT = {PICKOVER_ROW_HIGHLIGHT:?}, within \
         {PICKOVER_ROW_HIGHLIGHT_TOLERANCE} per channel, across columns {}..{}, below the \
         {PICKOVER_MIN_SELECTED_ROW_H_PX} floor. The measurements are \
         34 rows with `padding: 8px 10px` on `list > row` in `overlay_style::OVERLAY_CSS_DATA`, \
         and 22 with that one declaration removed. The walk ran up from row {} and left the box \
         at row {walk_stopped_at}. Four readings, told apart by the number.\n\
         ZERO is the reading to check FIRST, because it is the one that is not about this crate: \
         the selection colour belongs to libadwaita, so a release that re-tints `:selected` \
         matches no pixel here while the padding is exactly as declared. Its other causes are a \
         query that matched no file — PICKOVER_QUERY is {PICKOVER_QUERY:?} and must match exactly \
         one — and a list that ended up with no row selected, and the retained PNG separates all \
         three at a glance.\n\
         A reading AT 22 is the named regression and the only one this assertion exists for: the \
         row is selected and drawn, and only the menu-sized padding is gone, so the list packs \
         its rows the way a plain `GtkListBox` does rather than the way a menu does.\n\
         A reading BETWEEN 22 and this floor means the padding reached the row partially — the \
         wrong axis, or a `list > row` selector that stopped matching. A reading far ABOVE 34 \
         means what the run found is not a row at all: if the box's own fill ever resolved to the \
         selection colour the whole empty area below the row would join the run and this would \
         read in the hundreds, which is a finding about the stylesheet rather than about the \
         padding. None of the four is answered by lowering this floor. Captures kept at {} and \
         {}.",
        reference.x0,
        reference.x1,
        reference.y0,
        control_png.display(),
        sample_png.display()
    );
}

/// [`rgb_within`] at [`PICKOVER_ROW_HIGHLIGHT_TOLERANCE`] admits the selected-row colour
/// GSK's cairo renderer draws, and refuses every colour assertion 9's walk must not
/// count as the row.
///
/// ## What it closes
///
/// [`file_picker_covers_the_live_preview_under_cage`]'s assertion 9 finds the selected
/// row by [`PICKOVER_ROW_HIGHLIGHT`], measured under GL, and a host without GL draws
/// that row one step lower on every channel. An exact comparison fails the smoke there
/// with a height of zero while the padding it is about is exactly as declared; a
/// comparison loose on one channel, or one-sided, counts colours that are not the row.
/// A smoke run sees one renderer per host, so neither edge is visible from it.
///
/// ## What it checks
///
/// A table against [`PICKOVER_ROW_HIGHLIGHT`], every row asserted in both argument
/// orders and every failing row reported together:
///
/// - the cairo reading `(53, 73, 100)` is within;
/// - `(53, 73, 99)`, `(52, 74, 101)` and `(54, 72, 101)`, each two steps off on one
///   channel, are not;
/// - the picker box's fill `(54, 54, 58)` is not.
///
/// Falsify: compare exactly in [`rgb_within`] (the cairo row fails); drop one channel's
/// comparison (that channel's two-step row fails); write a channel's comparison as
/// `a.0.saturating_sub(b.0)`, which is one-sided (the reversed order fails); raise the
/// tolerance to 2 (all three two-step rows fail).
///
/// ## What it needs
///
/// No environment at all: no compositor, no [`require_smoke_env`], no subprocess.
#[test]
fn row_highlight_tolerance_admits_the_cairo_reading_and_nothing_near() {
    const GUARD: &str = "row_highlight_tolerance_admits_the_cairo_reading_and_nothing_near";
    // (other colour, whether it must count as the selected row, what it is)
    let table: [(Rgb8, bool, &str); 5] = [
        (
            (53, 73, 100),
            true,
            "the row as GSK's cairo renderer draws it",
        ),
        ((53, 73, 99), false, "two steps off on blue"),
        ((52, 74, 101), false, "two steps off on red"),
        ((54, 72, 101), false, "two steps off on green"),
        ((54, 54, 58), false, "the picker box's own fill"),
    ];

    let mut wrong = Vec::new();
    for (other, want, what) in table {
        let forward = rgb_within(
            PICKOVER_ROW_HIGHLIGHT,
            other,
            PICKOVER_ROW_HIGHLIGHT_TOLERANCE,
        );
        let reverse = rgb_within(
            other,
            PICKOVER_ROW_HIGHLIGHT,
            PICKOVER_ROW_HIGHLIGHT_TOLERANCE,
        );
        if forward != want || reverse != want {
            wrong.push(format!(
                "{other:?} ({what}): want {want}, got {forward} as the second argument and \
                 {reverse} as the first"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{GUARD}: rgb_within against PICKOVER_ROW_HIGHLIGHT = {PICKOVER_ROW_HIGHLIGHT:?} at \
         PICKOVER_ROW_HIGHLIGHT_TOLERANCE = {PICKOVER_ROW_HIGHLIGHT_TOLERANCE} classifies these \
         wrongly: {wrong:#?}\n\
         The cairo row refused means the picker-over-preview smoke's assertion 9 reads a height \
         of zero on every host without GL. A two-step row admitted means the tolerance grew or \
         one channel is not compared, so a re-tinted `:selected` would pass as the row. A row \
         right in one order and wrong in the other means the comparison is one-sided. The fill \
         admitted means the box's empty area would join the selected row's run. Fix \
         `rgb_within` or the tolerance; if PICKOVER_ROW_HIGHLIGHT was re-measured, re-derive \
         these rows from the new value."
    );
}
