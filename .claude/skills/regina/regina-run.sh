#!/bin/sh
# Starts and finishes a regina run for the regina skill; SKILL.md says when to call it
# and README.md why each refusal exists.
set -eu

project=wolf
# The oldest regina with the Seatbelt grants and the host profile's /dev/shm lift wolf's gate needs.
regina_floor=7b72b1ec575c2dcd20e21bb6c100dcd215c62d47
# Bazel's TEST_TMPDIR nests under the state root, and short_tempdir refuses one past
# SHORT_TEMP_ROOT_MAX (crates/wolf-core/tests/common/mod.rs); README.md has the reason.
state_max=60

usage() {
    echo "usage: regina-run.sh start BRANCH | regina-run.sh finish BRANCH LABEL" >&2
    exit 2
}

refuse() {
    echo "regina-run.sh: $1" >&2
    exit 1
}

# A path as a single-quoted shell word, for the wrapper this script writes.
quoted() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

# A path as the replacement text of a sed `s|…|…|` command.
replacement() {
    printf '%s' "$1" | sed 's/[\\&|]/\\&/g'
}

tracked_changes() {
    git -C "$1" status --porcelain --untracked-files=no
}

# The directory `git worktree list` gives for the branch, or nothing.
worktree_of() {
    git -C "$main" worktree list --porcelain | awk -v want="branch refs/heads/$1" '
        /^worktree / { path = substr($0, 10) }
        $0 == want { print path }'
}

