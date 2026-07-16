# Op-authoring surface — completion map

## Context

Reframed 2026-07-16. The goal is not "finish the call cutover" and not "count
down the 584 raw-SQL sites." It is **operator-authoring ergonomics**: authoring a
new operator should feel like defining a SolidJS computed — one self-contained
file, a reactive signature, zero SQL — with proper framework-owned SQL
underneath. This is the v3/v4 op-authoring design
([`v3-min-author-ops.md`], [`v3-plugin-author-surface.md`] in
`~/projects/sprefa-archive-20260701`), which v5 flattened away. The idea was
better there. Bring it forward.

The v3 thesis: **min-author-ops `(1, 0, 0)`** — one new file, zero existing
edits, zero central-enum growth per new op. Min-viable op = 4 slots
(`NAME`, `parse`, `pipe`, one capture). Framework owns all SQL; the audit
**fails** if an op file reaches the relations table via raw SQL. Effects typed
(`EffectKind { type Response }` + `Batcher<E>`), self-registered
(`inventory::submit`).

v5 already has the SolidJS half: `Ctx::scan` intercepts reads and records a
`DepKey` per row (the MobX/`computed` dep-capture model, not the React `useMemo`
declared-array model). What v5 lost is everything that made authoring cheap.

## Completion condition — min-author-ops for a new operator

DONE = a drill proves adding a new family/op is `(1 new file, 0 existing edits
excl. mod.rs, 0 central-enum growth)`, the op file contains **zero raw SQL**,
declares its schema + reactive signature, and the framework owns read / write /
DDL / dep-capture / reconcile.

```
v3 op-authoring shape                     v5 today                       task
─────────────────────────────────────────────────────────────────────────────
self-registers (inventory::submit)   ⇄  call_families() vec +        C1 register
                                         static CALL_X +
                                         call_input_rels() sets
framework owns SQL; raw SQL in op    ⇄  Ctx::scan IS                  C2 typed read
  = AUDIT FAIL                           format!("SELECT..FROM..")
declared schema() -> RowSchema → DDL ⇄  hand SQLITE_CALL_DELTA_SCHEMA  C3 schema→DDL
  + framework writer helper              + persist_sqlite_call_family    + C4 writer
typed effect + Batcher<E>            ⇄  (none)                        C5 signature
SolidJS read-capture deps            ⇄  DepKey capture  ✅ present     — already right
reconcile / retract render           ⇄  built, proven by rail  ✅      — already built
```

### Landed (branch `next`)
- **C1 self-registration** — DONE (`0f57ca1a`). `Family` trait gained `out_cols` +
  `input_rels`; families self-register via `register_family!` (inventory). The
  `static CALL_X` + `call_families()` vec + `call_input_rels()` set are gone
  (registry-collected / framework-computed); the render's per-family `match name`
  arm is gone (generic `tbl(name)` + `out_cols`). A new family = 1 file + 1 mod line.
- **C2 typed read** — already satisfied pre-C1. `Ctx::scan` is the framework's
  single SQL read site; op files contain zero raw SQL. No work needed.
- **C-proof** — DONE (`c7f16b4c`). Hosted `call_kind` + `call_edge_rev` as 2 new
  files + 2 mod lines, zero framework edits, byte-identical to legacy. The drill
  held: `(1 file, 1 mod line, 0 central-enum growth)` per family.
- **C6 no-raw-SQL audit rail** — DONE (`8451bfa4`). `.dl/rusqlite-coupling.dl`
  emits an ERROR `op-raw-sql` (exit 2) for any raw SQL in `src/engine/family/*.rs`
  (excl. `mod.rs`/`router.rs` + `#[cfg(test)]`). Proven clean on the 5 real
  families, fires on a planted `db.prepare(...)`. This is the "less SQL everywhere"
  enforcement, and it subsumes the 584-site canonicalization (route through the
  sanctioned helper the audit demands).

### Remaining
<!-- todo(feature): C3 declared schema — family declares schema() -> RowSchema; framework derives owned-table DDL + insert/retract, retiring hand-written SQLITE_CALL_DELTA_SCHEMA per rel (src/storage/call.rs). Bites only when a family needs a NEW owned input table (e.g. call_def_rev's extended _call_def) -->
<!-- todo(feature): C4 writer helper — owned-table persist + public-rel write route through one framework writer keyed off the declared schema, not per-family persist_sqlite_call_family/DDL (src/storage/call.rs) -->
<!-- todo(feature): C5 reactive signature — formalize SubscribePolicy/memo as a declared per-op reactive signature over the existing DepKey read-capture; reconcile/retract (built) is the render (src/engine/family/router.rs) -->

## Folded-in sub-goals (in service of the surface, not separate finish lines)

- **Call cutover** (families sole writer, live incremental render): now just the
  *demonstration* that the surface works end-to-end on the call family. Gated on
  hosting call_kind / call_edge_rev / call_def_rev and freezing 6-rel parity
  snapshots before deleting legacy. Do it as the C-proof capstone, not first.
- **584 raw-SQL sites** (was "north star B"): subsumed by C6. The off-ramp is the
  C4 writer helper; the audit rail forces sites through it. Worst offenders
  storage/call.rs (65), meta.rs (58), db.rs (48).

<!-- todo(feature): capstone — after C1-C6 land, host call_kind/call_edge_rev/call_def_rev and cut over to families-as-sole-writer with live react_deltas render, proving the surface end-to-end; freeze 6-rel golden snapshots BEFORE deleting legacy (legacy is the parity oracle) -->

## Anti-goals (from v3-min-author-ops.md, still binding)

- min-author-ops is **not** min-LoC. Do not collapse distinct author slots into
  one fat trait to cut files. Separate slots cost zero extra files.
- Do not hide typed effect responses behind `Box<dyn Any>` to "simplify." The
  typed `E::Response` is the point.
- Framework core wears the cost (where-clause ladder, downcast in `put<E>`) once,
  in one file. Op authors never touch it.

[`v3-min-author-ops.md`]: ../../sprefa-archive-20260701/v3/docs/v3-min-author-ops.md
[`v3-plugin-author-surface.md`]: ../../sprefa-archive-20260701/v3/docs/v3-plugin-author-surface.md
