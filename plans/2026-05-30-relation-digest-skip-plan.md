# Relation input-digest skip — plan (2026-05-30)

Axis-A reconcile scaling (see memory `v5-dl-engine`, "intelligent scaling
relative to v4"). The fix: a file edit that changes bytes but not extracted rows
(a comment, reformatting) must NOT trigger a derived rebuild. Today
`tick_paths` marks a source relation changed whenever a matching file's hash
moved, regardless of whether the rows it produced differ.

This is v4's `Replay` short-circuit (dirty_source.rs) at RELATION granularity
instead of v4's row-granular `MEMO_DEPS`. Keep v4's cheap invariant (skip the
unchanged), drop v4's expensive granularity (no per-row dependency set, no RAM
residency). The digest lives in SQLite, so it is durable across daemon restarts
for free.

Goal property: editing a comment in a scanned file re-extracts the source
relation, finds its row set byte-identical, and rebuilds zero derived relations.

---

## Layer 1 — type signatures (engine.rs)

```rust
/// Order-independent content digest of a relation's current rows: XOR-fold of
/// the per-row `__src` hashes in `rel_<rel>`. Same row set ⇒ same digest, any
/// insert order. All-zero = empty relation.
fn rel_digest(&self, rel: &str) -> Result<[u8; 32]>;

/// Stored digest for a relation, or None if never recorded.
fn load_rel_digest(&self, rel: &str) -> Result<Option<[u8; 32]>>;

/// Upsert the current digest for a relation.
fn save_rel_digest(&self, rel: &str, digest: &[u8; 32]) -> Result<()>;

/// Drop from `changed` every relation whose freshly computed digest equals its
/// stored digest (bytes moved, rows did not). Records the new digest for the
/// relations that really changed. Returns the pruned set.
fn prune_unchanged_by_digest(&self, changed: HashSet<String>) -> Result<HashSet<String>>;
```

## Layer 2 — pseudo-bodies

```rust
// rel_digest(rel):
//   acc = [0u8; 32]
//   for src in `SELECT __src FROM rel_<rel>`:        // set, by PK on user cols
//     if let Ok(b) = hex32(&src): acc ^= b           // skip '' / malformed
//   return acc

// prune_unchanged_by_digest(changed):
//   out = {}
//   for rel in changed:
//     d_new = rel_digest(rel)
//     if load_rel_digest(rel) == Some(d_new): continue   // unchanged → skip
//     save_rel_digest(rel, d_new)
//     out.insert(rel)
//   return out
```

Wire-in, `tick_paths`, between the extract loop (engine.rs:449) and `need_full`
(engine.rs:451):

```rust
// the file moved, but did the rows? prune the ones that didn't.
changed_source_rels = self.prune_unchanged_by_digest(changed_source_rels)?;
if changed_source_rels.is_empty() { changed_facts = false; }
```

`need_full` is independent (it fires on empty derived/closure, i.e. cold start),
so a comment edit with non-empty derived takes neither rebuild branch ⇒
`rebuilt = none`. The source rows were retracted and re-inserted identical, so
derived stays consistent. `_file` hash is already updated, so the next tick will
not re-extract this file.

Cold-path seed: at the end of `reconcile_sources` (or end of `tick`), call
`save_rel_digest` for every source rel, so the first delta after a cold run has
a baseline to compare against. Without the seed, the first delta sees `None` and
treats everything as changed (safe, just no skip that once).

## Layer 3 — instance lifetimes

| holds state | lifetime | where |
|---|---|---|
| `_reldigest` rows | per (db, source-rel); durable across ticks AND process restarts | SQLite table |
| `[u8; 32]` digests | function-local, per tick | stack |
| Engine | no new field | — |

No in-RAM cache. The store is the table, consistent with "SQLite is the durable
state, don't build a second one" (the v4 over-build lesson).

## Layer 4 — storage, reads/writes, uniqueness

Storage (one new meta table, created in `ensure_meta` beside `_prov`):

```sql
CREATE TABLE IF NOT EXISTS _reldigest (rel TEXT PRIMARY KEY, digest TEXT);
```

`digest` = 64-char hex of the 32-byte XOR-fold. PK(rel) ⇒ one digest per rel.

Reads/writes per warm tick:
1. (existing) extract changed files → `rel_<R>` updated, `_file` hash bumped,
   `changed_source_rels` tentatively filled.
2. (new) for each `R` in `changed_source_rels` only: `SELECT __src FROM rel_<R>`
   (scoped to changed rels — bounded by the edit, never a full-graph scan),
   XOR-fold → `d_new`; read `_reldigest WHERE rel=R` → `d_old`; equal ⇒ drop R;
   else upsert.
3. (existing) `affected_derived(pruned)` → rebuild only the affected subset.

Uniqueness / correctness:
- `rel_<R>` PK on user cols ⇒ true set ⇒ each `__src` contributes once ⇒ XOR
  cannot accidentally cancel a duplicate.
- XOR is commutative + associative ⇒ order-independent across files and ticks.
- blake3 ⇒ digest equality ⇒ row-set equality (practically).
- Empty relation ⇒ all-zero digest. `load` returns None when no row recorded;
  `None != Some(zero)` so a first-ever-empty relation still records once.
- Only source-rule heads are ever in `changed_source_rels`; derived rows
  (`__src` default `''`) are never digested here.

## Cost and the v2

v1 reads the WHOLE changed relation to fold (e.g. on the kernel, editing one
`.c` re-folds the ~95k-row `callsite` relation: ~tens of ms). Still a large win
over a full derived rebuild (seconds), and only on changed relations.

v2 (follow-on, not now): maintain the digest INCREMENTALLY — XOR out the
retracted rows' `__src` in `retract_path` and XOR in the inserted rows' `__src`
in `insert_source_rows`. Then no full scan; cost is O(changed rows). Defer until
v1 is proven; the full-scan version is the teachable first step.

## Test (tests/digest_skip.rs, ban-gate discipline)

Observable: `tick_paths` logs `rebuilt derived: <what>` on the `dl --changed`
path (run_changed, quiet=false). `_reldigest` persists across the two processes,
which is the point.

| case | action | assert |
|---|---|---|
| comment edit | cold run, then add a `// comment` line, `dl --changed file` | stderr shows `rebuilt derived: none` and `+N -N source facts` (re-extracted, identical) |
| real edit | cold run, then add a real fact (a new function), `dl --changed file` | stderr shows `rebuilt derived: <relname>` |

The contrast is the proof: bytes-only change ⇒ no rebuild; content change ⇒
rebuild. Same shape as `tests/ban_gate.rs`.

## Phases

| phase | deliverable |
|---|---|
| A | `_reldigest` table in `ensure_meta`; `rel_digest` / `load` / `save` / `prune_unchanged_by_digest`; cold-path seed |
| B | wire `prune` into `tick_paths`; the two-line gate |
| C | `tests/digest_skip.rs` (comment-edit-no-rebuild + real-edit-rebuilds) |
| D (later) | incremental digest (v2); cold-`tick` digest skip; the Axis-B condensation cache |

A → B → C sequential; D deferred. A+B+C is one focused session.
