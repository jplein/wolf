//! The Split Horizontal and Split Vertical smokes: each command run from the palette in the
//! real app, from an editor pane, a terminal pane and a preview pane, read back from the
//! frozen `state.json` layout.

use super::terminal::pin_terminal_smoke_shell;
use super::*;

/// Typed into the palette to reach the Split Vertical row: a subsequence of its title and of no
/// other, which [`assert_split_queries_are_unique`] checks before launch.
const SPLIT_VERTICAL_QUERY: &str = "splitvert";

/// Typed into the palette to reach the Split Horizontal row, as [`SPLIT_VERTICAL_QUERY`].
const SPLIT_HORIZONTAL_QUERY: &str = "splithor";

/// The registry id of the command [`SPLIT_VERTICAL_QUERY`] must select.
const SPLIT_VERTICAL_ID: &str = "split_vertical";

/// The registry id of the command [`SPLIT_HORIZONTAL_QUERY`] must select.
const SPLIT_HORIZONTAL_ID: &str = "split_horizontal";

/// The Markdown file the editor and preview smokes split. No smoke here takes a capture, so
/// its name is on no page an assertion reads.
const SPLIT_SOURCE_FILE: &str = "splitsrc.md";

/// The shape every smoke's frozen layout must have, in [`layout_shape`]'s notation: the first
/// split side by side with the new pane on the right, and the second stacking a pane under it.
const SPLIT_SHAPE_EXPECTED: &str = "vertical[leaf, horizontal[leaf, leaf]]";

/// The kind of pane each smoke starts in, which the scenario reads as `SPLIT_KIND`.
#[derive(Clone, Copy, Debug)]
enum FirstPane {
    /// `:e` of [`SPLIT_SOURCE_FILE`] in the launch pane.
    Editor,
    /// The `WOLF_SMOKE_TERMINAL` hook's `/bin/sh` session in the launch pane.
    Terminal,
    /// [`SPLIT_SOURCE_FILE`] opened, then the palette's Preview, which binds the preview into
    /// the launch pane.
    Preview,
}

impl FirstPane {
    /// The `SPLIT_KIND` value the scenario switches on.
    fn kind(self) -> &'static str {
        match self {
            FirstPane::Editor => "editor",
            FirstPane::Terminal => "terminal",
            FirstPane::Preview => "preview",
        }
    }
}

/// The scenario every split smoke runs: one launch, the first pane made per `SPLIT_KIND`, then
/// Split Vertical and Split Horizontal from the palette, each awaited in the live `state.json`,
/// which is then frozen into `$STATE_COPY`.
///
/// Leaves are counted on the pretty-printed `state.json`, where `wolf_core`'s
/// `serde_json::to_string_pretty` puts each `"type": "leaf"` on a line of its own. The layout
/// is persisted only once every leaf is reported, so a count reached is a whole layout.
///
/// ## Exit codes
///
/// | code | meaning |
/// | --- | --- |
/// | `11` | the app died before it was ready for input (the shared convention) |
/// | `40` | it never became ready (the shared convention) |
/// | `61` | the palette's Preview left no preview leaf in `state.json` |
/// | `62` | the layout never reached two leaves after Split Vertical |
/// | `63` | the layout never reached three leaves after Split Horizontal |
const SPLIT_SCENARIO: &str = r#"
set -eu

# SETTLE is the settle between steps; see SCENARIO.
WAIT_INTERVAL=0.2
SETTLE=1.0
FILE_TRIES=100

cleanup() {
    set +e
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null
        wait "$app_pid" 2>/dev/null
    fi
}
trap cleanup EXIT INT TERM HUP

"$WOLF_BIN" &
app_pid=$!
wait_ready "$app_pid" 11 "wolf-gnome split-command smoke"

