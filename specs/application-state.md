# Application state

What Wolf remembers between launches, where it keeps it, and how it decides what to save and what to restore. This spec owns the state database, the state model, the save and restore rules, the rule that every tab runs its own Neovim, and how a new window is sized and placed. The core↔front-end contract these rules travel over is in [architecture.md](architecture.md).

## 1. Storage

All persistent state lives in one SQLite database, `wolf.sqlite`, in `wolf_core::state_dir()`: `~/Library/Application Support/Wolf` on macOS and `~/.local/state/wolf` on Linux, with `WOLF_STATE_DIR` overriding both.

| Table           | Holds                                                                            | Class       |
| --------------- | -------------------------------------------------------------------------------- | ----------- |
| `file_mru`      | Recently opened files, each tagged with its project directory                    | cache       |
| `folder_mru`    | Recently used directories, capped at `RECENT_DIRS_LIMIT`                         | cache       |
| `command_mru`   | Recently run palette commands                                                    | cache       |
| `window_state`  | One row: the window state, as JSON                                               | state       |
| `project_state` | One row per directory: the directory, `last_used`, and the project state as JSON | state       |
| `schema`        | One row per table: its name and schema version                                   | bookkeeping |

The window state and each project state are JSON columns rather than normalized tables, because each tree is always read and written whole. Splitting tabs, layouts and panes into rows would add joins and buy nothing.

### Versioning and recovery

Each table carries its own version in `schema`, so one table's schema change never touches another's rows. What a mismatch does depends on the table's class:

- A cache table at any other version is dropped and recreated. Caches are disposable by design.
- A state table at an older version is migrated in place.
- A state table at a newer version, written by a newer build, is neither read nor written by this build for the rest of the process. The launch behaves as if nothing were saved, and the newer build's rows survive.

Only a database that is unusable as a file, `SQLITE_CORRUPT` or `SQLITE_NOTADB`, is deleted, together with its `-wal` and `-shm` sidecars, and recreated. A busy or locked database is never deleted: the read degrades to empty and the write is dropped, because deleting it would unlink the file from under every other live connection.

The connection installs `busy_timeout` before setting `journal_mode=WAL`, for the reason the `db.rs` `connect` doc comment gives.

### Files Wolf removes

At launch Wolf deletes `state.json`, `state.json.tmp`, `file-mru.sqlite`, `folder-mru.sqlite` and `command-mru.sqlite` from the state directory, with their `-wal` and `-shm` sidecars, when they exist. None of their contents is imported.

### Size limits and pruning

`project_state` keeps at most `PROJECT_STATE_LIMIT` rows, 200. Each write sets the row's `last_used`, and so does Open Folder reading it; the write transaction then deletes the rows beyond the limit, least recently used first.

A project state is never pruned because its directory has vanished. A directory on an unmounted volume looks deleted, and the size limit already bounds the table. `folder_mru` hides a vanished directory when it is read, and `file_mru` deletes a vanished file's row when it is read.

## 2. The state model

```json
{
  "windows": [
    {
      "frame": { "x": 120, "y": 80, "width": 1400, "height": 900 },
      "active_tab": 0,
      "tabs": [
        {
          "directory": "/home/user/dev/project",
          "root": {
            "type": "layout",
            "orientation": "vertical",
            "weights": [0.5, 0.5],
            "children": [
              {
                "type": "pane",
                "kind": "edit",
                "path": "/home/user/dev/project/README.md",
                "cursor": { "line": 12, "col": 4 },
                "top_line": 1
              },
              {
                "type": "pane",
                "kind": "preview",
                "path": "/home/user/dev/project/README.md",
                "scroll": 0.25
              }
            ]
          },
          "focused": 0,
          "zoomed": null,
          "sidebar": {
            "visible": true,
            "mode": "search",
            "query": "TODO",
            "case_sensitive": false,
            "use_regex": false,
            "all_files": false
          },
          "drawer": { "open": true, "share": 0.33 }
        }
      ]
    }
  ]
}
```

### Pane

A pane is one leaf of a tab's tree. `kind` is exactly one of `edit`, `preview` or `terminal`, and `path` means something different for each:

- `edit` — the buffer's absolute file path, or `""` for a buffer that is not a file, such as an unnamed buffer, help or quickfix. It carries `cursor`, with `line` 1-based and `col` a 0-based byte column as `nvim_win_get_cursor` reports them, and `top_line`, the 1-based first visible line.
- `preview` — the absolute path of the Markdown source. It carries `scroll`, the fraction of the page's scrollable height above the viewport, from `0` to `1`.
- `terminal` — the directory the shell starts in. A shell process cannot be restored, so it carries nothing else.

### Layout

A layout splits its area along `orientation`, which uses `PaneOrientation`'s names. It holds its `children` in order, left to right or top to bottom, and a parallel `weights` list; each child is a layout or a pane.

### Tab

A tab holds one root node, a layout or a lone pane, plus:

- `directory` — the absolute canonical path of its Neovim's working directory. A tab with `directory` `""` is an empty tab, the front-end's "no folder" state, and its `root` is `null`. A tab inside a project state has no `directory`, because the project supplies it.
- `focused` and `zoomed` — pane references. A reference is a pane's index among the root's panes in depth-first order, or `"drawer"`. A reference names a position rather than a path because the same file can be open in two panes. `zoomed` is `null` when nothing is zoomed.
- `sidebar` — whether the sidebar is visible, which mode it shows (`tree` or `search`), and the search panel's query and its three toggles, which mirror `SearchOptions`. Expanded tree folders are not stored.
- `drawer` — whether the terminal drawer is open, and `share`, the fraction of the pane area's height it takes.

### Window

