# The `wolf` command and opening files

How a path reaches a running Wolf from outside it: the `wolf` command, a file or folder opened from Finder or GNOME Files, and `wolf --wait` as an editor for other programs. This spec owns the command's interface, the channel it uses, the rules that choose a project, a tab and a pane for a path, and the wait. Opening a folder, choosing a directory's tab, and restoring a session are [application-state.md](application-state.md)'s, which this spec cites rather than restates.

## 1. The command

```text
wolf [--wait] [PATH]
```

| Invocation         | What it does                                                                     | Exit status                   |
| ------------------ | -------------------------------------------------------------------------------- | ----------------------------- |
| `wolf`             | Launches Wolf, or activates it when it is running                                | `0` once Wolf has the request |
| `wolf DIR`         | Opens the directory (§4)                                                         | `0` once Wolf has the request |
| `wolf FILE`        | Opens the file (§5)                                                              | `0` once Wolf has the request |
| `wolf --wait FILE` | Opens the file, then waits (§6)                                                  | `0`, or `1` (§6)              |
| anything else      | Refused: `--wait` without a file, `--wait DIR`, several paths, an unknown option | `2`, with a message on stderr |

A relative `PATH` resolves against the command's working directory. A path that exists as a directory is a directory; any other path is a file, including one that does not exist yet. A command that cannot reach Wolf exits `2`.

One `wolf` binary, written in Rust, serves both platforms. Wolf terminals set `WOLF_TAB_ID` (§3) and nothing else for it: setting `EDITOR` or `GIT_EDITOR` to `wolf --wait` is left to the user's shell configuration, which would override anything Wolf set.

### Installing it

- **macOS:** the binary ships inside `Wolf.app`. The application menu has "Install `wolf` Command" and "Uninstall `wolf` Command", modelled on Visual Studio Code's `code` commands. Install creates the symbolic link `/usr/local/bin/wolf` to the bundled binary, asking for administrator rights when the directory is not writable; uninstall removes that link and nothing else.
- **Linux:** the package ships `bin/wolf` beside `bin/wolf-gnome`.

## 2. The channel

The running app listens on a Unix-domain socket in `wolf_core::state_dir()`. The command connects, sends one request, and reads the replies; the app answers on both platforms through the same `wolf-core` code. One channel on both platforms, rather than GApplication's command-line forwarding on Linux and Apple Events on macOS, keeps `WOLF_TAB_ID`, the working directory, `--wait` and the exit status identical everywhere, and testable without a display. Apple Events carry neither the environment nor a wait.

A request carries the absolute path or none, whether to wait, and `WOLF_TAB_ID` when set. The app replies once it has taken the request, and, for a wait, once more when the wait ends, with its outcome. A connection that closes before that final reply ends the wait as an abnormal close (§6).

A socket path is limited to 104 bytes on macOS, the terminating null byte included, so on both platforms a socket path over 103 bytes is an error the command and the app both report, rather than a silent fallback elsewhere.

### When Wolf is not running

The command launches it, on macOS through Launch Services by bundle identifier and on Linux by starting the `wolf-gnome` beside it, then retries the connection until the app listens or a timeout passes. A cold launch follows the platform's convention: it restores the last session ([application-state.md](application-state.md) §5) and only then handles the request.

## 3. Tab IDs

Every tab has an ID, a random version-4 `uuid`, fresh for each tab created or restored and never persisted, so no ID outlives its tab or is reused by a later launch. Every terminal session started in a tab, its terminal panes and its drawer alike, carries the tab's ID in `WOLF_TAB_ID`. A crash restart of the tab's Neovim keeps the ID, because the tab is the same tab.

A request whose `WOLF_TAB_ID` names no live tab is handled as if the variable were unset. That covers a closed tab, a relaunched Wolf, and a process that inherited the variable and outlived its terminal, such as a tmux server.

## 4. Opening a directory

`wolf DIR` and a folder opened from Finder or GNOME Files are Open Folder ([application-state.md](application-state.md) §6) for `DIR`. "The current tab" there is the tab `WOLF_TAB_ID` names when it is live, and otherwise the frontmost window's active tab.

## 5. Opening a file

