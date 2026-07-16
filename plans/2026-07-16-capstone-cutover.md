# Capstone cutover — families as sole writer + live react_deltas render

Planned by a Fable consult 2026-07-16. Makes the extraction families the SOLE
writer of the public call rels and flips the live render from full-reload
(`router.react`) to incremental retraction (`router.react_deltas`, built +
rail-proven but dormant). Retraction goes live: a retracted input row surfaces
as a retracted output row instead of being silently rebuilt.

## Three scope-changing discoveries (not in the original 6-rel framing)
- **A — 7th public rel `call_def`.** `rebuild_sqlite_legacy_call_rels`
  (src/storage/call.rs:767) writes `rel_call_edge` AND `rel_call_def(repo, sym,
  kind, file, line, end)`. `call_def` is read across anchor.rs, engine/lens.rs,
  engine/symbols.rs, verbs.rs, and user `.dl`. Cutover needs a `CallDef` family
  (rev-collapsed twin of `CallDefRev`, both reading the extended `_call_def`).
- **B — 3rd writer `sweep_gone_revs`** (src/engine/extract/mod.rs:1157). Runs per
  tick, deletes `call_def_rev`/`call_edge_rev` rows for gone revs directly from
  public tables (`REV_TWINS`, line 1122), then calls `rebuild_legacy_call_rels`.
  Under family-sole-writer this is a mixed-writer + memo/table desync. Convert to
  input-side retraction: delete gone-rev rows from the OWNED tables and let the
  router surface the public retraction (this is retraction going live).
- **C — `retract_rows` does not chunk** (src/storage.rs:72). One `DELETE ...
  WHERE (cols) IN (VALUES ...)` with `rows*cols` bound params, unlike
  `Db::insert_rows` (chunks by PARAM_BUDGET, db.rs:604). A rev disappearing =
  tens of thousands of rows = param-limit blowout. Fix before Phase 5.

## Phase map (each independently committable; P1-P3 verify BOTH DL_FAMILY_CALL states)
```
P0  fix retract_rows chunking (Discovery C)                  [isolated, additive]
P1  extend _call_def owned input + plumb full def rows       [additive, legacy live]
P2  host CallDefRev + CallDef families (7 total)             [additive, flip covers]
P3  freeze 7-rel golden snapshots (text-decoded)             [test-only, BEFORE delete]
P4  sole-writer cut: delete legacy public writes + gate +    [the cutover commit]
    convert sweep_gone_revs to input-side retraction
P5  render flip: react -> react_deltas (retract+insert)      [retraction live]
P6  cleanup: dead code, docs, CI collapse
```

## P1 — extend `_call_def` (prereq for call_def_rev/call_def families)
Owned `_call_def` goes from `(sym_sid, name_sid)` to 8 cols:
`(sym_sid, name_sid, repo_sid, kind_sid, file_sid, line, "end", rev_sid)`,
PK `(sym_sid, rev_sid)` WITHOUT ROWID. Migration: `ensure_sqlite_call_def_shape`
probes `PRAGMA table_info`; wrong col count => DROP (rebuilt wholesale every full
refresh). Bump `_call_delta_marker.schema_version` 1->2 (delta path bails
`Unsupported("call-schema-version")` until one full refresh rebuilds baseline).
`CallDefBaseline{repo,sym,name,kind,file,line,end,rev}` replaces `name_rows` as
`_call_def`'s source; `replace_sqlite_call_def` interns once + `insert_rows`
(plural, chunked). CallName keeps passing (scans only sym_sid/name_sid; Ctx::scan
is column-projected). def_rev_rows/name_rows keep being built (legacy still live).

## P2 — CallDefRev + CallDef families
- `call_def_rev.rs`: out_cols [repo,sym,kind,file,line,end,rev], input [_call_def],
  scan `_call_def` -> emit 7-tuple (rows distinct by (sym,rev) PK).
- `call_def.rs`: out_cols [repo,sym,kind,file,line,end], input [_call_def], dedup
  via HashSet (two revs collapse to one row).
