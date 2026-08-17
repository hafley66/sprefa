# Lane brief: rev-parse failure must not mint a row (issue ts-revparse-phantom-row)

First action: `git merge --ff-only 4205d318`. Failure = STOP AND REPORT.

## The defect, already shrunk and pinned

Issue `issues/ts-revparse-phantom-row` (high). PR #291 pinned it:
`v6/tsv2/goldens/scip_combo/6_door_skew_files_at.dl6` is the minimal program,
`8_gate.sh` expects the disagreement TODAY (goes red if doors agree).

Mechanism: `git rev-parse <arg>` echoes its argument when it cannot resolve,
so an authored shell template capturing `oid=$(git rev-parse ...)` then
guarding `[ -n "$oid" ]` passes on failure and absence becomes a row whose
digest is the literal `<rev>:<path>`. Effect: rev-pinned file feeds
over-answer on the TS door; "what is new since a base revision" answers 0
(4 rows on the Rust door, which resolves through soopy and cannot echo).

## Exact fix

1. `grep -rn "rev-parse" v6/tsv2/goldens/` — seven authored declarations
   carry the pattern (PR #291's count; re-count yourself and list them in
   the commit body).
2. In each, replace the bare capture with a verified form:
   `oid=$(git rev-parse --verify --quiet "<rev>^{commit}") || exit 0`
   (empty answer on unresolvable rev; keep each template's existing exit
   discipline — a template that exits 3 for fall-through protection keeps
   that shape for the no-executor case, the verify failure answers zero
   rows). Match each file's existing template style.
3. Two of the seven claim the opposite behavior in their header comments
   (PR #291 commit body). Correct those headers.
4. Flip `8_gate.sh` + `scip_combo/README.md` expectations for programs 1 and
   6: they now agree across doors. The gate keeps going RED if any pinned
   expectation is wrong in either direction. F2 (7_door_skew_family.dl6)
   stays a pinned disagreement; do not touch its expectation.
5. Close-out check: rerun and paste the new graded line; expected shape is
   more rels byte-identical, only the F2 family skew remaining.

## Receipts (three runs each, check loop rc, never tail-mask)

```bash
cd v6 && just scip-combo
cd v6 && just multirepo-golden
cd v6 && just git-refs-golden
```

Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`, never pipe a commit,
check `git log` before finishing.

## File ownership

OWNS: existing `.dl6` files under `v6/tsv2/goldens/` that carry the rev-parse
pattern, `v6/tsv2/goldens/scip_combo/8_gate.sh`,
`v6/tsv2/goldens/scip_combo/README.md`.

FORBIDDEN: `v6/sprefa-engine-rs/src/**`, `v6/sprefa-extract/src/**`,
`v6/prolog/**`, `v6/justfile`, NEW numbered files under
`goldens/multirepo_crawl/` (another lane creates 9_+ tonight; you edit only
files that already exist at your base sha).

## Laws

- Comment budget: constraints only.
- dl variable names descriptive, never single-letter.
- A permission denial ends the approach; report, never work around.
