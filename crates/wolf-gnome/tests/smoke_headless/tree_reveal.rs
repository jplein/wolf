//! The sidebar's launch reveal: a restored tab whose editor pane is active on a file below the
//! fold must draw every row above that file once the reveal and the traversal have settled, with
//! no scroll and no expand, and again after a watcher rescan rebuilds the tree.
//!
//! The fixture is generated in the shape of a large notes folder: about three hundred root
//! Markdown files with mixed-case names, a few holding spaces and one a non-ASCII letter, one
//! file of about 400 KB, four subdirectories and the usual dot entries. The rows asserted on are
//! the subject, so their NAMES carry their terms, and
//! [`reveal_fixture_puts_its_rows_directly_above_the_current_file`] checks where they sit.

use wolf_core::app_state::{Frame, Sidebar, SidebarMode, Tab, TabContent};
use wolf_core::tree::{children_of, TreeNode};
use wolf_core::FileEntry;

use super::extras::{canonical_project, plant_window_state, planted_window};
use super::sidebar::projsearch_sidebar_band;
use super::*;

/// The six files directly above [`CURRENT_FILE`] in tree order. Two are capitalized, so the
/// block stays contiguous only under the tree's case-insensitive order.
const ABOVE_FILES: [&str; 6] = [
    "dunealfa.md",
    "Dunebravo.md",
    "dunecharlie.md",
    "DUNEDELTA.md",
    "dunefoxtrot.md",
    "dunegolf.md",
];

/// The lowercased stem of each [`ABOVE_FILES`] entry, in the same order: what OCR of its row
/// reads.
pub(crate) const ABOVE_TERMS: [&str; 6] = [
    "dunealfa",
    "dunebravo",
    "dunecharlie",
    "dunedelta",
    "dunefoxtrot",
    "dunegolf",
];

/// The file both restored panes show, and so the file the editor pane's reveal targets.
const CURRENT_FILE: &str = "dunekilo.md";

/// [`CURRENT_FILE`]'s row as OCR reads it.
pub(crate) const CURRENT_TERM: &str = "dunekilo";

/// The file the scenario creates after capture A. Its name sorts directly above
/// [`ABOVE_FILES`], so its row is in view exactly when theirs are. The generator never writes it.
const TRIGGER_FILE: &str = "duneacorn.md";

/// [`TRIGGER_FILE`]'s row as OCR reads it.
pub(crate) const TRIGGER_TERM: &str = "duneacorn";

/// The fewest tree rows above [`ABOVE_FILES`], the root row included, so [`CURRENT_FILE`] sits
/// below the fold of the 720-pixel output.
const MIN_ROWS_ABOVE: usize = 50;

/// The fewest tree rows below [`CURRENT_FILE`], so a reveal scroll has room to place it anywhere
/// in the view rather than against the end of the list.
const MIN_ROWS_BELOW: usize = 150;

/// The fewest files the walk lists, so the traversal streams more than one batch.
const MIN_FIXTURE_FILES: usize = 300;

/// The longest term, so its row fits the sidebar band unellipsized.
const MAX_TERM_LEN: usize = 12;

/// How many tree rows below the root the top-of-tree diagnostic reads.
const TOP_ROWS_PROBED: usize = 8;

/// Root file stems that sort above [`ABOVE_FILES`]: each starts with `a`, `b` or `c`.
const EARLY_STEMS: [&str; 14] = [
    "anchor", "aspen", "atlas", "bamboo", "beacon", "birch", "bramble", "cedar", "cinder",
    "clover", "comet", "copper", "coral", "cobalt",
];

/// Root file stems that sort below [`CURRENT_FILE`]: each starts with a letter after `d`.
const LATE_STEMS: [&str; 45] = [
    "ember", "fable", "fern", "garnet", "glacier", "harbor", "hazel", "iris", "ivory", "jasper",
    "juniper", "kelp", "lantern", "larch", "maple", "meadow", "nectar", "nimbus", "oak", "onyx",
    "orchid", "pebble", "pine", "quartz", "quince", "raven", "reed", "saffron", "sage", "spruce",
    "tamarind", "thistle", "tundra", "umber", "valley", "velvet", "willow", "wisteria", "xenon",
    "yarrow", "yew", "zephyr", "zinnia", "walnut", "violet",
];

