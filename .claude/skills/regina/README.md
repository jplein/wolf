# The regina harness

This directory runs wolf's tasks through regina, checked out at `~/dev/jplein/regina`, with this Mac as the primary worker and `nixos.local` as a Linux secondary. [`SKILL.md`](SKILL.md) is the procedure; [build.md](../../../build.md#under-regina) has what `gates.sh` needs from a regina environment.

**Nothing under `.claude/` is gated.** Prettier, the Markdown checker and doc-lint all skip it, so formatting and the comment limits are kept by hand: format a file here with `prettier --ignore-path /dev/null --write <file>`.

#### `SKILL.md`

Purpose: the `regina` skill, the procedure an interactive session follows to carry a request from task files to an integration branch. It is the regina alternative to `/orchestrate`.

#### `regina-run.sh`

Purpose: starts and finishes a run, keeping the binary, the flow and the definitions it uses the ones it started with.

**Public API** — `regina-run.sh start BRANCH` creates or reuses `BRANCH` in `.worktrees/BRANCH`, writes `.workspace/BRANCH/{regina.conf,tasks/,regina,state}`, where `state` links to the run's state root, and registers the project. `regina-run.sh finish BRANCH LABEL` fast-forwards the branch to `regina/land/LABEL`. Every later command goes through the wrapper `.workspace/BRANCH/regina`.

**Decisions**

- Regina is built from its own checkout and pinned by the nix out-link `.workspace/BRANCH/result`; a dirty checkout is refused, since nix would build it as is. The commit is printed: `nixos.local`'s `regina serve` must speak the same protocol.
- `REGINA_SOURCE`, by default `~/dev/jplein/regina`, must be the top of a regina checkout that contains 7b72b1e, the first with the Seatbelt grants wolf's macOS gate needs and the `host` profile's `/dev/shm` lift.
- The state root is `~/.local/state/wolf-regina/KEY`, `KEY` hashing the main checkout and the branch. Bazel's `TEST_TMPDIR` nests under it, and `short_tempdir` (`crates/wolf-core/tests/common/mod.rs`) refuses one over `SHORT_TEMP_ROOT_MAX`, 222 bytes, which a run-directory state root nearly reaches. So `start` refuses a root over 60 bytes canonical, and `regina.conf` names the canonical path, never the link.
- Remove a state root with `chmod -R u+w` first: Bazel's output roots in it are read-only.
- The wrapper refuses all but `status` once the starting checkout moves or gets a tracked change: regina re-reads the flow and the definitions there at every command.

#### `regina.conf.template`

Purpose: the configuration `start` writes into the run directory.

**Decisions**

- `start` gives the remote its own `--state`, whose lock a `serve` session needs and a `run` on `nixos.local` would otherwise hold. Every wolf run shares it, so one runs at a time.
- `profile = host`: Podman refuses `seatbelt-default`, and under `light` 3 to 6 cage smokes die of `SIGBUS` on Podman's 64 MB `/dev/shm`, which `host` lifts from regina 7b72b1e, and starve on 2 CPUs and 4 GiB.
- `timeout = 3600` covers session, push and image provisioning besides the gate, measured at about 2 minutes for the first image build and 3m51s for a cold Linux gate. Too tight costs a `retry`, too loose only a slower hang detection.
- `ssh` gives the session and the push `ConnectTimeout=30`, ample for a local banner's second, and `ServerAliveInterval=30` with `ServerAliveCountMax=4`. A host sending no banner hung requests 20 minutes; against a silent listener ssh failed in 30.00 s, and without `ConnectTimeout` waited past 120 s. `sshd` answers ServerAlive whatever the gate prints, so a lost host fails in 2 minutes, a silent gate never; four misses ride out a blip.
- Two documentation gates: macOS, with no Cargo and a Linux-only `markdown_all.sh`, runs Bazel on the task's warm output root. Linux's gawk checks every Markdown file git knows of, more than Bazel's filegroups and the macOS awk see, and no implementation gate follows a documentation-only round.
- The comments never spell a placeholder, which `start`'s `sed` would replace.

#### `flow.yaml`

Purpose: regina's five-agent flow less its `final` gate, plus a retrospective agent whose reports go to `notes/retrospectives/`.

**Decisions**

- Sets use gitignore semantics, where a directory a set matches takes every file under it, whatever a later `!` says. So the Swift tests are `/crates/wolf-macos/Tests/**/*.swift` rather than the directory, which keeps the two READMEs there in `docs`, and `!/crates/*/tests/README.md` works because `/crates/*/tests/**` does not match the directory itself.
- `wiring` is in both the test writer's and the coder's writes: wiring a test is part of writing it, and the coder still needs `BUILD.bazel` for `compile_data` and deps.
- `CLAUDE.md` is in `protected`, which no agent writes, and regina reverts any write to it. `specs/` cannot be a set, since a set cannot read the task body: the definitions allow a spec edit only when the task names the spec.
- A `#[cfg(test)]` block inside `src/` is code by path, so it is the coder's.
- `test-notes`, the `notes/` mirrors of test files, is the test writer's and not the coder's: a test comment over the limit moves to its mirror. It stays in `docs` for the technical writer.
- The coder's falsifier evidence goes to a report under `.regina/reports`, not to `notes/`: wolf has no red-green note convention, and a note needs a verified header.
- To drop the retrospective, point the reviewer's `passed` and the architect's `none` at `coverage` and delete its three states. Choose before `start`: an edit to the flow mid-request makes a retry start over.

#### `agents/`

Purpose: the six agent definitions the flow names.

**Decisions**

- They are not under `.claude/agents/`, where Claude Code would load them as this session's subagents and clash by name with the `/orchestrate` team.
- The protocol sections are copied verbatim from regina's shipped `res/definitions/<agent>.md`, and the retrospective's from regina's `.claude/agents/retrospective.md`, at regina 7b72b1e. They name the `reason`s regina's flows send, so re-copy them whenever regina is upgraded.
- A rule two definitions share is copied into each, never linked: both harnesses send a definition's body verbatim as the system prompt and expand no link, so a linked rule reaches only an agent that reads it. A regina agent would read it from the task's branch, not the checkout [`SKILL.md`](SKILL.md#the-two-sides-of-a-run) fixes for the run.
- The worker sections come in two versions, each word for word: the four writing agents' and the read-only architect's and reviewer's. The zsh paragraph is the same in all six. Nothing checks that here.
- The `/orchestrate` team in `.claude/agents/` states some of the same rules on tests, in its `coder.md`, `architect.md`, `test-writer.md` and `reviewer.md`, kept in line by hand in the same task. The copies differ where the harnesses do. A `#[cfg(test)]` block is the coder's here, since write sets go by path, and the test writer's under `/orchestrate`. Falsifier IDs exist only here: the reviewer joins the plan and the coder's report, written in separate invocations, by ID and by reading alone, where `/orchestrate`'s orchestrator checks the evidence itself in its step 7.
