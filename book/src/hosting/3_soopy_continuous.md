# soopy continuous

[Example](#example) · [The three watchers](#the-three-watchers) · [A file-content relation](#a-file-content-relation) · [Retraction](#retraction) · [Receipts](#receipts)

## Example

`soopy_refs` is the one continuous soopy executor. A repository with one commit, a second commit made while the run waits, and the `Head` rule of `fixtures/hosts/0_refs.dl7`:

```console
$ d=$(mktemp -d) && export GIT_AUTHOR_NAME=dl8 GIT_AUTHOR_EMAIL=dl8@example.invalid GIT_COMMITTER_NAME=dl8 GIT_COMMITTER_EMAIL=dl8@example.invalid GIT_AUTHOR_DATE="1700000000 +0000" GIT_COMMITTER_DATE="1700000000 +0000" && git -C $d init -q -b main && echo one > $d/a.txt && git -C $d add a.txt && git -C $d commit -q -m a && sed "s|__ROOT__|$d|" fixtures/hosts/0_refs.dl7 > $d/refs.dl7; { sleep 1; echo two > $d/b.txt; git -C $d add b.txt; git -C $d commit -q -m b; } & bash book/show.sh run $d/refs.dl7 --serve soopy_refs --max-ticks 2 | grep '^(Head \|^ticks\|^exit'; wait
(Head "1bd4c43e7380496d43b8c78be3cb324f0a1aec7f")
(Head "e11e71ad03bcedad1d840052fac86d369363baab")
ticks 2
exit 0
```

```
step 0  tick 0  (soopy_refs Root "HEAD" ?Sha) is served             -> effect row
step 1  tick 1  SoopyRefs::answer opens a RepositoryWatcher (refs)  -> snapshot rows, HEAD 1bd4c43
step 2  idle    Wake::woke blocks on recv_timeout                   -> nothing; the loop stays armed
step 3  tick 2  commit b moves refs/heads/main and HEAD             -> diff_refs: Changed, HEAD target moved -> rows for e11e71a
step 4  tick 2  evaluate                                            -> (Head e11e71a) beside (Head 1bd4c43)
```

Steady state after `--max-ticks 2`. Without the flag the loop keeps waiting at step 2.

## The three watchers

All three are public (`hafley-rs/crates/soopy/src/lib.rs:50`).

| watcher | opened by | watches | yields | used by dl8 |
|---|---|---|---|---|
| `RepositoryWatcher` | `SourceTree::watch_repository(WatchQuery)` (`soopy/src/_7_source_tree.rs:108-115`) | any mix of worktree sources, refs, the index, linked worktrees (`soopy/src/_0_types.rs:750-756`) | `RepositoryDelta::{Ref, Source, Index, Worktree, RescanRequired}` (`_0_types.rs:825-831`) | yes, `soopy_refs`, refs only: `source: None`, `index: false` (`src/_9_runtime/_3_executors/soopy_refs.rs:84-91`) |
| `SourceWatcher` | `SourceTree::watch(SourceQuery)` (`_7_source_tree.rs:102-106`) | a `RepositoryWatcher` with source and index set (`soopy/src/_8_watch.rs:357-376`); rejects an immutable revision | `SourceDelta::{Added, Changed, Removed, RevisionChanged, RescanRequired}` (`_0_types.rs:567-579`) | no |
| `DirectoryWatcher` | `DirectoryWatcher::open(root)` or `DirectoryRoot::watch(FileWatchQuery)` (`_8_watch.rs:36-39`, `:111-115`) | a plain directory, no git (`_8_watch.rs:26-27`) | `DirectoryDelta::{Added, Changed, Removed, RescanRequired}` (`_0_types.rs:162-167`) | no |

`git_dirs` (`_8_watch.rs:582`) is public and unused by dl8. `DirectoryWatcher::open_with_query` is `pub(crate)` (`_8_watch.rs:41`); `DirectoryRoot::watch` is its public door. `extract` reads a plain directory once through `DirectoryRoot::snapshot`, with no watcher (`src/_9_runtime/_3_executors/extract.rs:99-100`).

```console
$ grep -rn 'SourceWatcher\|DirectoryWatcher\|git_dirs\|watch_repository' src | sed 's/^\([^:]*:[0-9]*\):.*/\1/'
src/_9_runtime/_3_executors/soopy_refs.rs:91
```

## A file-content relation

A relation with one row per path and content id under a root, git or not. It compiles; no executor serves it.

```dl7
{{#include ../probes/14_file_content.dl7}}
```

```console
$ bash book/show.sh run book/src/probes/14_file_content.dl7 --serve soopy_files
diagnostic served_relation_no_executor(soopy_files)
exit 1
```

| piece | git root | plain directory | what exists to copy |
|---|---|---|---|
| arm on a bound root | `SourceTree::watch(SourceQuery { revision: Worktree, patterns })` | `DirectoryRoot::watch(FileWatchQuery)` | `SoopyRefs::open`, `soopy_refs.rs:74-101` |
| which one | `soopy::discover(root)` succeeds | it fails | `extract.rs:67-100` makes the same choice |
| opening rows | `SourceEntry { source, content }`, content a `GitBlob` or `Blake3` (`_0_types.rs:292-296`) | `FileEntry { file, content }` (`_0_types.rs:117-121`) | `SoopyRefs::snapshot_rows`, `soopy_refs.rs:116-130` |
| wake | `SourceWatcher::recv_timeout` | `DirectoryWatcher::recv_timeout` (`_8_watch.rs:75`) | `Wake::woke`, `soopy_refs.rs:38-63` |
| `Added`, `Changed` | a row for the new content | same | `SoopyRefs::delta_rows`, `soopy_refs.rs:132-155` |
| `RescanRequired` | re-snapshot, rows for every entry | same | `poll`, `soopy_refs.rs:210-237` |
| `Removed` | nothing can be written | same | `soopy_refs.rs:146` |
| error relation | `soopy_files_error` | same | `mod.rs:79-98` |

## Retraction

Tables only grow: one append-only table per relation (`src/_6_eval/_3_table.rs:1-3`). A moved ref is a new row; the old row stays true. A removed file or ref writes nothing (`soopy_refs.rs:146`).

| event | today | with retraction |
|---|---|---|
| ref moves from 1bd4c43 to e11e71a | both `(Head ...)` rows hold, the example above | the 1bd4c43 row retracts |
| file content changes | a second row for the path | the old content row retracts |
| file or ref removed | the last row holds | the row retracts |

Plans and status: [Not built yet](../16_not_built.md), rows "retraction" and "a removed ref retracting its `soopy_refs` row".

## Receipts

| claim | path | command |
|---|---|---|
| snapshot, then a moved ref as a new row | `tests/_20_hosts.rs:228-345` | `cargo test --test _20_hosts` |
| the watcher degrades to a one-second re-read | `soopy_refs.rs:15-16`, `:39-50`, `:93-96` | `sed -n 38,63p src/_9_runtime/_3_executors/soopy_refs.rs` |
| the probe compiles | `book/src/probes/14_file_content.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