/// The four subdirectories, one with a space in its name. Each holds [`DIR_FILES`] files.
const DIRS: [&str; 4] = ["Archive", "daily notes", "templates", "Zettel"];

/// How many Markdown files each of [`DIRS`] holds.
const DIR_FILES: usize = 3;

/// The root file holding a non-ASCII letter and a space.
const NON_ASCII_FILE: &str = "résumé draft.md";

/// The root file of about [`LARGE_FILE_BYTES`].
const LARGE_FILE: &str = "Logbook export.md";

/// [`LARGE_FILE`]'s size, about 400 KB.
const LARGE_FILE_BYTES: usize = 400 * 1024;

/// The body of every generated file but [`CURRENT_FILE`] and [`LARGE_FILE`].
const FILLER_BODY: &str = "# Note\n";

/// [`CURRENT_FILE`]'s body, which the editor and the preview show beside the sidebar.
const CURRENT_BODY: &str = "# Reveal target\n\nThe file the restored editor and preview show.\n";

/// `stem` with its first letter uppercased.
fn capitalized(stem: &str) -> String {
    let mut chars = stem.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Five root file names per stem, in mixed case, some with a space.
fn stem_names(stems: &[&str]) -> Vec<String> {
    stems
        .iter()
        .enumerate()
        .flat_map(|(i, stem)| {
            let cap = capitalized(stem);
            [
                format!("{stem}.md"),
                format!("{cap} notes.md"),
                format!("{}-log.md", stem.to_uppercase()),
                format!("{stem}-{i:02}.md"),
                format!("{cap}_draft.md"),
            ]
        })
        .collect()
}

/// [`LARGE_FILE`]'s body: numbered list lines up to [`LARGE_FILE_BYTES`].
fn large_body() -> String {
    let mut body = String::from("# Logbook\n\n");
    let mut n = 0;
    while body.len() < LARGE_FILE_BYTES {
        body.push_str(&format!("- entry {n:05} of the exported logbook\n"));
        n += 1;
    }
    body
}

/// Every file of the fixture as `(path relative to the project, body)`, in a fixed order.
///
/// `.git/HEAD` is among them, which the walk prunes; [`walk_entries`] models that.
fn reveal_fixture() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = [
        (".obsidian/app.json", "{}\n"),
        (".git/HEAD", "ref: refs/heads/main\n"),
        (".gitignore", "*.tmp\n"),
        (
            ".gitmodules",
            "[submodule \"vendor\"]\n\tpath = vendor\n\turl = https://example.invalid/v.git\n",
        ),
        (".nvmrc", "20\n"),
    ]
    .into_iter()
    .map(|(rel, body)| (rel.to_owned(), body.to_owned()))
    .collect();
    for dir in DIRS {
        for n in 1..=DIR_FILES {
            files.push((format!("{dir}/note {n}.md"), FILLER_BODY.to_owned()));
        }
    }
    let names = stem_names(&EARLY_STEMS)
        .into_iter()
        .chain(ABOVE_FILES.iter().map(|name| (*name).to_owned()))
        .chain(stem_names(&LATE_STEMS))
        .chain([NON_ASCII_FILE.to_owned()]);
    files.extend(names.map(|name| (name, FILLER_BODY.to_owned())));
    files.push((CURRENT_FILE.to_owned(), CURRENT_BODY.to_owned()));
    files.push((LARGE_FILE.to_owned(), large_body()));
    files
}

/// Writes [`reveal_fixture`] into `cwd`.
fn write_reveal_fixture(smoke: &str, cwd: &Path) {
    for (rel, body) in reveal_fixture() {
        let path = cwd.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("{smoke}: failed to create {}: {e}", parent.display()));
        }
        std::fs::write(&path, body)
            .unwrap_or_else(|e| panic!("{smoke}: failed to write {}: {e}", path.display()));
    }
}

/// The [`FileEntry`]s the walk lists for `rels`: every path but those under `.git/`, which
/// `wolf_core`'s walk prunes.
fn walk_entries<'a>(rels: impl IntoIterator<Item = &'a str>) -> Vec<FileEntry> {
    rels.into_iter()
        .filter(|rel| !rel.starts_with(".git/"))
        .map(|rel| FileEntry {
            path: Path::new("/reveal-fixture").join(rel),
            display: rel.to_owned(),
            mru_rank: None,
        })
        .collect()
}

