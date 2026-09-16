# Lane brief: change facts as authored dl6 (changed / changed_line / created / deleted / modified / renamed)

First action: `git merge --ff-only 0a87f88b`. Failure = STOP AND REPORT.

## Goal

Issue `issues/dl6-change-facts` (high, parity). The V5 change-fact family as
authored dl6 relations, fed from git diff between revs through soopy.

## Shape decision, argued not assumed

The issue text predates PR #290 and names SourceBind
(`source_bind/_0_types.rs` + `_1_runtime.rs`). PR #290 (commit a18966aa) put
refs/ancestry on the DEMAND path with column constants beside the executor,
and its commit body argues the split: SourceBind exists for content that
binds per source row and arrives on its own schedule; per-question rows ride
the demand path. Change facts are a rev-pair question (base, head) — read
a18966aa's rationale, decide which side of that line diff facts sit on, and
argue your call in the commit body against both precedents. Either choice is
acceptable; an unargued choice is not.

## The relations

- created(repo, rev_base, rev_head, path)
- deleted(repo, rev_base, rev_head, path)
- modified(repo, rev_base, rev_head, path)
- renamed(repo, rev_base, rev_head, path_from, path_to)
- changed(repo, rev_base, rev_head, path) — union view of the above
- changed_line(repo, rev_base, rev_head, path, line_number) — head-side line
  numbers of changed lines

changed_line MAY lower as a dl6 join of rev-pinned `files_at` against base
with not/1 where that is faithful, or come straight from soopy's diff; if
soopy already exposes per-line diff, use it (check
~/projects/hafley-rs/crates/soopy API first; read-only, do not edit soopy).

## Gate

`just precommit-changed` exists in `v6/justfile` (the V5 git-fact diags rail
on a real four-commit repo). It must pass: sorted row-set equality, not
counts, so a rail firing on every new file fails on the control. Add your own
numbered gate under `v6/tsv2/goldens/multirepo_crawl/` (9_ onward, the
7_git_refs.dl6 / 8_git_gate.sh pattern) grading byte-identical against an
independently spelled `git diff --name-status` / unified-diff dump over the
pinned corpus (6_history_corpus.sh already deepens it with history).

## Receipts (three runs each)

```bash
cd v6 && just precommit-changed
bash v6/tsv2/goldens/multirepo_crawl/9_*.sh   # your gate
cd v6 && just multirepo-golden                # stays green
cd v6/sprefa-engine-rs && cargo test
```

Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`, never pipe a commit,
check `git log` before finishing.

## File ownership

OWNS: `v6/sprefa-engine-rs/src/hosts.rs`, `src/source_bind/**`, `src/lib.rs`
(module lines), engine-rs tests, `v6/tsv2/goldens/multirepo_crawl/` NEW
numbered files only (9_ onward; 0-8 are read-only).

FORBIDDEN: `v6/justfile`, `v6/tsv2/goldens/scip_combo/**` (another lane owns
both tonight), `src/text_plane.rs`, `src/dep_resolve.rs` (read-only),
`v6/prolog/**`, soopy sources in hafley-rs.

## Laws

- Surrogate INTEGER keys; no composite TEXT PKs; read
  `.claude/skills/sql-relational-design` before DDL.
- No `eprintln!`; `tracing` only. N+1: never a per-row write.
- Comment budget: constraints only; rationale in the commit body.
- A permission denial ends the approach; report, never work around.
