# `crates/wolf-core/src/files.rs`

Last verified at commit `0405bb21a5356bd7adf53deb86308c792d118487`.

## Unwatch before watch

The watcher thread watches only the outermost desired roots, because a recursive watch already covers every root under it. When the set changes, every stale watch is dropped before any new one is made. On inotify, nested watches share one watch descriptor per inode, so unwatching an inner root after the outer root's watch landed would remove the descriptor the outer watch relies on and cut its coverage of that subtree.

A root whose watch fails is excluded from the outermost set, so the roots under it are watched on their own in the next pass of the same reconcile.

## The gitignore chain

`WatchFilter::ignored` reproduces `ignore` 0.4.26's `Ignore::matched_ignore` for the options `walk_files` sets: over the directories from the path's parent up to `/`, deepest first, the first `.gitignore` match and the first `.git/info/exclude` match are kept separately, and the verdict is the `.gitignore` match, else the exclude match, else the global excludes. Under `require_git(false)` no `.git` directory stops the chain, and `info/exclude` is read from `<dir>/.git` whether or not it is a repository. The walk prunes an ignored directory and never looks inside it, so `admits` checks each directory on the way down before the path itself.

`watch_filter_admits_exactly_what_the_walk_lists` holds the filter to the walk on a fixture with a whitelist, a nested `.gitignore`, an exclude file, the depth cap and `skip_dirs`. A change to the walk's options needs the same change here, or the filter drops events the walk would list.