/// The names of the root's children in tree order, which are the tree's rows below the root row
/// while no directory is expanded.
fn root_row_names(entries: &[FileEntry]) -> Vec<String> {
    children_of(entries, "")
        .into_iter()
        .map(|node| match node {
            TreeNode::Dir { name, .. } | TreeNode::File { name, .. } => name,
        })
        .collect()
}

/// Where `name` sits among `rows`, panicking with `what` when it is absent.
fn row_index(rows: &[String], name: &str, what: &str) -> usize {
    rows.iter()
        .position(|row| row == name)
        .unwrap_or_else(|| panic!("{what}: {name:?} is not a root row; the rows are {rows:?}"))
}

/// The leading word of each of the first [`TOP_ROWS_PROBED`] rows below the root, lowercased:
/// what OCR reads when the top of the tree is drawn. A diagnostic, asserted nowhere.
fn top_row_words() -> Vec<String> {
    let fixture = reveal_fixture();
    let entries = walk_entries(fixture.iter().map(|(rel, _)| rel.as_str()));
    root_row_names(&entries)
        .into_iter()
        .take(TOP_ROWS_PROBED)
        .filter_map(|name| {
            name.trim_start_matches('.')
                .split(|c: char| !c.is_ascii_alphabetic())
                .next()
                .map(str::to_lowercase)
        })
        .filter(|word| word.len() >= 5)
        .collect()
}