# $1 is a line pattern, $2 the count of state.json lines that must match it, $3 the exit code.
wait_state_lines() {
    t=0
    while [ "$t" -lt "$FILE_TRIES" ]; do
        n=$(grep -c "$1" "$STATE_SRC" 2>/dev/null || true)
        if [ "${n:-0}" -eq "$2" ]; then
            return 0
        fi
        t=$((t + 1))
        sleep "$WAIT_INTERVAL"
    done
    echo "wolf-gnome split-command smoke: state.json never held $2 lines matching $1." >&2
    if [ -s "$STATE_SRC" ]; then
        cat "$STATE_SRC" >&2
    fi
    exit "$3"
}

# $1 is the query; Enter runs the palette's top row.
palette() {
    wtype -M ctrl -M shift -k p -m shift -m ctrl
    sleep "$SETTLE"
    wtype -- "$1"
    sleep "$SETTLE"
    wtype -k Return
}

# --- Step 1: the first pane ---------------------------------------------------
# A terminal pane is already there: WOLF_SMOKE_TERMINAL opened it before wait_ready.
if [ "$SPLIT_KIND" != terminal ]; then
    wtype -- ":e $SPLIT_SOURCE"
    wtype -k Return
    sleep "$SETTLE"
fi
if [ "$SPLIT_KIND" = preview ]; then
    palette "$PREVIEW_QUERY"
    wait_state_lines '"preview": true' 1 61
    sleep "$SETTLE"
fi

# --- Step 2: Split Vertical, then Split Horizontal in the pane it focused --------
palette "$SPLIT_VERTICAL_QUERY"
wait_state_lines '"type": "leaf"' 2 62
sleep "$SETTLE"
palette "$SPLIT_HORIZONTAL_QUERY"
wait_state_lines '"type": "leaf"' 3 63

# --- Step 3: freeze the layout, once its third leaf's content has settled ------
sleep "$SETTLE"
cp "$STATE_SRC" "$STATE_COPY"

exit 0
"#;

/// The layout's structure with every leaf written `leaf`, as `vertical[leaf, leaf]`.
fn layout_shape(node: &PaneNode) -> String {
    match node {
        PaneNode::Leaf { .. } => "leaf".to_owned(),
        PaneNode::Split {
            orientation,
            children,
            ..
        } => {
            let name = match orientation {
                PaneOrientation::Horizontal => "horizontal",
                PaneOrientation::Vertical => "vertical",
            };
            let inner: Vec<String> = children.iter().map(layout_shape).collect();
            format!("{name}[{}]", inner.join(", "))
        }
    }
}

/// Every leaf of `node`, in order.
fn split_leaves<'a>(node: &'a PaneNode, out: &mut Vec<LeafFields<'a>>) {
    match node {
        PaneNode::Split { children, .. } => {
            for child in children {
                split_leaves(child, out);
            }
        }
        leaf => out.extend(leaf_fields(leaf)),
    }
}

/// Checks each palette query selects exactly its command's title as a case-insensitive
/// subsequence, so which row Enter runs rests on no ranking.
fn assert_split_queries_are_unique(smoke: &str) {
    let is_subsequence = |query: &str, title: &str| {
        let mut chars = title.chars().map(|c| c.to_ascii_lowercase());
        query.chars().all(|q| chars.any(|c| c == q))
    };
    for (query, id) in [
        (SPLIT_VERTICAL_QUERY, SPLIT_VERTICAL_ID),
        (SPLIT_HORIZONTAL_QUERY, SPLIT_HORIZONTAL_ID),
    ] {
        let matched: Vec<(&str, &str)> = wolf_core::COMMAND_SPECS
            .iter()
            .filter(|spec| is_subsequence(query, spec.title))
            .map(|spec| (spec.id, spec.title))
            .collect();
        assert!(
            matched.len() == 1 && matched[0].0 == id,
            "{smoke}: the palette query {query:?} must match exactly the {id:?} command's title \
             as a case-insensitive subsequence; it matches {matched:?}. No match means the \
             command is not registered; more than one means Enter runs whichever ranks first, \
             so the smoke could split with the wrong command."
        );
    }
}

