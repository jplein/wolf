# The regina harness

This directory runs wolf's tasks through regina, checked out at `~/dev/jplein/regina`, with this Mac as the primary worker and `mugatu` as a Linux secondary. [`SKILL.md`](SKILL.md) is the procedure; [build.md](../../../build.md#under-regina) has what `gates.sh` needs from a regina environment.

**Nothing under `.claude/` is gated.** Prettier, the Markdown checker and doc-lint all skip it, so formatting and the comment limits are kept by hand: format a file here with `prettier --ignore-path /dev/null --write <file>`.

#### `SKILL.md`

Purpose: the `regina` skill, the procedure an interactive session follows to carry a request from task files to an integration branch. It is the regina alternative to `/orchestrate`.

#### `regina-run.sh`

Purpose: starts and finishes a run with a tool fixed at its start, and removes the worktrees of merged branches.

**Public API** — `start BRANCH` creates or reuses `.worktrees/BRANCH`, writes the run directory `.workspace/BRANCH`, whose wrapper `regina` every later command goes through, and registers the project. `finish BRANCH LABEL` fast-forwards the branch to `regina/land/LABEL` and leaves its worktree. `clean [BRANCH...]`, by default every worktree under `.worktrees/`, expunges each qualifying worktree's Bazel output base, then removes it and its branch. It exits 1 on a refusal, a `failed` line or a named branch it did not remove; a sweep's skips leave it 0.

**Decisions**

- The state root, `~/.local/state/wolf-regina/KEY`, is outside the run directory: Bazel's `TEST_TMPDIR` nests under it, and `short_tempdir` (`crates/wolf-core/tests/common/mod.rs`) refuses one over `SHORT_TEMP_ROOT_MAX`. So `start` refuses a canonical root over `state_max` bytes, and `regina.conf` names the canonical path, never the `state` link.
- `clean` compares physical paths, so a symlink under `.worktrees` cannot lead it outside.
- Merged means an ancestor of local `main`, so a just-started branch that has not diverged from it qualifies. `git branch -d`, never `-D`, keeps git's own check, made against the branch's upstream or else the main checkout's `HEAD`, so a refusal can mean `HEAD` is off `main` or a stale upstream.
- Untracked files count as changes; ignored ones such as `bazel-*` do not, or no built worktree would qualify.
- Only a held `<state>/regina.lock` marks a live run, as the file outlives every command; a failing `status --open` skips. The probe takes a shared lock for an instant, so a regina command starting then fails with `state locked` and can be retried.
- `clean` always expunges when `bazelisk` or `bazel` is on `PATH`, having no cheap test that a worktree never ran Bazel.
- A worktree whose directory is missing is skipped and stays registered: `git worktree prune` would also drop records `clean` never chose.
- The run directory and the state root stay for a later `start`; remove the state root after `chmod -R u+w`, as Bazel's output roots in it are read-only.

#### `regina.conf.template`

Purpose: the configuration `start` writes into the run directory.

**Decisions**

- `start` gives the remote its own `--state`, whose lock a `serve` session needs and a `run` on `mugatu` would otherwise hold. Every wolf run shares it, so one runs at a time.
- `profile = host`: Podman refuses `seatbelt-default`, and under `light` 3 to 6 cage smokes die of `SIGBUS` on Podman's 64 MB `/dev/shm`, which `host` lifts from regina 7b72b1e, and starve on 2 CPUs and 4 GiB.
- `timeout = 3600` covers session, push and image provisioning besides the gate, measured at about 2 minutes for the first image build and 3m51s for a cold Linux gate. Too tight costs a `retry`, too loose only a slower hang detection.
- `ssh` gives the session and the push `ConnectTimeout=30`, ample for a local banner's second, and `ServerAliveInterval=30` with `ServerAliveCountMax=4`. A host sending no banner hung requests 20 minutes; against a silent listener ssh failed in 30.00 s, and without `ConnectTimeout` waited past 120 s. `sshd` answers ServerAlive whatever the gate prints, so a lost host fails in 2 minutes, a silent gate never; four misses ride out a blip.
- Two documentation gates: macOS, with no Cargo and a Linux-only `markdown_all.sh`, runs Bazel on the task's warm output root. Linux's gawk checks every Markdown file git knows of, more than Bazel's filegroups and the macOS awk see, and no implementation gate follows a documentation-only round.
- The comments never spell a placeholder, which `start`'s `sed` would replace.

#### `flow.yaml`

Purpose: regina's five-agent flow less its `final` gate, plus a retrospective agent whose reports go to `notes/retrospectives/`, a documentation-only route and a notes pass.

**Decisions**

- Sets use gitignore semantics, where a directory a set matches takes every file under it, whatever a later `!` says. So a test directory with a README below its top is named by file, as `/crates/*/tests/**/*.rs` and `/crates/wolf-macos/Tests/**/*.swift`, which keeps every README under it in `docs`; a fixture under `crates/*/tests/` outside `testdata/` is the coder's.
- `wiring` is the test writer's and the coder's: wiring a test is part of writing it, and the coder needs `BUILD.bazel` for `compile_data` and deps.
- `CLAUDE.md` is in `protected`, which no agent writes. `specs/` cannot be a set, since a set cannot read the task body: the definitions allow a spec edit only when the task names the spec.
- `test-notes`, the `notes/` mirrors of test files, is the test writer's, not the coder's: a test comment over the limit moves to its mirror. It stays in `docs` for the technical writer.
- `documentation-only` runs `./gates.sh` before the technical writer, as the coder's route does, so a plan of documentation alone reaches review with both gates passed and no coder.
- An approval with notes runs the `-notes` states from the first owner of a note, then the retrospective with no second review, so a note is fixed before the task lands.

#### `agents/`

Purpose: the six agent definitions the flow names.

**Decisions**

- They are not under `.claude/agents/`, where Claude Code would load them as this session's subagents and clash by name with the `/orchestrate` team.
- The protocol sections are copied verbatim from regina's shipped `res/definitions/<agent>.md`, and the retrospective's from regina's `.claude/agents/retrospective.md`, at regina 7b72b1e, but for the sentences on states wolf's flow adds. They name the `reason`s regina's flows send, so re-copy them whenever regina is upgraded, and keep those sentences.
- A rule two definitions share is copied into each, never linked: both harnesses send a definition's body verbatim as the system prompt and expand no link, so a linked rule reaches only an agent that reads it.
- The worker sections come in two versions, each word for word: the four writing agents' and the read-only architect's and reviewer's. The zsh paragraph and `## The project` are the same in all six, and `## The notes pass` in each agent of the notes pass. Nothing checks that here.
- The `/orchestrate` team in `.claude/agents/` states some of the same rules on tests, kept in line by hand in the same task. A `#[cfg(test)]` block is the coder's here, since write sets go by path, and the test writer's under `/orchestrate`. Falsifier IDs exist only here, since the reviewer joins the plan and the coder's report by ID and by reading alone.
