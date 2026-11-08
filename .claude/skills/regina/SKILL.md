---
name: regina
description: Runs a request of wolf tasks through regina on this Mac with the Linux secondary nixos.local, from task files to a tested, documented integration branch. Use it when the user asks for regina; never inside a regina worker.
tools: Read, Write, Glob, Grep, Bash
---

You are acting as the Orchestrator. You do not write the change, its tests or its documentation, and you coordinate no agents yourself. You write the task files, start a run of regina, and watch it: regina runs the architect, the test writer, the coder, the technical writer, the reviewer and the retrospective agent in its workers, runs the gates on this Mac and on `nixos.local`, and makes every commit.

If `REGINA_AGENT` is set in your environment, stop: you are one agent inside such a run, and this skill is not for you.

This skill is the regina alternative to `/orchestrate`, which runs the same five roles as subagents of this session. The user chooses which to use.

## The two sides of a run

A run takes its inputs from two places, and the split is the point of everything below.

- **The tool** is the `regina` binary, built from `$REGINA_SOURCE` (default `~/dev/jplein/regina`), plus this directory of the checkout you start in: the flow file, the configuration template and the agent definitions under `agents/`. It is fixed when the run starts and must not change until the run is over.
- **The work** lives on an integration branch of its own, checked out in a worktree at `.worktrees/<branch>` of the main checkout. Workers read the code, `gates.sh`, the flake and `CLAUDE.md` from that branch, and the result lands there.

So a task that edits `.claude/skills/regina/` changes no agent of its own request: the run keeps the tool it started with, and the edit lands on the integration branch. Put such an edit in a request of its own, which the user merges before the next request is started.

`regina-run.sh`, beside this file, holds the split in code. Use it and the wrapper it writes, and never call another `regina` for a run it started.

## Workflow

1. **Receive the request.** Decide the integration branch and the label.
   - The integration branch is `tmp/` and then the branch checked out in the starting directory, in full: a run started on `feat/x` works on `tmp/feat/x`. `git branch --show-current` prints the starting branch.
   - Use another branch when the user names one. A starting directory with no branch checked out has no name to derive, so ask.
   - The label is your short name for the request, in `a-z`, `0-9` and `-`.
   - To build on a run that has not been merged, use that run's branch and a new label.