locate() {
    branch=$1
    git check-ref-format --branch "$branch" >/dev/null 2>&1 ||
        refuse "$branch is not a branch name git accepts"
    origin=$(git rev-parse --show-toplevel 2>/dev/null) ||
        refuse "run this from inside the repository"
    common=$(git -C "$origin" rev-parse --path-format=absolute --git-common-dir)
    [ "$(basename "$common")" = .git ] ||
        refuse "$common is not the .git directory of a checkout"
    # The main checkout, so a run started from a linked worktree is never nested in it.
    main=$(dirname "$common")
    worktree=$main/.worktrees/$branch
    run=$main/.workspace/$branch
    case ${HOME:-} in
        /*) ;;
        *) refuse "HOME is not an absolute path, and the state root is under it" ;;
    esac
    # Short and fixed whatever the branch is called, and the same for start and finish.
    key=$(printf '%s\n%s\n' "$main" "$branch" | shasum -a 256 | cut -c1-8)
    state=$HOME/.local/state/wolf-regina/$key
}

start() {
    [ -z "${REGINA_AGENT:-}" ] ||
        refuse "REGINA_AGENT is set: this is a regina worker, which starts no run"
    [ "$(uname -s)" = Darwin ] ||
        refuse "the template is for a macOS host with the Linux secondary nixos.local"
    locate "$1"

    [ "$(git -C "$origin" branch --show-current)" != "$branch" ] ||
        refuse "$branch is checked out in $origin, which is where the tool comes from; the work needs a branch of its own"
    [ -z "$(tracked_changes "$origin")" ] ||
        refuse "$origin has uncommitted changes; regina reads commits, so commit or stash them first"
    template=$origin/.claude/skills/regina/regina.conf.template
    [ -f "$template" ] || refuse "$template is missing"
    # The template names it, and `register` would refuse a flow file that cannot be read.
    flow=$origin/.claude/skills/regina/flow.yaml
    [ -f "$flow" ] || refuse "$flow is missing"
    commit=$(git -C "$origin" rev-parse HEAD)

    # Regina is built from its own checkout, never this one; nix would build a dirty tree as is.
    regina_src=${REGINA_SOURCE:-$HOME/dev/jplein/regina}
    regina_src=$(cd "$regina_src" 2>/dev/null && pwd -P) ||
        refuse "${REGINA_SOURCE:-$HOME/dev/jplein/regina} is not a directory; set REGINA_SOURCE to a regina checkout"
    [ "$(git -C "$regina_src" rev-parse --show-toplevel 2>/dev/null)" = "$regina_src" ] &&
        [ -f "$regina_src/crates/regina-cli/Cargo.toml" ] ||
        refuse "$regina_src is not the top of a regina checkout; set REGINA_SOURCE to one"
    [ -z "$(tracked_changes "$regina_src")" ] ||
        refuse "$regina_src has uncommitted changes, which nix would build into the binary"
    git -C "$regina_src" merge-base --is-ancestor "$regina_floor" HEAD 2>/dev/null ||
        refuse "$regina_src does not contain $regina_floor, whose Seatbelt grants and /dev/shm lift wolf's gate needs"
    regina_commit=$(git -C "$regina_src" rev-parse HEAD)

    mkdir -p "$state" || refuse "cannot create the state root $state"
    state=$(cd "$state" && pwd -P)
    length=$(printf '%s' "$state" | wc -c | tr -d ' ')
    [ "$length" -le "$state_max" ] ||
        refuse "the state root $state is $length bytes, over $state_max; Bazel's TEST_TMPDIR under it would pass SHORT_TEMP_ROOT_MAX"

    mkdir -p "$run/tasks"
    if [ -L "$run/state" ] || [ ! -e "$run/state" ]; then
        # A link for reading the logs only: regina.conf names the real path.
        ln -sfn "$state" "$run/state"
    else
        refuse "$run/state is not a link; it holds a run of an older harness, which finish cannot land"
    fi
    if [ -x "$run/regina" ]; then
        open=$("$run/regina" status --open 2>/dev/null || true)
        case $open in
            "no open tasks"* | "") ;;
            *) printf 'warning: %s already holds open tasks, which the next run picks up:\n%s\n' "$run" "$open" >&2 ;;
        esac
    fi

    # Out-links in the run directory: nothing else re-points them, and they keep the
    # store paths alive for as long as the run directory exists.
    nix build "$regina_src" --out-link "$run/result" ||
        refuse "nix build of $regina_src at $regina_commit failed"
    regina=$(readlink -f "$run/result")/bin/regina
    [ -x "$regina" ] || refuse "$regina is not an executable"

    if git -C "$main" show-ref --verify --quiet "refs/heads/$branch"; then
        at=$(worktree_of "$branch")
        if [ -z "$at" ]; then
            mkdir -p "$(dirname "$worktree")"
            git -C "$main" worktree add --quiet "$worktree" "$branch"
            how="added for the existing branch"
        elif [ "$at" = "$worktree" ]; then
            [ -z "$(tracked_changes "$worktree")" ] ||
                refuse "$worktree has uncommitted changes; the run would not see them"
            how="reused"
        else
            refuse "$branch is checked out in $at, not in $worktree"
        fi
    else
        mkdir -p "$(dirname "$worktree")"
        git -C "$main" worktree add --quiet -b "$branch" "$worktree" "$commit"
        how="created at $commit"
    fi

    sed -e "s|@SOURCE@|$(replacement "$main")|g" \
        -e "s|@BASE@|$(replacement "$branch")|g" \
        -e "s|@ORIGIN@|$(replacement "$origin")|g" \
        -e "s|@STATE@|$(replacement "$state")|g" \
        "$template" >"$run/regina.conf"

    cat >"$run/regina" <<EOF
#!/bin/sh
# Written by regina-run.sh start. Every regina command of this run goes through here,
# so the binary and the tree its flow and its definitions are read from stay the ones it
# started with.
set -eu
origin=$(quoted "$origin")
commit=$commit
regina=$(quoted "$regina")
config=$(quoted "$run/regina.conf")
case \${1:-} in
    status) ;;
    *)
        head=\$(git -C "\$origin" rev-parse HEAD)
        [ "\$head" = "\$commit" ] || {
            echo "regina: \$origin moved from \$commit to \$head since this run started; check \$commit out there again, or start a new run" >&2
            exit 1
        }
        [ -z "\$(git -C "\$origin" status --porcelain --untracked-files=no)" ] || {
            echo "regina: \$origin has uncommitted changes, and this run reads its flow and its agent definitions from there" >&2
            exit 1
        }
        ;;
esac
exec env -u REGINA_WORKER_IMAGE -u REGINA_WORKER_IMAGE_ARCHIVE "\$regina" --config "\$config" "\$@"
EOF
    chmod +x "$run/regina"

    "$run/regina" register
    echo "tool      $origin at $commit"
    echo "binary    $regina"
    echo "regina    $regina_src at $regina_commit (nixos.local's start must be the same protocol)"
    echo "branch    $branch"
    echo "worktree  $worktree ($how)"
    echo "run       $run"
    echo "state     $state ($length bytes; $run/state links to it)"
    echo "next      write task files under $run/tasks, then: $run/regina enqueue $project LABEL FILE..."
}

finish() {
    locate "$1"
    label=$2
    repo=$state/repos/$project.git
    ref=refs/heads/regina/land/$label
    [ -d "$repo" ] || refuse "$state holds no run of $branch"
    git -C "$repo" rev-parse --verify --quiet "$ref^{commit}" >/dev/null ||
        refuse "nothing was enqueued under the label $label in $state"
    [ "$(worktree_of "$branch")" = "$worktree" ] ||
        refuse "$branch is not checked out in $worktree"
    [ -z "$(tracked_changes "$worktree")" ] ||
        refuse "$worktree has uncommitted changes; commit or discard them before the result lands there"

    before=$(git -C "$worktree" rev-parse HEAD)
    git -C "$worktree" fetch --quiet "$repo" "$ref"
    git -C "$worktree" merge --quiet --ff-only FETCH_HEAD 2>/dev/null ||
        refuse "$branch has moved since the run started, so regina/land/$label is not a fast-forward of it"
    after=$(git -C "$worktree" rev-parse HEAD)
    count=$(git -C "$worktree" rev-list --count "$before..$after")
    echo "landed    $count commit(s) from regina/land/$label"
    echo "branch    $branch at $after"
    echo "worktree  $worktree"
}

case ${1:-} in
    start)
        [ "$#" -eq 2 ] || usage
        start "$2"
        ;;
    finish)
        [ "$#" -eq 3 ] || usage
        finish "$2" "$3"
        ;;
    *) usage ;;
esac
