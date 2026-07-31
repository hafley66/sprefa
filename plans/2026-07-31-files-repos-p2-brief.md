# files/repos part 2 — brief addendum (opus worktree, ruling A)

Continues plans/2026-07-31-files-repos-brief.md parts 2-4. Part 1 is
MERGED (files/files_at live, scan refused). The spelling fork is RULED:
`repo_column_spelling = distinct_name_hosts` (rulings.pl tail) — user:
"no magic strings repeated all the time since we dont have defaults or
nulls".

## What to build

1. **repo-scoped host pair**: `repo_files(repo, glob) -> (path, digest)`
   and `repo_files_at(repo, rev, glob) -> (path, digest)`, templates =
   the files/files_at templates with `git -C "{repo}"`. The unscoped
   pair stays byte-untouched. Registry/grammar/SYNTAX rows.
2. **repo-scoped extraction**: under ruling A the extraction fork
   resolves the same way — a distinct-named repo-scoped extraction host
   (its template prefixes the repo root), NOT a widened
   host_executor_contract. Keep the sprefa_extract executor's batching
   for the unscoped host; the repo-scoped one may fall to the generic
   shell executor if the contract cannot name it — state which and why.
3. **bind watch refusal**: repo on `bind watch` = named refusal (the
   lane's four stated reasons; a crawl enumerates once, it does not
   react).
4. **repos host on the clock** (ruling org_fanout): sh host + clock
   bind exactly as the original brief part 3 — LOCAL corpus-dir variant
   graded, gh-shaped template written.
5. **crawl_org.dl6**: repo rows -> repo_files_at -> repo-scoped
   extraction -> one derived rel. Fixture-scale oracle-vs-emitted
   byte identity where gradeable.
6. **crawl-bench.sh v6 leg**: rewritten to run the ONE program over the
   real corpus — the shell loop over repos DIES. Before/after numbers
   in the script output.
7. **host_arity_overload_miscompile** (ARCH row): while you are in
   1_host_expand.pl territory, land the load-time refusal on duplicate
   host names (name-keyed means the name must be unique — say so in the
   refusal text). Fail-first fixture.

## Receipts

Battery from v6/: conformance, sweep both modes, TEXT_DOOR, roundtrip,
plunit, getting-started, `just files`, extraction-live, crawl-bench.
Counts stated. Fail-first fixtures: repo_files routes git -C,
repo_files_at pins rev, duplicate-host-name refusal, watch-repo refusal.

## Fences

- Worktree law: first action `git merge --ff-only 29f1a9fc`; failure =
  STOP AND REPORT.
- Do NOT touch: bench-cli/**, dataflow-atlas.* / atlas.sh /
  xref_facts.pl (a parallel lane owns them), v5 src/**, labs/**.
- pnpm install per package; never symlink outer node_modules.
- Style laws per CLAUDE.md. Commit per step `git commit -n`; no push.