/// **The fixture's premise: [`ABOVE_FILES`] are the six rows directly above [`CURRENT_FILE`],
/// with [`TRIGGER_FILE`] directly above them once it exists, and each term names one row.**
///
/// It reads the generator's paths as the walk lists them and orders them with
/// `wolf_core::tree::children_of`, the projection the sidebar draws. Its row counts assume the
/// sidebar shows only the root expanded, which holds while the revealed file has no ancestor
/// directory. It guards the premise rather than product code, so it has no falsifier.
#[test]
fn reveal_fixture_puts_its_rows_directly_above_the_current_file() {
    const GUARD: &str = "reveal_fixture_puts_its_rows_directly_above_the_current_file";

    let fixture = reveal_fixture();
    let rels: Vec<&str> = fixture.iter().map(|(rel, _)| rel.as_str()).collect();
    let entries = walk_entries(rels.iter().copied());
    assert!(
        entries.len() >= MIN_FIXTURE_FILES,
        "{GUARD}: the walk lists {} fixture files, fewer than {MIN_FIXTURE_FILES}, so the \
         traversal may stream them in one batch and the smoke stops covering a reveal that races \
         the stream. Add stems to EARLY_STEMS or LATE_STEMS.",
        entries.len()
    );
    assert!(
        rels.iter().all(|rel| !rel.ends_with(".tmp")),
        "{GUARD}: a fixture path ends in `.tmp`, which the fixture's .gitignore hides from the \
         walk; walk_entries does not model that, so every row count below would be wrong."
    );
    let mut distinct = std::collections::HashSet::new();
    for rel in &rels {
        assert!(
            distinct.insert(*rel),
            "{GUARD}: {rel:?} is generated twice, so one write overwrites the other and the \
             fixture holds fewer files than the counts below assume."
        );
    }

    // --- The block, as the launch's tree orders it ------------------------------------------
    let rows = root_row_names(&entries);
    let current = row_index(&rows, CURRENT_FILE, GUARD);
    assert!(
        current >= ABOVE_FILES.len() && rows[current - ABOVE_FILES.len()..current] == ABOVE_FILES,
        "{GUARD}: the rows directly above {CURRENT_FILE:?} are {:?}, not ABOVE_FILES {:?}. A \
         generated name sorts inside the block, or the block's case mix breaks its order; the \
         smoke would then assert on rows that are not the ones beside the revealed file.",
        &rows[current.saturating_sub(ABOVE_FILES.len())..current],
        ABOVE_FILES
    );
    // One for the root row, which sits above every root child.
    let rows_above = 1 + current - ABOVE_FILES.len();
    assert!(
        rows_above >= MIN_ROWS_ABOVE,
        "{GUARD}: {rows_above} tree rows sit above the block, fewer than {MIN_ROWS_ABOVE}, so \
         {CURRENT_FILE:?} may be in view at the top of the tree and the smoke passes without a \
         reveal scroll. Add EARLY_STEMS."
    );
    let rows_below = rows.len() - current - 1;
    assert!(
        rows_below >= MIN_ROWS_BELOW,
        "{GUARD}: {rows_below} tree rows sit below {CURRENT_FILE:?}, fewer than \
         {MIN_ROWS_BELOW}, so the end of the list can hold the revealed row in place whatever \
         the scroll does. Add LATE_STEMS."
    );

    // --- The block once the scenario has created TRIGGER_FILE -------------------------------
    let with_trigger = walk_entries(rels.iter().copied().chain([TRIGGER_FILE]));
    let rows = root_row_names(&with_trigger);
    let current = row_index(&rows, CURRENT_FILE, GUARD);
    let block = current.saturating_sub(ABOVE_FILES.len());
    assert!(
        block >= 1 && rows[block..current] == ABOVE_FILES && rows[block - 1] == TRIGGER_FILE,
        "{GUARD}: with {TRIGGER_FILE:?} added, the seven rows above {CURRENT_FILE:?} are {:?}; \
         want TRIGGER_FILE then ABOVE_FILES. Leg B's trigger term then reads whether or not the \
         block is in view, or is out of view while the block is.",
        &rows[current.saturating_sub(ABOVE_FILES.len() + 1)..current]
    );

    // --- The terms ----------------------------------------------------------------------------
    let named: Vec<(&str, &str)> = ABOVE_FILES
        .iter()
        .copied()
        .zip(ABOVE_TERMS)
        .chain([(CURRENT_FILE, CURRENT_TERM), (TRIGGER_FILE, TRIGGER_TERM)])
        .collect();
    for &(file, term) in &named {
        assert_eq!(
            file.strip_suffix(".md").map(str::to_lowercase).as_deref(),
            Some(term),
            "{GUARD}: {term:?} is not the lowercased stem of {file:?}, so OCR of that row does not \
             read the term the smoke looks for."
        );
        assert!(
            term.len() <= MAX_TERM_LEN && term.chars().all(|c| c.is_ascii_lowercase()),
            "{GUARD}: {term:?} is longer than {MAX_TERM_LEN} or holds more than lowercase ASCII \
             letters, so its row may be ellipsized in the sidebar or misread by OCR."
        );
        for &(other_file, other) in &named {
            assert!(
                other_file == file || !term.contains(other),
                "{GUARD}: {term:?} contains {other:?}, so the row of {file:?} alone satisfies \
                 the assertion on {other_file:?}."
            );
        }
        let all_rels = rels.iter().copied().chain([TRIGGER_FILE]);
        let carriers: Vec<&str> = all_rels
            .filter(|rel| rel.to_lowercase().contains(term))
            .collect();
        assert_eq!(
            carriers,
            vec![file],
            "{GUARD}: {term:?} occurs in the generated names {carriers:?}; want only {file:?}. \
             Another row carrying it satisfies the assertion on {file:?} from wherever it sits."
        );
        for (rel, body) in &fixture {
            assert!(
                !body.to_lowercase().contains(term),
                "{GUARD}: the body of {rel:?} contains {term:?}; keep every term in its name \
                 alone, so only that row can draw it."
            );
        }
    }
}

/// The scenario of [`restored_editor_reveal_draws_the_rows_above_the_current_file_under_cage`],
/// in the shape of `sidebar.rs`'s `TREE_SCENARIO`. It sends no key and no pointer event.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input |
/// | `40` | the app never reported ready |
/// | `12` | the app died before capture A |
/// | `13` | creating `TRIGGER_FILE` failed |
/// | `14` | the app died before capture B |
const REVEAL_SCENARIO: &str = r#"
set -eu

# The restored panes, the traversal's batches and the reveal settling.
REVEAL_SETTLE=3.0
# The watcher's 200 ms debounce, the rescan's walk and the coalesced rebuild.
RESCAN_SETTLE=2.0

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome tree reveal smoke"

sleep "$REVEAL_SETTLE"
kill -0 "$app_pid" 2>/dev/null || exit 12
grim "$OUT_A"

printf '# Trigger\n' > "$TRIGGER_FILE" || exit 13
sleep "$RESCAN_SETTLE"
kill -0 "$app_pid" 2>/dev/null || exit 14
grim "$OUT_B"

set +e
kill "$app_pid" 2>/dev/null
wait "$app_pid" 2>/dev/null
exit 0
"#;

