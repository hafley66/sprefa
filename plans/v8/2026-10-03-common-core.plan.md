# Common core: ids, interning, arenas, row blocks across dl8, sqlite_ivm, ryi

User (2026-10-03): "interning in rust and arenas and storage, if they all three had some kind of
common core ... scaling and decomposing this is a bitch". Boundary agreements this builds on:
`.claude/skills/2026-10-03-ryi-sprefa-boundary/SKILL.md` (content-hash ids, `dict(id, text)`,
`.tsp` schema truth, kellnr for cross-repo crates).

## Today: four dictionaries for the same strings and terms

| where | text | terms | ids |
|---|---|---|---|
| dl8 `Universe` (`src/_6_eval/_0_term.rs`) | lasso `Rodeo` + spans | `IndexSet<Term>` (hashbrown) | dense u32 per process |
| ivm-ir `Interner` (`crates/ivm-ir/src/1_terms.rs`) | `Arc<str>` maps | `(functor, args) -> Cell` | dense counter per engine |
| ivm-sqlite | `ivm_term_dict`, `ivm_ctor_*` tables | ctor tables | SQLite rowids |
| ryi / scm++ (`hafley_scm`, `sprefa-extract`) | `scmpp_dict_*` tables | - | dense per run DB (moving to content hash) |

Cost of four: every boundary translates. Measured in this repo: `decode_terms` and
`intern_snapshots` exist only to move terms between the dl8 `Universe` and the engine interner
(`plans/v8/2026-10-03-dd-speed.findings.md` round 2 item 5: 12708 -> 3685 profile units after
batching, still nonzero); `Engine::intern_terms` / `intern_text` round trips on every comptime
round; the lab exported the same facts three times (SQLite, DuckDB, Parquet).

## Decision (user, 2026-10-03)

"ast-grep and tree sitter and biome all have mastered string interning; we will be absolutely zero
not making our own; we will pick one." No bespoke interner or arena. Prior art:

| tool | text | trees / terms |
|---|---|---|
| tree-sitter / ast-grep | no interning; node text = byte range into the source; kinds = grammar symbol ids (u16) | spans only |
| Biome `biome_rowan` | `TokenText` | `NodeCache` interns green tokens and nodes, recursive hashing, structural sharing |
| rust-analyzer `ra_ap_intern` | `Symbol` | `Interned<T>` for any Hash+Eq, global Arc-based, GC; 0.0.356 (2026-10-01); small deps |

Picks: source text = spans (done in dl8); in-memory text + terms = one library, `ra_ap_intern`
(Symbol + Interned<Term>) or `lasso` (dense u32 keys, already in dl8), decided by the spike;
persisted ids = xxh3-64 content hash written only at the storage boundary (a hash function, not an
interner). The shape below is superseded where it says to write arenas: use the picked library.

## Shape (pre-decision draft)

One small crate (working name `hafley-core`, published on local kellnr, no engine, no SQLite):

1. `Id(u64)`: content hash. Text: hash of the bytes. Term: hash of (functor id, arg ids) (Merkle,
   so a term's id is the same in every process, run and worktree). Collision checked on insert
   (named error, no fallback).
2. `TextArena`: append-only byte arena, `Id -> (offset, len)` map; `&str`/`&[u8]` views, never
   copies on read. Spans over loaded source blobs are a second kind of entry (blob id, start, end).
3. `TermArena`: append-only `(functor: Id, args: [Id])` records, `Id -> record` map.
4. `RowBlock`: columnar block of `Id`/`i64`/`f64` columns for one relation (Arrow-shaped: one
   buffer per column). The unit every store reads and writes and every engine ingests.
5. Store adapters live outside the crate, one per format: SQLite (rusqlite), Parquet, DuckDB CLI.
   Each maps `RowBlock` <-> its format and writes `dict(id, text)` / `term(id, functor, args)`
   once per new id.

Engines keep their own internal layout if they need it (DD wants dense u32 in arrangements?
measure), but ids crossing any boundary are the u64 content ids, so no translation table.

## Instance timelines

- Process start: arenas empty. Loading a store or a source file appends entries; ids are pure
  functions of content, so loading the same file twice is a no-op.
- Comptime: dl8 lowering, IR and engine share one `TermArena` handle per process (no
  decode/intern round trips). Engine on another process (ryi daemon): ids travel as u64; the
  receiver asks for missing `dict`/`term` rows only.
- Arenas never free during a run; a run ends with the process. Watch mode: arena grows; a
  compaction (rebuild from live ids) is a later step, measured first.

## Storage, reads, writes, uniqueness

- Uniqueness: one entry per `Id`; `Id` unique by content (hash + collision check).
- Write: a store adapter writes a relation as `RowBlock`s; new ids append to `dict`/`term` tables
  (`INSERT OR IGNORE` on the id PK in SQLite; append-only Parquet file per flush).
- Read: adapter yields `RowBlock`s; the reader looks up only ids it does not have.

## Build vs buy (candidates; versions and fit to verify before the lane starts)

| need | candidates | note |
|---|---|---|
| string interner | `lasso` (in dl8), `string-interner`, `ustr`, `internment` | all give dense or pointer keys; none gives stable content-hash ids out of the box; `hashbrown::HashTable<Id>` over a byte arena may be the whole thing |
| hash function for ids | `xxhash-rust` (xxh3-64), `foldhash`, `rustc-hash` | ids are persisted: needs a fixed, documented, seedless hash; xxh3-64 fits |
| arenas | `bumpalo`, `typed-arena`, `la-arena` (rust-analyzer) | append-only bytes + `Vec<record>` may be enough |
| hash-consing | `hashconsing` crate | check whether it supports content-hash (not pointer) ids |
| columnar blocks | `arrow-rs` (`arrow-array`, `arrow-buffer`), `parquet` | arrow is the interchange DuckDB/DataFusion/Parquet already speak; heavy compile: keep it in the adapter crates, the core uses plain `Vec<u64>` columns shaped like arrow |

## Steps (one fresh agent each, sequential)

1. Spike: `lasso` (dense u32) vs `ra_ap_intern` (Symbol / Interned<Term>, pointer handles) in the
   dl8 `Universe` and ivm-ir/ivm-dd interner on the c15 and registry builds (time, RSS, compile
   time). Picks the one library.
2. Thin crate: re-exports the picked library, `RowBlock`, the xxh3 persisted id; kellnr; tests only.
3. dl8 + ivm-ir share one `TermArena` (delete `decode_terms` / `intern_snapshots` round trips).
4. Store adapters: SQLite + Parquet readers/writers; dl8 import of external tables through them
   (unblocks the recursion lab's arm S).
5. ryi phase 2 interning adopts the same `Id` (ryi side, their lane).