- Rev-scoped emission = carry rev_sid as a column; no per-rev write helper. The
  derivation IS the full relation; reconcile computes rev-granular retraction free.
- Registry order becomes the 7-name sorted list; update assertions at
  storage/call.rs:1529, family/mod.rs:434,552 (call_def/call_def_rev/call_name
  stay absent from RERUN lists on owner-delta footprints — only cold list grows).
  Seed `delta_test_db` with `_call_def` rows; extend flip-parity to diff
  rel_call_def_rev + rel_call_def.

## P3 — freeze goldens BEFORE any legacy delete
`tests/fixtures/call_golden/<rel>.tsv`, 7 files, sorted text-decoded (sids joined
through _strings). Producer: `#[ignore]` regen test over a multi-rev git fixture
(WORK + >=1 committed rev pins the rev-union), flag-OFF legacy dump. Consumer:
live `call_family_matches_golden` under both flag states. Second carried oracle:
"incremental == cold rebuild" (replaces the legacy-engine v2 oracle in
family_delta_skips_call_name_on_site_only_change).

## P4 — sole-writer cut (one commit; every intermediate state is mixed-writer)
- `CallFamilyWrite` shrinks to owned-input-only (delete def_rev_rows/site_rows/
  edge_rev_rows/name_rows/kind_rows/revs). Delete `rebuild_legacy_call_rels`,
  `replace_sqlite_call_revs`, `family_flip_enabled`, all DL_FAMILY_CALL reads.
- `refresh_call_rels`: keep resolve_callee/caller + owners + def_buckets + defs;
  delete public-row accumulation; flip call becomes UNCONDITIONAL.
- `reproject_sqlite_call_affected_keys` -> keep `_call_edge_support` recompute
  (owned), delete the 4 public-rel DELETE/INSERT blocks.
- `sweep_gone_revs`: drop call rels from REV_TWINS + drop rebuild call; add
  `sweep_gone_call_inputs` = set-diff DELETEs on the 6 owned tables for
  `rev_sid NOT IN _live_rev_scope` (explicit deletes, FKs proven off-able), then
  flip if anything moved.

## P5 — render flip (the diff)
`react_deltas` contract: return EVERY rerun family incl. empty deltas (callers
filter; preserves rerun-name observability the skip asserts need). Render:
```
cold = families with no memo this process (fresh DB or restart)
deltas = router.react_deltas(db, changed)      // one tx around the whole render
for (name, delta) in deltas:
    cols = router.family(name).out_cols()
    if cold.contains(name): reload_rel(tbl(name), cols, router.rows(name))   // authoritative
    else: retract_rows(tbl(name), cols, delta.retracted); insert_rows(tbl(name), cols, delta.inserted)
```
RowDelta -> retracted = chunked row-value DELETE, inserted = chunked INSERT OR
IGNORE, both full-tuple identity matching reconcile's row_key. Cold rule is the
guard: empty memo => reload (INSERT OR IGNORE can't remove stale rows).

## Risks
1. Rev-scoped rels under retraction — semantics move to "derivation = owned rev
   set"; sweep_gone_call_inputs keeps it honest; multi-rev golden pins it.
2. Owned-table delta path stays intact — only the 4 public blocks die;
   `_call_edge_support` retained (deleting it is a separate follow-up).
3. Cold load / restart — empty memo => reload, never delta-apply. The `cold` set.
4. Parity tests lose oracle — replaced by frozen goldens + incremental==cold
   self-oracle + delta_test_db's frozen-copy SQL.
5. retract_rows param-limit (Discovery C) — P0.
6. Digest-gate early return + persistent DB — no flip, tables correct from disk,
   memo empty, first change cold-loads. Cold-restart rail proves it.

## Blockers (all resolved in-plan)
- rel_call_def orphaned by legacy delete -> P2 CallDef family.
- sweep_gone_revs unlisted writer -> P4 input-side conversion.
- _call_def on-disk migration -> shape probe + schema_version bump.

Critical files: src/storage/call.rs, src/engine/extract/call.rs,
src/engine/extract/mod.rs, src/engine/family/router.rs, src/engine/family/mod.rs.