/// The one tab the smoke plants: the tree sidebar shown beside an editor pane (left, focused)
/// and a preview pane (right), both on `current`.
fn reveal_tab(project: &str, current: &str) -> Tab {
    Tab {
        directory: project.to_owned(),
        content: TabContent {
            root: Some(Node::Layout(Layout {
                orientation: PaneOrientation::Vertical,
                weights: vec![0.5, 0.5],
                children: vec![
                    Node::Pane(Pane::Edit {
                        path: current.to_owned(),
                        cursor: None,
                        top_line: None,
                    }),
                    Node::Pane(Pane::Preview {
                        path: current.to_owned(),
                        scroll: None,
                    }),
                ],
            })),
            focused: Some(PaneRef::Pane(0)),
            sidebar: Sidebar {
                visible: true,
                mode: Some(SidebarMode::Tree),
                ..Sidebar::default()
            },
            ..TabContent::default()
        },
    }
}

/// Which of `terms` OCR's `text` lacks.
fn missing_terms<'a>(text: &str, terms: &[&'a str]) -> Vec<&'a str> {
    terms
        .iter()
        .copied()
        .filter(|term| !text.contains(term))
        .collect()
}

/// What the band says about the top of the tree, for a failure message.
fn top_of_tree_reading(text: &str) -> String {
    let top: Vec<String> = top_row_words()
        .into_iter()
        .filter(|word| text.contains(word.as_str()))
        .collect();
    format!(
        "The band reads the root row {TEMP_CWD_BASENAME:?}: {}; it reads these top-of-tree rows: \
         {top:?}.",
        text.contains(TEMP_CWD_BASENAME)
    )
}

