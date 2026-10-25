# v9 dict: what it does, every other place text lives, library candidates

## 1. What `Dict` does today (`v9/src/_0_id.rs`)

| part | code | job |
| --- | --- | --- |
| `id_of(bytes)` | blake3, first 8 bytes as `u64` | content id: same text, same id, in every process and on disk |
| `bytes: HashMap<u64, Box<[u8]>>` | one heap box per distinct text | id -> text, for printing and sending |
| `intern` | hash, compare on hit, panic on a different text under one id | text enters once; collisions stop the run |
| `fresh: Vec<u64>` + `take_new` | ids interned since the last write | the store writes only new dict rows (`_8_runtime/_3_core.rs:366`) |
| `load` | fill from the SQLite `dict` table | open replays every stored text into memory (`_7_store/_1_sqlite.rs:153`) |

Rows in DD and in SQLite hold only `u64`. Text appears at the edges.

## 2. Every other place text or bytes live (v9 at 93b878083)

| place | form | copies per text |
| --- | --- | --- |
| ryi answer path (`_1_effects.rs:20-79`) | whole stdout `Vec<u8>` (22.8 MB on hafley_scm) -> `serde_json::Value` per line -> `format!("{path}#{name}")` `String` per caller/callee (`CallEdge`) -> `Dict::intern` copies into a `Box<[u8]>` | 4 before the dict keeps one |
| client commit (`_3_core.rs:162-220`) | `Cell::Text(String)` per cell from the wire; hashed, then interned | 2 (wire JSON, `String`), then the dict copy |
| client read (`_3_core.rs:495`) | `Cell::Text(String::from_utf8_lossy(..).into_owned())` per text cell of every Delta | 1 new `String` per cell per answer |
| SQLite `dict(id, bytes)` | BLOB per text | the durable copy; the in-memory `HashMap` is a second full copy after open |
| program metadata (`_2_ir.rs`, `_3_plan.rs`, `Core::ids`) | rel/col/var names as `String`, `HashMap<String, RelId>` | small, fixed per program |
| `Ryi::answered` (`_8_runtime/_2_ryi.rs:21`) | ids only | 0 |

## 3. Plan vs built

`plans/v9/2026-10-21-v9-core-types.md` sec. 2 planned `Id` (hash) + `Sid` (dense u32, minted per process) + `Dict` with "hashbrown maps, bytes in one arena", `intern -> (Id, Sid)`. Built: hash ids only, no `Sid`, one `Box` per text, std `HashMap`. Principle 5 ("text lives in one dictionary collection") is not a DD collection either; it is a Rust side map.

## 4. Candidates (from memory; versions and APIs to be checked on crates.io/docs.rs before a pick)

| candidate | what it gives | ids | fits v9 | misses |
| --- | --- | --- | --- | --- |
| `lasso` (`Rodeo`, `ThreadedRodeo`, `RodeoReader`) | arena-backed `str` interner, dense keys | dense `u32` (`Spur`) | the planned `Sid`; arena storage | `str` only (text, not bytes); no content hash, so a `u64 -> Spur` map rides beside it |
| `string-interner` | interner with pluggable backends (bucket, string, buffer) | dense symbols | same role as `lasso`, backend choice for memory | `str` only; same missing hash |
| `ustr` | global interned `str` with a precomputed hash | pointer-sized handle | cheap compare/hash | process-global, never freed; not stable across processes |
| `internment` (`Intern`, `ArcIntern`) | global or refcounted interning of any `T: Hash + Eq` | pointer | `ArcIntern` frees unused text | pointer ids, not storable |
| `bumpalo` / `typed-arena` | arena for the byte storage under the current `HashMap<u64, &[u8]>` | n/a (keep blake3) | the "bytes in one arena" half of the plan | no freeing short of dropping the arena |
| `hashbrown` raw/entry API + one `Vec<u8>` | `u64 -> (offset, len)` into one buffer | keep blake3 | smallest change to `Dict` | semi-bespoke |
| `arrow` (`StringViewArray` / `BinaryViewArray`, `DictionaryArray`) | columnar text batches | dictionary keys | principle 10 tier 1 (shared memory, Arrow C Data Interface) for answer batches | a column format, not an interner; batch-oriented |
| `fst` | compact sorted set/map, memory-mapped | ordinal | frozen dictionaries (stdlib names, past snapshots) | immutable once built |
| SQLite read-through + `quick_cache` / `moka` | dict stays on disk; bounded in-memory cache by id | keep blake3 | removes the whole-table load at open (SLOP P3) | a lookup per cache miss at print edges |
| DD collection `(id, bytes)` arranged by id | the dict as a dataflow relation; text joined in only where read | keep blake3 | principle 5 as written | text bytes live inside DD batches; needs a byte-friendly container |

## 5. Questions for a pick

1. Does the dict need bytes (non-UTF-8), or is text enough? `lasso`/`string-interner`/`ustr` are `str` only.
2. Is the dense `Sid` still wanted inside the engine (smaller rows, ordered keys), or do blake3 `u64`s stay end to end?
3. Should the in-memory dict hold everything, or only a bounded cache over the SQLite table?
4. Is Arrow the batch format for service answers (ryi), which would decide the shape of the copy chain in sec. 2?