/// Runs [`SPLIT_SCENARIO`] from `first` and returns the frozen layout's leaves after checking
/// its shape.
///
/// ## Assertions
///
/// | # | assertion | kind |
/// | --- | --- | --- |
/// | 1 | each query selects exactly its command | precondition, checked before launch |
/// | 2 | the scenario exits 0 | discriminator for a command the palette does not run |
/// | 3 | the frozen layout is [`SPLIT_SHAPE_EXPECTED`] | discriminator for the orientations |
///
/// Each smoke then asserts its leaves' kind.
fn run_split_smoke(smoke: &str, first: FirstPane) -> Vec<(String, bool, bool)> {
    assert_split_queries_are_unique(smoke);

    let runtime_dir = require_smoke_env();
    let bin = env!("CARGO_BIN_EXE_wolf-gnome");
    let cwd = make_temp_cwd();
    let source = cwd.join(SPLIT_SOURCE_FILE);
    std::fs::write(&source, "# Split source\n\nA paragraph to show.\n")
        .unwrap_or_else(|e| panic!("{smoke}: failed to write {}: {e}", source.display()));

    let smoke_state_file = smoke_state_dir(&cwd).join(STATE_FILE_NAME);
    let frozen_state_dir = runtime_dir.join(format!("wolf_gnome_split_{}_state", first.kind()));
    std::fs::create_dir_all(&frozen_state_dir).unwrap_or_else(|e| {
        panic!(
            "{smoke}: failed to create the frozen-state dir {}: {e}",
            frozen_state_dir.display()
        )
    });
    let frozen_state_file = frozen_state_dir.join(STATE_FILE_NAME);
    clear_evidence(&frozen_state_file, smoke);

    let mut cmd = cage_command(SPLIT_SCENARIO, &cwd, &runtime_dir);
    cmd.env("WOLF_BIN", bin)
        .env("SPLIT_KIND", first.kind())
        .env("SPLIT_SOURCE", SPLIT_SOURCE_FILE)
        .env("PREVIEW_QUERY", super::preview::PREVIEW_QUERY)
        .env("SPLIT_VERTICAL_QUERY", SPLIT_VERTICAL_QUERY)
        .env("SPLIT_HORIZONTAL_QUERY", SPLIT_HORIZONTAL_QUERY)
        .env("STATE_SRC", &smoke_state_file)
        .env("STATE_COPY", &frozen_state_file);
    if let FirstPane::Terminal = first {
        pin_terminal_smoke_shell(&mut cmd).env("WOLF_SMOKE_TERMINAL", "/bin/sh");
    }
    let status = cmd
        .status()
        .expect("failed to spawn `cage` for the headless split-command scenario");

    // --- Assertion 2: the scenario ran to completion --------------------------
    assert!(
        status.success(),
        "{smoke}: the cage split-command scenario exited unsuccessfully ({status}); cage.log's \
         stderr names the step and prints the last state.json. Exit 11: the app died before it \
         was ready for input. Exit 40: it never became ready. Exit 61: the palette's Preview \
         left no preview leaf, so the preview pane the smoke splits never existed. Exit 62: \
         Split Vertical from the palette added no pane: its row is missing from the palette \
         (`ui_action_for` not classifying it as dispatched) or `perform` does nothing with it. \
         Exit 63: Split Horizontal added no pane, for the same reasons. Otherwise wtype or cp \
         failed."
    );

    // --- Assertion 3: the shape names both orientations ------------------------
    let saved = load_state(&frozen_state_dir).unwrap_or_else(|| {
        panic!(
            "{smoke}: wolf_core::load_state could not parse the frozen state at {}; the scenario \
             saw it hold three leaves, so suspect its shape.",
            frozen_state_file.display()
        )
    });
    let layout = saved
        .windows
        .first()
        .and_then(|window| window.tabs.first())
        .and_then(|tab| tab.layout.as_ref())
        .unwrap_or_else(|| {
            panic!(
                "{smoke}: the frozen state.json carries no layout in its first window's first \
                 tab. File: {}",
                frozen_state_file.display()
            )
        });
    let shape = layout_shape(layout);
    assert_eq!(
        shape,
        SPLIT_SHAPE_EXPECTED,
        "{smoke}: Split Vertical then Split Horizontal in the pane it focused must leave \
         {SPLIT_SHAPE_EXPECTED:?}, not {shape:?}. `horizontal[leaf, vertical[leaf, leaf]]` means \
         the two commands are swapped: Split Horizontal is Vim's `:split`, stacked, and Split \
         Vertical `:vsplit`, side by side. The new pane on the left, or the second split taking \
         the first pane, means the split did not focus its new pane. File: {}",
        frozen_state_file.display()
    );

    let mut leaves = Vec::new();
    split_leaves(layout, &mut leaves);
    leaves
        .into_iter()
        .map(|leaf| (leaf.path.to_owned(), leaf.preview, leaf.terminal))
        .collect()
}