/// **A restored tab whose editor pane is active on a file below the fold draws the rows above
/// that file in the sidebar once the launch has settled, with no input, and again after a
/// watcher rescan rebuilds the tree.**
///
/// ## The scenario
///
/// One launch through [`cage_command`] over a planted window of one tab:
/// the tree sidebar beside an editor pane (left, focused) and a preview pane (right), both on
/// [`CURRENT_FILE`], in the [`reveal_fixture`] project. After [`REVEAL_SCENARIO`]'s settle,
/// capture A. The scenario then creates [`TRIGGER_FILE`], waits for the rescan's rebuild, and
/// takes capture B. Each leg OCRs the sidebar band, [`projsearch_sidebar_band`].
///
/// The revealed row is bold and selected, and OCR reads such a row: the project-search reveal
/// smoke asserts its own current file's row the same way.
///
/// ## Assertions
///
/// | ID | assertion | falsifying edit to `tree_view.rs` | predicted red |
/// | --- | --- | --- | --- |
/// | — | precondition: the scenario reached each capture | none | n/a |
/// | A | capture A reads [`ABOVE_TERMS`] and [`CURRENT_TERM`] | `reveal` calls `scroll_to` at once (C1); the tick scrolls on its first frame (C3) | A |
/// | B | capture B reads them and [`TRIGGER_TERM`] | `reveal_target` ignores `rebuilt` (C2) | B |
///
/// Nothing here observes the `map` re-arm, because the launch reveal runs with the list mapped:
/// see `notes/crates/wolf-gnome/tests/what-is-still-unproven.md`.
///
/// At the base commit leg A is red: that is the reproduction.
#[test]
fn restored_editor_reveal_draws_the_rows_above_the_current_file_under_cage() {
    const SMOKE: &str = "restored_editor_reveal_draws_the_rows_above_the_current_file_under_cage";

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    write_reveal_fixture(SMOKE, &cwd);
    let (project, project_str) = canonical_project(SMOKE, &cwd);
    let current = project.join(CURRENT_FILE).to_string_lossy().into_owned();
    let mut window = planted_window(0, vec![reveal_tab(&project_str, &current)]);
    window.frame = Frame {
        x: None,
        y: None,
        width: 1200.0,
        height: 640.0,
    };
    plant_window_state(
        SMOKE,
        &cwd,
        &WindowState {
            windows: vec![window],
        },
    );

    let trigger = cwd.join(TRIGGER_FILE);
    let png_a = runtime_dir.join("wolf_gnome_tree_reveal_a.png");
    let png_b = runtime_dir.join("wolf_gnome_tree_reveal_b.png");
    let band_a = runtime_dir.join("wolf_gnome_tree_reveal_a_band.png");
    let band_b = runtime_dir.join("wolf_gnome_tree_reveal_b_band.png");
    for path in [&trigger, &png_a, &png_b, &band_a, &band_b] {
        clear_evidence(path, SMOKE);
    }

    let status = cage_command(REVEAL_SCENARIO, &cwd, &runtime_dir)
        .env("WOLF_BIN", bin)
        .env("OUT_A", &png_a)
        .env("OUT_B", &png_b)
        .env("TRIGGER_FILE", &trigger)
        .status()
        .expect("failed to spawn `cage` for the headless tree reveal scenario");
    let codes = "Exit 11: the app died before it was ready. 40: it never reported ready. 12: it \
                 died after readiness, before capture A. 13: the scenario could not create the \
                 trigger file. 14: the app died after capture A, before capture B. Any other \
                 non-zero status: grim failed or the window never mapped.";

    let mut above_and_current: Vec<&str> = ABOVE_TERMS.to_vec();
    above_and_current.push(CURRENT_TERM);

    // --- Leg A: the launch reveal ------------------------------------------------------------
    assert!(
        status.success() || matches!(status.code(), Some(13 | 14)),
        "{SMOKE}: the scenario exited {status} before capture A was taken. {codes} Capture A \
         expected at {}.",
        png_a.display()
    );
    assert_png_present(SMOKE, &png_a);
    let text_a = ocr_band_to_lowercase(&png_a, projsearch_sidebar_band(), &band_a, SMOKE);
    let missing = missing_terms(&text_a, &above_and_current);
    assert!(
        missing.is_empty(),
        "{SMOKE}: the sidebar band of capture A lacks {missing:?} of the rows directly above \
         {CURRENT_FILE:?} and that row itself. The causes, in order: (1) the list stayed at the \
         top of the tree, fully drawn: the reveal's scroll never ran, or a rebuild after it \
         dropped it; (2) the rows below the root were drawn blank in their reserved space: the \
         scroll ran before the list was laid out; (3) {CURRENT_TERM:?} reads but the rows above \
         it do not: the row was aligned to the top of the view rather than scrolled into it; (4) \
         the editor pane was not active at launch, so nothing was revealed and the list shows \
         its top; (5) an OCR misread: read the retained crop. {} Capture at {}, band crop at \
         {}.\n\nBand OCR was:\n{text_a}",
        top_of_tree_reading(&text_a),
        png_a.display(),
        band_a.display()
    );

    // --- Leg B: the rescan's rebuild ---------------------------------------------------------
    assert!(
        status.success(),
        "{SMOKE}: the scenario exited {status} after capture A, so leg B has no capture. {codes} \
         Capture B expected at {}.",
        png_b.display()
    );
    assert_png_present(SMOKE, &png_b);
    let text_b = ocr_band_to_lowercase(&png_b, projsearch_sidebar_band(), &band_b, SMOKE);
    let missing = missing_terms(&text_b, &above_and_current);
    assert!(
        missing.is_empty(),
        "{SMOKE}: the sidebar band of capture B, taken after {TRIGGER_FILE:?} was created, lacks \
         {missing:?}, though capture A read them all. The causes, in order: (1) the rescan's \
         rebuild left the list at its first row and nothing revealed the current file again; \
         (2) {CURRENT_TERM:?} reads but the rows above it do not: the re-reveal scrolled before \
         the rebuilt rows were laid out, which aligns the row to the top of the view; (3) an OCR \
         misread: read the retained crop. {} Capture at {}, band crop at {}.\n\nBand OCR \
         was:\n{text_b}",
        top_of_tree_reading(&text_b),
        png_b.display(),
        band_b.display()
    );
    assert!(
        text_b.contains(TRIGGER_TERM),
        "{SMOKE}: the sidebar band of capture B reads every row above {CURRENT_FILE:?} but not \
         {TRIGGER_TERM:?}, the row of the file created after capture A, which sorts directly \
         above them. The rescan never landed, so no rebuild ran before capture B and leg B \
         proved nothing about one: check that the watcher runs in the cage (WOLF_FS_WATCH) and \
         that RESCAN_SETTLE covers the debounce and the walk. Or an OCR misread: read the crop \
         at {}.\n\nBand OCR was:\n{text_b}",
        band_b.display()
    );
}