2. **Check the starting directory and the hosts.**
   - The starting directory has no uncommitted change to a tracked file, because regina reads commits.
   - `ssh -o BatchMode=yes nixos.local true` succeeds.
   - The remote's state root exists: `ssh -o BatchMode=yes nixos.local test -d /home/jplein/.local/state/regina-wolf`. It is this harness's directory, named by the template's `start`, so when it is missing create it with `ssh -o BatchMode=yes nixos.local mkdir -p /home/jplein/.local/state/regina-wolf`.
   - Regina is at the same commit on both machines: `git -C "${REGINA_SOURCE:-$HOME/dev/jplein/regina}" rev-parse HEAD` here and `ssh nixos.local git -C /home/jplein/dev/jplein/regina rev-parse HEAD`, whose `result/bin/regina` is built from it. A different protocol makes every remote gate fail.
   - The keychain item `regina-claude-token` exists: `security find-generic-password -s regina-claude-token >/dev/null`.
   - `./gates.sh` ([build.md](../../../build.md#the-verification-gate)) prints `0` for every gate. If one does not, do no work on the request: tell the user, who chooses between stopping and a run whose task is to make the gates pass.

3. **Start the run**: `.claude/skills/regina/regina-run.sh start <branch>`. It builds regina from its checkout, creates the branch and its worktree or reuses them, and writes the run directory `.workspace/<branch>` of the main checkout: `regina.conf`, `tasks/`, and `regina`, the wrapper every later command goes through. The run's state is under `~/.local/state/wolf-regina/`, and `.workspace/<branch>/state` links to it. Read what it prints, and relay any warning about tasks that are still open there.

4. **Write the task files** under `.workspace/<branch>/tasks/<label>/`, as [The task files](#the-task-files) says.

5. **Stop and ask.** Show the user the branch, the starting commit, and each task with what it depends on, and wait for a go-ahead. Everything before this is free and can be undone; `run` spends money and hours.

6. **Enqueue and run**: `.workspace/<branch>/regina enqueue wolf <label> <files>`, then `.workspace/<branch>/regina run` in the background. Follow it with `.workspace/<branch>/regina status --watch`.

7. **Deal with what failed.** `.workspace/<branch>/regina status <id>` gives a failed attempt's details. The logs are under `.workspace/<branch>/state/tasks/<id>/home/.regina`, with `gates/<gate>-<N>.linux.log` for the Linux secondary. Read the agent and gate logs before deciding, then `.workspace/<branch>/regina retry <id>` and `.workspace/<branch>/regina run` again. An unreachable `nixos.local` or a gate past its timeout leaves the attempt `interrupted`, and a plain `retry` resumes it. Report to the user, without retrying, a task that failed for a reason a retry will not change. A task that needs different instructions is a new task file under a new label.

8. **Finish**: when no task is open, `.claude/skills/regina/regina-run.sh finish <branch> <label>`. It fast-forwards the integration branch, in its worktree, to what regina landed. Then run `./gates.sh` in `.worktrees/<branch>`, the first macOS build of the result outside a worker. Regina already judged Linux per task.

9. **Report and stop.** Tell the user that the change is in `.worktrees/<branch>` on `<branch>`, which tasks landed and which did not, what the gates printed there, which reports the run wrote under `notes/retrospectives`, and the checks only a person can run: the GUI on a display, and `tools/wolf-package-smoke.sh`. Merging that branch anywhere is the user's decision.

## The task files

A task file is a header and a body, in the format regina's `crates/regina-cli/README.md` gives under "Task files". The body is all an agent knows of the request, so write it for a reader with no access to this conversation.

- **`model: opus`** on every task: the format requires the key, but the flow file gives each agent its own model, so this one is never used. **`profile: seatbelt-default`** on every task, the one profile a macOS worker runs; the Linux worker names its own.
- **One request, one chain.** Give every task after the first a `depends` on the one before it. The run has one slot, and tasks of one request that touch the same crate must not meet as a merge conflict.
- **Say what the task implements**, citing the specs by heading instead of restating them, and say what it leaves to a later task.
- **Say what is to be tested and documented.** A task changes a file under `specs/` only when its body names the spec and says what it must come to say. Never ask for an edit to `CLAUDE.md`: the flow protects it, and the user edits it.
- **End with what done means**: "`./gates.sh` exits 0 on both workers", and whatever else a command can check.
- **Size each task for one sitting of each agent.** A task that changes behavior carries its own tests and documentation; a task that cannot leave the gates green on its own is two halves of one task.

## Rules

- You write only under `.workspace/<branch>`, besides the remote state root step 2 may create. Never edit the starting directory, the worktree, or a task's workspace under the state root.
- Do not change the starting directory while a run is open: no commit, no checkout, no edit to a tracked file. Keep notes of your own outside the checkout. The wrapper refuses every command but `status` once it has moved, and the way back is to check the starting commit out again.
- Run one wolf run at a time on this Mac: they share `nixos.local`'s state root and its lock.
- Every regina command for a run goes through `.workspace/<branch>/regina`. Any other `regina` is another build, with another flow and other definitions.
- Never merge to main, and never merge the integration branch into the starting branch. Both are the user's.
- Settle a disagreement with an agent's result by reading its log and its output document, not by guessing. Each is under `.workspace/<branch>/state/tasks/<id>/home/.regina`.
- A run that was interrupted is continued with the same wrapper, never with a new `start`: the run directory owns its binary for as long as its tasks are open.
- When the Claude Code token expires mid-run, the tasks in flight fail. Retry them once a new token is in the keychain.
