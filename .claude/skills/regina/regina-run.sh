#!/bin/sh
# Starts and finishes a regina run, and cleans up merged worktrees, for the regina skill;
# SKILL.md says when to call it.
set -eu

project=wolf
# The oldest regina with the Seatbelt grants and the host profile's /dev/shm lift wolf's gate needs.
regina_floor=7b72b1ec575c2dcd20e21bb6c100dcd215c62d47
# Bazel's TEST_TMPDIR nests under the state root, and short_tempdir refuses one past
# SHORT_TEMP_ROOT_MAX (crates/wolf-core/tests/common/mod.rs); README.md has the reason.
state_max=60

usage() {
    echo "usage: regina-run.sh start BRANCH | regina-run.sh finish BRANCH LABEL | regina-run.sh clean [BRANCH...]" >&2
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

# SHA-256 of stdin; Linux hosts may lack shasum, and both print the same hex digest.
digest() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256
    else
        sha256sum
    fi
}

find_main() {
    origin=$(git rev-parse --show-toplevel 2>/dev/null) ||
        refuse "run this from inside the repository"
    common=$(git -C "$origin" rev-parse --path-format=absolute --git-common-dir)
    [ "$(basename "$common")" = .git ] ||
        refuse "$common is not the .git directory of a checkout"
    # The main checkout, so a run started from a linked worktree is never nested in it.
    main=$(dirname "$common")
    case ${HOME:-} in
        /*) ;;
        *) refuse "HOME is not an absolute path, and the state root is under it" ;;
    esac
}

branch_paths() {
    worktree=$main/.worktrees/$1
    run=$main/.workspace/$1
    # Short and fixed whatever the branch is called, and the same for start and finish.
    key=$(printf '%s\n%s\n' "$main" "$1" | digest | cut -c1-8)
    state=$HOME/.local/state/wolf-regina/$key
}

locate() {
    branch=$1
    git check-ref-format --branch "$branch" >/dev/null 2>&1 ||
        refuse "$branch is not a branch name git accepts"
    find_main
    branch_paths "$branch"
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
    # regina re-reads the flow and the agent definitions in origin at every command, so a moved
    # or edited origin would change them; status reads neither, so it is exempt.
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

say() {
    printf '%-10s%s\n' "$1" "$2"
}

# The physical path; for a missing directory, its parent's physical path plus its name.
canon() {
    if [ -d "$1" ]; then
        (cd "$1" && pwd -P) || printf '%s\n' "$1"
    elif [ -d "$(dirname "$1")" ]; then
        printf '%s/%s\n' "$(cd "$(dirname "$1")" && pwd -P)" "$(basename "$1")"
    else
        printf '%s\n' "$1"
    fi
}

# One line per worktree on stdin's `git worktree list --porcelain`: branch, flags, path,
# split by \037 so an empty branch survives `read`; the path goes last as it may hold anything.
worktree_records() {
    awk '
        function emit() { if (seen) printf "%s\037%s\037%s\n", branch, flags, path }
        /^worktree / { emit(); seen = 1; path = substr($0, 10); branch = ""; flags = ","; next }
        /^branch refs\/heads\// { branch = substr($0, 19); next }
        $1 == "locked" || $1 == "prunable" { flags = flags $1 "," }
        END { emit() }'
}

# 0 when nobody holds the regina.lock at $1, or it does not exist; 1 when held; 2 when unknown.
lock_state() {
    [ -e "$1" ] || return 0
    rc=0
    python3 -I -c '
import errno, fcntl, sys
f = open(sys.argv[1], "r")
try:
    fcntl.flock(f.fileno(), fcntl.LOCK_SH | fcntl.LOCK_NB)
except OSError as e:
    sys.exit(3 if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK) else 4)
' "$1" </dev/null >&2 || rc=$?
    case $rc in
        0) return 0 ;;
        3) return 1 ;;
        *) return 2 ;;
    esac
}

skip() {
    say skipped "$1: $2"
    skipped=$((skipped + 1))
}

fail_one() {
    say failed "$1: $2"
    failed=$((failed + 1))
}

# Removes the worktree at $3 (physical $4) of branch $1 with flags $2 if every check passes.
# Sets outcome to removed, skipped or failed; README.md has the reasons for the less obvious checks.
clean_one() {
    b=$1 flags=$2 path=$3 p=$4
    outcome=skipped
    if [ "$b" = main ]; then
        skip "$b" "is main"
        return
    fi
    if [ -z "$b" ]; then
        skip "$path" "detached HEAD"
        return
    fi
    if [ "$p" != "$wt_root/$b" ]; then
        skip "$b" "checked out at $p, not at $wt_root/$b"
        return
    fi
    case $flags in
        *,prunable,*) prunable=1 ;;
        *) prunable="" ;;
    esac
    if [ -n "$prunable" ] || [ ! -d "$p" ]; then
        skip "$b" "directory missing"
        return
    fi
    case $flags in
        *,locked,*)
            skip "$b" "locked"
            return
            ;;
    esac
    if [ "$p" = "$here" ]; then
        skip "$b" "this command runs from it"
        return
    fi
    if ! git -C "$main" merge-base --is-ancestor "refs/heads/$b" refs/heads/main 2>/dev/null; then
        skip "$b" "not merged into main"
        return
    fi
    rc=0
    dirty=$(git -C "$p" status --porcelain </dev/null) || rc=$?
    if [ "$rc" -ne 0 ]; then
        skip "$b" "git status --porcelain exited $rc"
        return
    fi
    if [ -n "$dirty" ]; then
        skip "$b" "uncommitted or untracked changes"
        return
    fi

    # Last before anything destructive, and fail closed: a live run must keep its worktree.
    branch_paths "$b"
    rc=0
    lock_state "$state/regina.lock" || rc=$?
    case $rc in
        0) ;;
        1)
            skip "$b" "a regina command holds $state/regina.lock"
            return
            ;;
        *)
            skip "$b" "cannot probe $state/regina.lock"
            return
            ;;
    esac
    if [ -e "$run/regina" ]; then
        rc=0
        open=$("$run/regina" status --open </dev/null) || rc=$?
        if [ "$rc" -ne 0 ]; then
            skip "$b" "$run/regina status --open exited $rc"
            return
        fi
        first=$(printf '%s\n' "$open" | sed -n 1p)
        if [ "$first" != "no open tasks" ]; then
            skip "$b" "open tasks in $run"
            return
        fi
    fi

    bin=$(command -v bazelisk || command -v bazel || true)
    if [ -z "$bin" ]; then
        for link in "$p"/bazel-*; do
            if [ -L "$link" ]; then
                skip "$b" "Bazel output but no bazelisk or bazel on PATH"
                return
            fi
        done
    else
        # The output base lives outside the worktree; once the worktree is gone nothing finds it.
        rc=0
        (cd "$p" && "$bin" --noblock_for_lock clean --expunge) </dev/null >&2 || rc=$?
        if [ "$rc" -ne 0 ]; then
            outcome=failed
            fail_one "$b" "$(basename "$bin") clean --expunge exited $rc; worktree kept"
            return
        fi
    fi

    rc=0
    git -C "$main" worktree remove "$path" </dev/null >&2 || rc=$?
    if [ "$rc" -ne 0 ]; then
        outcome=failed
        fail_one "$b" "git worktree remove exited $rc"
        return
    fi
    gone="$gone$p
"
    # Never -D: -d refuses a branch its upstream, or else the main checkout's HEAD, lacks,
    # and -D would delete it anyway.
    if ! git -C "$main" branch -d "$b" </dev/null >/dev/null; then
        outcome=failed
        fail_one "$b" "worktree removed; git branch -d refused, branch kept"
        return
    fi
    outcome=removed
    removed=$((removed + 1))
    say removed "$b"
    # Kept: the run's task files and binary, and the state root's database and refs.
    if [ -e "$run" ]; then
        say kept "$run"
    fi
    if [ -e "$state" ]; then
        say kept "$state"
    fi
}

clean() {
    [ -z "${REGINA_AGENT:-}" ] ||
        refuse "REGINA_AGENT is set: this is a regina worker, which removes no worktree"
    find_main
    for name in "$@"; do
        git check-ref-format --branch "$name" >/dev/null 2>&1 ||
            refuse "$name is not a branch name git accepts"
    done
    git -C "$main" show-ref --verify --quiet refs/heads/main ||
        refuse "$main has no local main branch"
    listing=$(git -C "$main" worktree list --porcelain) ||
        refuse "git worktree list in $main failed"
    records=$(printf '%s\n' "$listing" | worktree_records)
    wt_root=$(canon "$main/.worktrees")
    here=$(canon "$origin")
    us=$(printf '\037')
    removed=0 skipped=0 failed=0 missed=0 gone=""

    if [ "$#" -eq 0 ]; then
        while IFS=$us read -r b flags path <&3; do
            [ -n "$path" ] || continue
            p=$(canon "$path")
            # A sweep never looks at the main checkout or a worktree outside .worktrees.
            case $p in
                "$wt_root"/*) clean_one "$b" "$flags" "$path" "$p" ;;
            esac
        done 3<<EOF
$records
EOF
    else
        for name in "$@"; do
            outcome=""
            while IFS=$us read -r b flags path <&3; do
                if [ "$b" = "$name" ]; then
                    clean_one "$b" "$flags" "$path" "$(canon "$path")"
                    break
                fi
            done 3<<EOF
$records
EOF
            if [ -z "$outcome" ]; then
                skip "$name" "not checked out in any worktree"
            fi
            [ "$outcome" = removed ] || missed=$((missed + 1))
        done
    fi

    # Emptied parents of removed worktrees, up to but never including .worktrees itself.
    while IFS= read -r p <&3; do
        [ -n "$p" ] || continue
        d=$(dirname "$p")
        while [ "$d" != "$wt_root" ] && rmdir "$d" 2>/dev/null; do
            d=$(dirname "$d")
        done
    done 3<<EOF
$gone
EOF
    say summary "removed=$removed skipped=$skipped failed=$failed"
    [ "$failed" -eq 0 ] && [ "$missed" -eq 0 ] || exit 1
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
    clean)
        shift
        clean "$@"
        ;;
    *) usage ;;
esac