Opening a file `F` chooses a tab (§5.1), places `F` in a pane of it (§5.2), and activates the tab's window, the tab and that pane. A file opened from Finder or GNOME Files follows the same rules, with no `WOLF_TAB_ID`. A file that does not exist opens as a new, unsaved buffer for that path.

### 5.1 Choosing the tab

1. **`WOLF_TAB_ID` names a live tab:** that tab, whatever project `F` belongs to.
2. **Otherwise, find the project directory `P`.** Walk up from `F`'s directory, or from its nearest existing ancestor when it does not exist, to the first directory holding a `.git`, which can be a directory or, in a linked worktree or a submodule, a file. Call it the repository root `R`. Then `P` is the first of:
   1. the nearest directory that is open in a tab, contains `F`, and lies inside `R`, `R` included;
   2. the nearest directory above `R` that is open in a tab, unless `R` is a linked worktree of that directory's repository, meaning `git -C R rev-parse --git-common-dir` resolves to that repository's git directory; a submodule therefore resolves to its open superproject, and a worktree never does;
   3. `R` itself.

   With no repository above `F`, `P` is the nearest directory open in a tab that contains `F`, or else `F`'s directory, or its nearest existing ancestor when it does not exist. "Open in a tab" means a tab is on that directory at the time of the request, not that a project state is stored.

3. **Then the tab.** When `P` is open in a tab, Wolf chooses one by the rule that chooses a directory's project state ([application-state.md](application-state.md) §4, "Which tab becomes a directory's project state"). Otherwise it opens `P` as Open Folder does (§6 there), and `F` goes into the tab that results.

A pane on `F` in a tab other than the chosen one plays no part.

### 5.2 Choosing the pane

In the chosen tab, a zoomed pane is unzoomed first. If an editor pane already shows `F`, Wolf focuses it: the active pane when it is one of them, else the first in depth-first order. A preview of `F` does not count. Otherwise `F` opens in the first of:

1. the tab's active pane, if it is an editor pane;
2. the leftmost editor pane;
3. the leftmost editor or preview pane, which becomes an editor pane showing `F`;
4. a new editor pane made by splitting the whole tab vertically, with the new pane on the left and everything the tab held on the right.

A leftmost pane is one whose left edge is the pane area's left edge; among several, the largest wins, and the topmost among equals. The terminal drawer is never a candidate, and neither is a terminal pane, so the terminal that ran `wolf` is never replaced.

## 6. Waiting

`wolf --wait FILE` returns when the buffer it opened is closed, so that `EDITOR="wolf --wait"` lets a program such as git hand a file to Wolf and resume when the user is done. If `F` was already shown in a pane, the wait is on that buffer.

The buffer counts as closed when its last window closes, as with `:q`, `:wq`, `ZZ` or `:close`, or when it is deleted or wiped, as with `:bd` or `:bw`. Showing another buffer in its window, as with `:e` or `:b`, only hides it, and the wait goes on; Wolf then shows a toast saying that `wolf --wait` is still waiting for that file.

| Outcome                                                                                                          | Exit status |
| ---------------------------------------------------------------------------------------------------------------- | ----------- |
| The buffer closed, including by a close that also closes its tab or window, such as `:wq` on the tab's last pane | `0`         |
| `:cq` in a window showing the waited buffer                                                                      | `1`         |
| The tab, window or app closed by any other route, or the tab's Neovim crashed                                    | `1`         |

`:cq` in a window showing a waited buffer closes that buffer and does not quit Neovim, because a captive Neovim that quits is a crash Wolf restarts. Anywhere else, `:cq` keeps its ordinary meaning.

## 7. Finder and GNOME Files

Wolf offers to open Markdown files and folders, without claiming to be the default for either.

- **macOS:** `Info.plist` declares Markdown (`net.daringfireball.markdown`, imported as conforming to `public.plain-text`, extensions `md` and `markdown`) and folders (`public.folder`) as document types Wolf opens, with `LSHandlerRank` `Alternate`. The Markdown type names no icon, so Finder draws its generic document icon. `application(_:open:)` hands each URL to the same `wolf-core` handling as a request with no `WOLF_TAB_ID`.
- **Linux:** the desktop file lists `text/markdown` and `inode/directory` in `MimeType`, and its `Exec` runs `wolf %f`, so GNOME Files goes through the command and its channel. A file's icon comes from the icon theme.