/// The file name of a persisted leaf path, or `""`.
fn path_tail(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
}

/// **Split Vertical and Split Horizontal from the palette split an editor pane into windows
/// on its buffer, in Vim's orientations.**
///
/// Falsified by swapping the two orientations in `shell.rs`'s `perform` (assertion 3) and by
/// classifying Split Vertical as deferred in `command_action.rs` (exit 62).
#[test]
fn split_commands_split_an_editor_pane_under_cage() {
    const SMOKE: &str = "split_commands_split_an_editor_pane_under_cage";
    let leaves = run_split_smoke(SMOKE, FirstPane::Editor);
    assert!(
        leaves.iter().all(|(path, preview, terminal)| {
            path_tail(path) == SPLIT_SOURCE_FILE && !preview && !terminal
        }),
        "{SMOKE}: every pane must be an editor on {SPLIT_SOURCE_FILE}, as <C-w>s and <C-w>v \
         give; the leaves are {leaves:?} (path, preview, terminal). Another path, or an empty \
         one, means a split pane shows a buffer other than the one it split."
    );
}

/// **Split Vertical and Split Horizontal from the palette split a terminal pane into terminal
/// panes.**
///
/// `state.json` carries no session ids, so this does not tell a new session from a shared one;
/// wolf-core's `split_commands.rs` does.
#[test]
fn split_commands_split_a_terminal_pane_under_cage() {
    const SMOKE: &str = "split_commands_split_a_terminal_pane_under_cage";
    let leaves = run_split_smoke(SMOKE, FirstPane::Terminal);
    assert!(
        leaves
            .iter()
            .all(|(_, preview, terminal)| *terminal && !preview),
        "{SMOKE}: every pane must be a terminal pane; the leaves are {leaves:?} (path, preview, \
         terminal). An editor leaf means a terminal pane was split as a Neovim window."
    );
}

/// **Split Vertical and Split Horizontal from the palette split a preview pane into previews
/// of the same source.**
///
/// Falsified by making the split open an empty buffer in its new window, which leaves editor
/// leaves where previews belong.
#[test]
fn split_commands_split_a_preview_pane_under_cage() {
    const SMOKE: &str = "split_commands_split_a_preview_pane_under_cage";
    let leaves = run_split_smoke(SMOKE, FirstPane::Preview);
    assert!(
        leaves.iter().all(|(path, preview, terminal)| {
            path_tail(path) == SPLIT_SOURCE_FILE && *preview && !terminal
        }),
        "{SMOKE}: every pane must preview {SPLIT_SOURCE_FILE}; the leaves are {leaves:?} (path, \
         preview, terminal). A leaf with preview false means a split of a preview pane showed \
         something other than the preview, which a front-end draws as an editor."
    );
}
