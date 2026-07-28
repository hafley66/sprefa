# SQLITE RETRACTION LAB VERDICT

Lab: `v6/prolog/labs/sqlite_retraction/lab.pl`. Run:
`swipl -q -l v6/prolog/labs/sqlite_retraction/lab.pl -g go -g halt` (exit 0, 20/20
PASS, stdout is PASS-only; timings and receipts print to stderr as `TIMING` and
diagnostic lines, never to stdout). Every number below is read back from a real
`.sqlite3` file under `TMPDIR`, driven through the `sqlite3` CLI via
`process_create` (no ODBC, no packs, no in-memory prolog model of the store).

This re-proves plans/2026-07-28-types-as-rels-verdict.md's Q3 domination
claims, which were graded against a prolog model of the store, in the real
database. It also surfaces one finding the header did not anticipate: fk_cascade
has a hard operational ceiling well below the semantic question the header
posed.

## Schema (byte-identical across all three strategies)

```sql
CREATE TABLE value_node (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  is_root INTEGER NOT NULL DEFAULT 0,
  owner_id INTEGER REFERENCES value_node(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE value_ref (
  parent_id INTEGER NOT NULL,
  child_id INTEGER NOT NULL,
  PRIMARY KEY (parent_id, child_id)
);
```

`value_ref` is the real, possibly multi-parent, possibly cyclic logical
reference graph; `support_count` and `fixpoint_recompute` read only this
table and never turn `PRAGMA foreign_keys` on. `owner_id` is a single-parent
self-reference that only `fk_cascade`'s delete relies on. It models the
naive "give every row exactly one owner" schema a developer reaches for to
get `ON DELETE CASCADE` to clean up a tree: sqlite's cascade fires
referenced-row-deletion -> referencing-row-deletion, so a parent-points-at-
child edge table cannot express "delete parent, remove now-orphaned
children" at all. A single `owner_id` column recursing through the same
table is the only shape that does, and it forces exactly one owner per row,
which is the schema-level reason cascade cannot be right whenever a child is
actually shared.

## Strategy x scenario matrix

| scenario | fk_cascade | support_count | fixpoint_recompute |
|---|---|---|---|
| a. chain, depth 4 | correct: survivors `[]`, 1 statement | correct: survivors `[]`, 3 rounds | correct: survivors `[]`, 1 statement |
| b. shared child, step 1 (release root1) | **WRONG**: survivors `[2]`, child dies early, dangling ref `(2,3)` | correct: survivors `[2,3]`, child survives, no dangling | correct: survivors `[2,3]`, child survives |
| b. shared child, step 2 (release root2) | survivors `[]` (converges, but via the wrong path) | correct: survivors `[]` | correct: survivors `[]` |
| c. cycle (root->a, a<->b) | survivors `[]` (coincidentally correct, see below) | **WRONG**: survivors `[2,3]`, 0 rounds, counts lie on cycles | correct: survivors `[]` |
| d. diamond, root->a,b->shared c | correct: survivors `[]`, count 4->0 | correct: survivors `[]`, count 4->0, 2 rounds | correct: survivors `[]`, count 4->0, 1 statement |
| e. crash mid cascade (ROLLBACK sim) | not applicable | correct: survivors `[1,2,3,4]` recovered | not applicable |
| e. crash mid cascade (real SIGKILL) | not applicable | correct: survivors `[1,2,3,4]` recovered | not applicable |

All 20 lab checks PASS; "WRONG" above means the check asserts and confirms
the wrong result as the real, measured sqlite behavior, not a failing test.

## Affected-row counts

