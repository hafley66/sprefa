# Lane brief: dl6 bindings for refs, tags, merge-base, ahead/behind, ancestry

First action: `git merge --ff-only cf6c4259`. Failure = STOP AND REPORT.

## Goal

Issue `issues/dl6-git-ref-ancestry` (high, parity). Soopy holds the git
mechanics; project refs, tags, tag history, merge-base, ahead/behind, and
revision ancestry into authored dl6 relations through the host tick path.
This is the "rev" slice of the v5-parity port.

## Where

- `v6/sprefa-engine-rs/src/hosts.rs` — new soopy executor arms. Follow
  SoopyFilesExecutor (`hosts.rs:98`) and the just-landed dep_crawl arms
  (PR #289): name + execution tag match, no child spawn, memoise where four
  demands share one computation.
- `v6/sprefa-engine-rs/src/source_bind/_0_types.rs` + `_1_runtime.rs` —
  relation declarations and arrivals if these ride SourceBind; if they ride
  the demand path like dep_crawl, declarations live beside the executor.
  Choose ONE shape and say why in the commit body, citing how dep_crawl and
  SoopyFilesExecutor each did it.
- `v6/sprefa-engine-rs/src/driver.rs` — only if scheduling needs it;
  PR #289 found run_schedule_live already schedules host plans.
- Authored dl6 spelling + gate: extend `v6/tsv2/goldens/multirepo_crawl/`
  with a numbered dl6 + gate pair (the 4_dep_crawl.dl6 / 5_dep_gate.sh
  pattern), graded against soopy's own answers on the pinned corpus.

## Relation sheet (surrogate INTEGER ids; no composite TEXT PKs anywhere)

- ref(repo, ref_name, kind, target_sha) — branches + tags + HEAD
- tag(repo, tag_name, target_sha, tagged_at) — tag history ordered
- merge_base(repo, rev_a, rev_b, base_sha)
- ahead_behind(repo, rev_a, rev_b, ahead_count, behind_count)
- ancestor(repo, ancestor_sha, descendant_sha) — bounded: only for revs a
  program demands, never the whole DAG closure unasked

Read `.claude/skills/sql-relational-design` before any DDL decision.

## Receipts (three runs each)

```bash
cd v6 && just multirepo-golden        # stays green
cd v6/sprefa-engine-rs && cargo test  # crate suite + your new tests
```

Your new gate prints per-rel row counts and byte-diffs against a soopy-direct
dump. Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`, never pipe a
commit, check `git log` before finishing.

## File ownership

OWNS: `v6/sprefa-engine-rs/src/hosts.rs`, `src/driver.rs`,
`src/source_bind/**`, `src/lib.rs` (module lines), engine-rs tests,
`v6/tsv2/goldens/multirepo_crawl/` (NEW numbered files only — do not edit
0_/1_/2_/3_/4_/5_ existing files).

FORBIDDEN: `v6/tsv2/goldens/scip_combo/**`, `v6/justfile`,
`src/text_plane.rs`, `src/dep_resolve.rs`, `v6/prolog/**`. Another lane owns
scip_combo and the justfile tonight.

## Laws

- No `eprintln!`; `tracing` only.
- N+1: never a per-row write.
- Comment budget: constraints only; design rationale goes in the commit body.
- Language vocabulary: rxjs/prolog/SQL words; "support" banned.
- A permission denial ends the approach; report, never work around.
