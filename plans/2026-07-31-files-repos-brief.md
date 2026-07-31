# files/files_at + repos-on-clock — brief (opus worktree)

Executes two rulings (rulings.pl tail): `files_naming =
files_unmarked_worktree_marked_rev` and `org_fanout = repos_host_on_clock`.
Goal receipt: the v5 crawl two-liner becomes ONE v6 program — no shell loop
over repos. Standing directive: smallest correct solution ("turbo mid").

## Part 1: the naming ruling

- `files(glob) -> (path, digest)` = the UNMARKED live worktree feed.
- `files_at(rev, glob) -> (path, digest)` = the marked pinned-rev case.
- `scan` is BANNED for file enumeration (named refusal `removed_word(scan)`
  pointing at files/files_at, the `set` precedent).
- Current spellings `enumerate`/`enumerate_at` become the refused legacy
  words or rename outright — check what GETTING-STARTED/fixtures/receipt
  scripts use and keep every receipt green. Renames sweep: registry.pl rows,
  parse/print, grammar, SYNTAX.md generated table, fixtures, extraction-live
  + enumerate receipt scripts, crawl-bench.sh.

## Part 2: repo as a column (the fan-out enabler)

Today every enumeration host roots at the serve cwd; crawl-bench smuggles
repo via `$DL_CRAWL_REPO` env, one served process per repo. The smallest
correct change: an OPTIONAL leading `repo: text` demand column on
files/files_at (and the extraction host it feeds), lowered to `git -C
"$repo"`. Absent column = serve cwd (today's behavior, all existing
programs unchanged). Decide and STATE in the verdict whether repo also
belongs on `bind watch` (a watcher per repo row) or is refused there for
now with a named refusal — do not silently widen the watch seam.

## Part 3: repos host on the clock

The user's spelling (ruling 4.5): "data from host gh org call on a timer of
1 day". One ordinary sh host + clock bind, zero new constructs:

```
bind interval(secs: int).
sh repos(org: text) -> (repo: text) = `...`.   % gh repo list / ls dirs
rel repo(name: text).
repo(name) <- want_org(org), repos(org, name), interval(_).
```

Exact spelling is yours to fit the shipped kernel (clock joins for SWR
refresh per the clock_residency ruling; content-addressed salts dedupe the
unchanged answer). A LOCAL variant (org = a directory of git repos,
`ls`/`find`-shaped template) must work offline — that is what the graded
receipt uses; the gh-shaped template is written but graded only if
credentials exist.

## The graded receipt (the point of the lane)

A fixture-or-receipt program `crawl_org.dl6`: repo rows from the repos host
over a LOCAL corpus dir -> files_at(repo, "HEAD", glob) -> extraction ->
one derived rel. Graded: (a) oracle-vs-emitted byte identity where the
corpus is fixture-small; (b) crawl-bench.sh v6 leg REWRITTEN to run the one
program over the real corpus — the shell loop over repos DIES; numbers
recorded in the script output vs the previous loop numbers. Both receipts
re-runnable by the coordinator.

## Receipts battery

From v6/: conformance, sweep BOTH modes, TEXT_DOOR, roundtrip, plunit,
`just getting-started` (it names enumerate), extraction-live + enumerate
recipes, crawl-bench. Counts stated. Fail-first fixtures for: files
resolves worktree, files_at pins rev, repo-column demand routes `git -C`,
scan named refusal.

## Fences

- Touch: prolog compile/oracle/registry/grammar, tsv2 serve hosts + bind
  seams, fixtures, receipt scripts named above, SYNTAX.md hand half where
  the generated table does not cover.
- Do NOT touch: bench-cli/** (the doom-probe fix just landed), v5 src/**,
  labs/**.
- pnpm install per package; NEVER symlink outer node_modules.
- Worktree law: first action `git merge --ff-only 6431f7ef`; on failure
  STOP AND REPORT. Commit per step `git commit -n`; no push.
- Vocabulary law: rx/prolog/SQL words only; descriptive variable names.