A window holds one or more tabs in tab-bar order, even when the tab bar is hidden, `active_tab`, the index of the selected one, and `minimized`, which is `true` for a minimized window and omitted otherwise. `frame` is the window's normal frame: its un-maximized, non-fullscreen geometry, in the writing front-end's coordinates (AppKit's bottom-left global coordinates on macOS). A platform that cannot report a window's position writes `x` and `y` as `null`, and restore ignores them; on GNOME this is always the case, since a Wayland client is never told its position.

### Window state

The window state holds every open window in stacking order, frontmost first, with minimized windows behind all others. The active window is the first one, and nothing else records it. Several windows and tabs can share a directory.

### Project state

A project state is a directory plus one window holding exactly one tab. Its JSON holds that window's `frame` and its tab as `tab`, with no `active_tab`, which a one-tab window does not need; the directory is the row's key. There is at most one per directory. It is what Open Folder restores, and it is never read at launch.

## 3. Stacking order and tab recency

Each front-end keeps the window stacking order itself, from focus changes, rather than querying the window system; on Wayland there is nothing to query. A window that gains focus moves to the front, and a window that is minimized moves behind every window that is not. Restore is the one other writer: it seeds the order from the window state.

Each window also keeps its tabs in order of recency, most recently selected first. Only the selection rule in §4 reads it, so it is not persisted; at launch it starts with the active tab first and the rest in tab order.

## 4. Saving

The window state is written whenever its tree changes, through a writer that throttles and coalesces, and an unchanged tree writes nothing. A tab whose live state cannot be read yet, such as one whose Neovim is dead or whose panes are not bound, contributes its last known state rather than an empty one. Otherwise one launch that has not finished restoring would overwrite a saved layout with nothing.

### Cursor and scroll capture

Cursor, `top_line` and preview `scroll` change constantly, so they are captured at fixed moments rather than on every change: when focus moves to another pane, when a tab is switched away from, when a window closes, and at quit. A write carries the most recently captured values. At quit, every tab is captured before any Neovim exits, then the final write happens, then the Neovims shut down. A crash loses the movement since the last capture.

### One transaction per write

Every write is one transaction that replaces the window state, writes the project state of every directory that has a tab open, and enforces `PROJECT_STATE_LIMIT`. The window state and the project states therefore never disagree about a moment.

### Which tab becomes a directory's project state

For each directory `D` with at least one tab open, the stored tab is:

1. the active tab of the frontmost window whose active tab is in `D`; or, when no window's active tab is in `D`,
2. the most recently selected tab in `D` of the frontmost window holding a tab in `D`.

The stored window is that tab alone, with the frame of the window it came from. A directory with no tab open keeps its stored row unchanged: absence never deletes a project state. An empty tab never produces one.

A tab's directory is its Neovim's working directory, so `:cd` moves a tab to another project. The next write can then replace the destination's project state with panes from the directory the tab left, and that is accepted.

## 5. Restoring at launch

Launch reads the window state and recreates every window in stacking order, the first one frontmost and focused, and minimized windows minimized. The project state plays no part.

- A frame that intersects no screen is discarded on macOS, and the window is placed as a new one (§7). GNOME applies the size alone.
- A tab whose directory is missing restores as an empty tab.
- An `edit` pane whose file is missing, or a `preview` pane whose source is missing, restores as an empty pane in the same position.
- A `terminal` pane, and an open drawer, start a fresh shell in the saved directory, falling back to the tab's directory when it is gone.
- A cursor beyond the end of its file is clamped. A preview's `scroll` applies once the page has rendered.
- `focused` and `zoomed` restore with their panes. A zoomed pane that restored empty, or whose zoom would hide nothing, restores unzoomed.

## 6. Opening a folder

Open Folder, Open Recent included, is the only reader of `project_state`. For a directory `D`:

1. If any tab is open on `D`, Wolf focuses it rather than opening another. It chooses the tab by the rule in §4, raises its window and selects it.
2. Otherwise, if the current tab is empty, Wolf fills it with `D`'s stored tab. When that tab is its window's only tab, the stored frame applies to the window as well.
3. Otherwise Wolf opens a new window from `D`'s stored window and frame.

When `D` has no project state, steps 2 and 3 use a fresh tab on `D`, and a new window is sized by §7. Opening a folder never replaces a tab that has a directory. `:cd` inside Neovim is not Open Folder and reads no project state.

## 7. New windows

A new window with no stored frame to apply, from New Window or from Open Folder without a project state, takes the active window's normal frame size: never its maximized or fullscreen size. It always opens as a normal window. With no active window it opens at the front-end's default size.

Placement belongs to the platform. GNOME leaves it to the compositor. macOS has no window manager that places windows, so Wolf cascades the new window from the active window with `cascadeTopLeft(from:)`.

## 8. One Neovim per tab

Every tab runs its own captive Neovim, on both platforms. It starts when the tab is created or restored, background tabs included, and ends when the tab closes. Tabs never share a Neovim, even when they share a directory, so a `:cd` in one tab moves only that tab, and a Neovim crash restarts only its own tab.

The cost is that tabs share nothing live: no registers, undo history or jump list, beyond what Neovim's shada file merges when an instance exits. Each tab also runs its own copy of every language server.

Two tabs can edit the same file, and the captive `runtime/nvim/init.lua` makes that quiet:

- `shortmess+=A` suppresses the swap-file prompt (`E325`), which would otherwise stop a captive Neovim at an unanswerable prompt. Swap files stay on, because crash recovery depends on them ([architecture.md](architecture.md) §8).
- `autoread` is on, and each tab's Neovim runs `:checktime` when the tab gains focus, the app becoming active included, so a file one tab saves reloads in another that has no unsaved changes.

Unsaved changes in both tabs remain an ordinary Neovim write conflict, which Neovim reports to whichever tab saves second.