Diamond (scenario d), before/after `COUNT(*)` on `value_node` (4 -> 0 for
every strategy): `changes()` was NOT used for this measurement because
sqlite's own documentation excludes cascade-caused rows from `changes()` --
it reports only the row directly targeted by the DELETE statement, so a
naive `changes()` read after `fk_cascade`'s single `DELETE FROM value_node
WHERE id=1` would report 1, not 4, and silently hide whether the cascade
touched anything more than once. The before/after table count is the
uniform, honest measurement across all three strategies; it confirms no
strategy visited or deleted `c` (reachable via both `root->a->c` and
`root->b->c`) more than once.

`support_count`'s round counts, each round being exactly two SQL statements
(prune dead edges, delete newly-zero-support non-root rows) plus a
`changes()` read, looped in prolog until a round changes nothing:

| scenario | rounds |
|---|---|
| a. chain, depth 4 | 3 |
| b. shared child, step 1 | 0 (child's support only drops 2->1) |
| b. shared child, step 2 | 1 |
| c. cycle | 0 (never sees a zero-support row) |
| d. diamond | 2 |

## 10k-row timings, scenario a (chain of 10,000 nodes, release the root)

| strategy | result | elapsed |
|---|---|---|
| fixpoint_recompute | correct, survivors `[]`, 1 statement | 9 ms |
| support_count | correct, survivors `[]`, 9999 rounds | 18,334 ms |
| fk_cascade | **FAILS**, exit 1, table untouched (count stays 10,000) | 520 ms to fail |

`fk_cascade`'s failure is not a timing outlier to explain away: it is a real
sqlite error, `Runtime error near line ...: too many levels of trigger
recursion`. Binary-probed directly (outside the lab, receipts below) against
plain chains of increasing depth: a 1000-node chain (999 cascade hops)
deletes clean; a 1001-node chain (1000 cascade hops) fails identically, and
the failed statement leaves the table exactly as it was (no partial
cascade). `.limit trigger_depth` reports `1000` as both the default and, on
this build, the ceiling: `.limit trigger_depth 20000` does not raise it, it
still reports `1000` afterward. `trigger_depth` is sqlite's compiled-in
`SQLITE_MAX_TRIGGER_DEPTH`; the CLI's `.limit` command cannot exceed a
compiled hard maximum regardless of the value passed. At the scale this lab
was asked to measure (10k), `fk_cascade` is not a slower alternative to the
other two strategies. It does not run.

## What fk_cascade actually did, with receipts

**Shared child (scenario b, step 1).** Seed: `root1(1)`, `root2(2)`, both
`is_root=1`, `child(3)` with `owner_id=1` (assigned to whichever root
happened to create it first); `value_ref` holds `(1,3)` and `(2,3)`, the true
multi-parent graph. `PRAGMA foreign_keys=ON; DELETE FROM value_node WHERE
id=1;` cascades through `owner_id` and removes `child(3)` along with
`root1`, even though `root2` still references it. Read back: survivors
`[2]`. The dangling-ref query (parent survives, child gone) returns exactly
`(2,3)`: `root2`'s own `value_ref` row now points at a deleted id. This is
the decisive result for Q3(a): a single `owner_id` column cannot represent
two parents, so whichever root did not win the ownership assignment is
silently left holding a broken pointer the moment its co-owner is released.

**Cycle (scenario c).** Seed: `root(1)` -`>` `a(2)` -`>` `b(3)` -`>` `a(2)`
in `value_ref` (a genuine two-node cycle downstream of the root); `owner_id`
assigns `root` owns `a`, `a` owns `b` (a simple tree that happens to span
one path through the cycle). `DELETE FROM value_node WHERE id=1;` cascades
`root -> a -> b` and removes both, matching the correct answer here --
purely because this particular ownership assignment happened to trace a
spanning path through the cycle. Scenario b already shows the opposite: the
same mechanism assigns the WRONG single owner whenever a second real parent
exists. This is not a property of cascade being safe on cycles; it is a
coincidence of one arbitrary assignment.

Separately, a **genuinely circular** `owner_id` (two rows each declared as
the other's owner) was attempted two ways against a bare `cyc(id,
owner_id REFERENCES cyc(id) ON DELETE CASCADE)` table:

- Immediate (non-deferred) FK, autocommit: `INSERT INTO cyc(id, owner_id)
  VALUES (1, 2);` before row 2 exists fails at insert time with the real
  sqlite text: `Runtime error near line 10: FOREIGN KEY constraint failed
  (19)`, exit 1. A literal cycle cannot even be built without deferred
  constraints; the chicken-and-egg insert order has no legal sequence under
  immediate checking.
- `DEFERRABLE INITIALLY DEFERRED`, both inserts wrapped in one `BEGIN
  ... COMMIT`: the mutual pair commits cleanly (exit 0, no error). Deleting
  either member (`DELETE FROM cyc WHERE id=1;`, `PRAGMA foreign_keys=ON`)
  then cascades through the true cycle and removes BOTH rows in the same
  statement: exit 0, empty result set, no infinite loop, no error. Sqlite
  handles a real cascade cycle correctly and atomically when the FK is
  declared deferrable; it simply cannot express one under immediate
  checking at all.

## Engine implication

Fk_cascade is right only for a genuine single-parent tree, where the
`owner_id`-shaped assignment it depends on is not an approximation but the
actual shape of the data; the moment a value has a second real parent
(shared children, the ordinary case for a content-addressed value graph)
cascade picks one owner arbitrarily and corrupts every other referrer's
pointer the instant that owner is released, with no error, no warning, and
a dangling ref left in the surviving row's own data. Support counting
(`support_count`) is complete and correct on that same DAG and is the
natural fit for content-addressed values, but it is a real per-level round
trip, not a single operation, and it is provably wrong the moment a cycle
exists downstream of a release, since a cyclic pair's mutual reference never
lets either side's incoming count reach zero. The recursive-CTE reseed
(`fixpoint_recompute`) is the only one of the three that is correct on both
DAGs and cycles, because it does not count support at all; it recomputes
reachability from whatever roots remain, which is why it is also the
cheapest strategy measured here by two to three orders of magnitude (9ms vs
18.3s at 10k rows) and the only one of the three not bounded by sqlite's
compiled `trigger_depth` ceiling. It is the honest referee, not merely the
usually-right answer.
