# CozoDB storage-backend abstraction (for sprefa `RelStore`)

Source: `~/projects/ext/cozo`, crate `cozo-core`. All refs are `file:line` into that tree.

Cozo's storage layer is a **single flat key-value store** (`(Vec<u8>, Vec<u8>)`), one
physical table for everything. Relations are namespaced by a `u64` `RelationId` prefix on
the key; relational structure (which slots are keys, arity, validity) lives only in the
*memcomparable key encoding*, not in the storage trait. The trait is two halves: a
`Storage` factory (`Clone`, `Send+Sync`) and a per-transaction `StoreTx` (raw KV + range
scans). MVCC and ordering guarantees are pushed onto the backend.

## 1. `Storage` trait — `cozo-core/src/storage/mod.rs:31`

```rust
pub trait Storage<'s>: Send + Sync + Clone {
    type Tx: StoreTx<'s>;                                         // mod.rs:33
    fn storage_kind(&self) -> &'static str;                       // :36
    fn transact(&'s self, write: bool) -> Result<Self::Tx>;       // :39
    fn range_compact(&'s self, lower: &[u8], upper: &[u8]) -> Result<()>;  // :43
    fn batch_put<'a>(
        &'a self,
        data: Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + 'a>,
    ) -> Result<()>;                                              // :48
}
```

- Associated type `Tx: StoreTx<'s>` ties a backend to its transaction type.
- `Result` = `miette::Result`.
- `transact(write)` is the only tx constructor; the `write` bool selects read vs write
  lock and whether write ops are legal.
- `batch_put` is the bulk-load path: a pre-sorted, dedup'd, strictly-ascending iterator of
  KV pairs, with the contract "no other access while running" (`:46`). This is the one
  plural-shaped method, used for restore/import, not for normal query writes.

## 2. `StoreTx` (transaction) trait — `cozo-core/src/storage/mod.rs:56`

```rust
pub trait StoreTx<'s>: Sync {
    fn get(&self, key: &[u8], for_update: bool) -> Result<Option<Vec<u8>>>;          // :60
    fn multi_get(&self, keys: &[Vec<u8>], for_update: bool)
        -> Result<Vec<Option<Vec<u8>>>> { /* default = map get */ }                  // :65
    fn put(&mut self, key: &[u8], val: &[u8]) -> Result<()>;                          // :71
    fn supports_par_put(&self) -> bool;                                              // :74
    fn par_put(&self, key: &[u8], val: &[u8]) -> Result<()> { panic!(..) }            // :80
    fn del(&mut self, key: &[u8]) -> Result<()>;                                     // :85
    fn par_del(&self, key: &[u8]) -> Result<()> { panic!(..) }                        // :90
    fn del_range_from_persisted(&mut self, lower: &[u8], upper: &[u8]) -> Result<()>; // :95
    fn exists(&self, key: &[u8], for_update: bool) -> Result<bool>;                  // :100
    fn commit(&mut self) -> Result<()>;                                              // :104

    // scans — lower inclusive, upper exclusive
    fn range_scan_tuple<'a>(&'a self, lower: &[u8], upper: &[u8])
        -> Box<dyn Iterator<Item = Result<Tuple>> + 'a> where 's:'a { /* default */ }// :111
    fn range_skip_scan_tuple<'a>(&'a self, lower, upper, valid_at: ValidityTs)
        -> Box<dyn Iterator<Item = Result<Tuple>> + 'a>;                             // :139
    fn range_scan<'a>(&'a self, lower, upper)
        -> Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + 'a> where 's:'a;     // :148
    fn range_count<'a>(&'a self, lower, upper) -> Result<usize> where 's:'a;          // :157
    fn total_scan<'a>(&'a self)
        -> Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + 'a> where 's:'a;     // :162
}
```

Shape notes:

- **All access is single-key or range.** No table-name argument anywhere; the relation is
  encoded into the `lower`/`upper` byte bounds. There is no `insert_rows(table, cols,
  rows)` — writes are per-key `put` calls.
- **Range scans** are `lower..upper`, inclusive/exclusive. `range_scan_tuple` is the
  decoded form (`Result<Tuple>`, where `Tuple = Vec<DataValue>`); `range_scan` is the raw
  bytes form. `range_scan_tuple` has a default body (`mod.rs:119`) that wraps `range_scan`
  through `decode_tuple_from_kv`.
- **`range_count`** is a backend-native count over a byte range (SQLite delegates to
  `count(*)`, mem counts the merged iterator).
- **`total_scan`** scans the entire physical store (all relations) ascending.
- **`for_update`** on `get`/`exists`: in a write tx the backend must fail `commit` if that
  key changed outside the tx (the MVCC hook).
- **`par_put`/`par_del`**: a `&self` (non-`&mut`) write path gated by `supports_par_put`.
  Backends that lock per-call (SQLite) say true; the BTree mem backend says false because
  its write delta lives behind `&mut`.
- **`del_range_from_persisted`** deletes a byte range, used to drop a whole relation.

### memcomparable-key encoding assumption

The trait's entire correctness rests on: **byte-lexicographic order of keys == logical
tuple order.** Keys are built by `encode_as_key` (`cozo-core/src/data/tuple.rs:29`):

```rust
fn encode_as_key(&self, prefix: RelationId) -> Vec<u8> {
    ret.extend(prefix.0.to_be_bytes());     // 8-byte big-endian relation id, tuple.rs:32
    for val in self { ret.encode_datavalue(val); }   // memcmp per value, :35
}
```

- Relation prefix is `u64` big-endian, so all of one relation's rows form a contiguous
  byte range `[id .. id.next())` (`relation.rs:361-362`).
- Per-value encoding is the `MemCmpEncoder` trait (`cozo-core/src/data/memcmp.rs:45`,
  `fn encode_datavalue`): order-preserving byte encoding so a plain `&[u8]` comparator
  sorts tuples correctly. Validity (time-travel) sorts as the last slot, descending, which
  is what `range_skip_scan_tuple` exploits.
- Decode: `decode_tuple_from_key` (`tuple.rs:41`) skips the 8-byte prefix
  (`ENCODED_KEY_MIN_LEN`) and walks values. Values live split across key and value bytes:
  key columns in the key, non-key columns rmp-serialized in the value
  (`relation.rs:410`, `extend_tuple_from_v`).

The backend never sees columns or types. It only has to honor "compare keys as bytes".

## 3. Concrete backends

### SQLite — `cozo-core/src/storage/sqlite.rs:26`

```rust
#[derive(Clone)]
pub struct SqliteStorage {
    lock: Arc<ShardedLock<()>>,                          // many readers / one writer
    name: PathBuf,
    pool: Arc<Mutex<Vec<ConnectionThreadSafe>>>,         // connection pool
}
```

One on-disk table: `create table cozo (k BLOB primary key, v BLOB)` (`sqlite.rs:43`).
The whole KV store is `(k,v)` BLOB rows; the memcomparable `k` is the SQLite primary key,
so SQLite's btree gives the required ascending order for free.

`impl Storage` (`sqlite.rs:62`):

| method | one-sentence body | line |
|---|---|---|
| `transact` | pop a pooled connection, take read/write `ShardedLock` guard, `begin;` if write, build `SqliteTx` with 4 cached-statement slots | :65 |
| `batch_put` | open a write tx, `put` each pair in a loop, `commit` | :95 |
| `range_compact` | drain the connection pool (no real compaction) | :108 |
| `storage_kind` | `"sqlite"` | :114 |

`SqliteTx<'a>` (`sqlite.rs:119`) holds the lock guard, a borrowed `&SqliteStorage`, the
connection, an array of cached prepared `Statement`s, and a `committed` flag.
`unsafe impl Sync` (`:127`). Queries are a fixed string table `QUERIES` (`sqlite.rs:131`):
get / upsert / delete / exists / range / skip-range / count, all parameterized on `k` or
`k >= ? and k < ?`.

`impl StoreTx` (`sqlite.rs:180`):

| method | body | line |
|---|---|---|
| `get` | bind key to cached GET stmt, read one BLOB | :181 |
| `put`/`par_put` | bind (k,v) to upsert stmt (`on conflict do update`); `put` just calls `par_put` | :197 / :205 |
| `del`/`par_del` | bind key to delete stmt | :217 / :221 |
| `del_range_from_persisted` | `delete from cozo where k>=? and k<?` | :233 |
| `exists` | `select 1 ... where k=?` | :245 |
| `commit` | if write guard and not yet committed, run `commit;` | :258 |
| `range_scan_tuple` / `range_scan` | fresh prepared range stmt (not cached, can overlap), wrap in `TupleIter`/`RawIter` | :272 / :305 |
| `range_skip_scan_tuple` | `SkipIter` that re-seeks `next_bound` per validity hop | :289 |
| `range_count` | `select count(*) where k>=? and k<?`, read i64 | :320 |
| `total_scan` | `select k,v from cozo order by k` | :338 |

Drop (`sqlite.rs:149`): rollback if a write tx wasn't committed, then return the
connection to the pool.

### In-memory — `cozo-core/src/storage/mem.rs:40`

```rust
#[derive(Default, Clone)]
pub struct MemStorage {
    store: Arc<ShardedLock<BTreeMap<Vec<u8>, Vec<u8>>>>,   // mem.rs:41
}
```

A single `BTreeMap<Vec<u8>, Vec<u8>>` — the btree *is* the ascending-key invariant.

```rust
pub enum MemTx<'s> {                                                      // mem.rs:78
    Reader(ShardedLockReadGuard<'s, BTreeMap<..>>),
    Writer(ShardedLockWriteGuard<'s, BTreeMap<..>>,
           BTreeMap<Vec<u8>, Option<Vec<u8>>>),   // write delta: None = tombstone
}
```

`impl Storage` (`mem.rs:44`): `transact` takes read or write guard and (if write) an empty
delta map (`:51`); `batch_put` inserts straight into the map under the write lock (`:65`);
`range_compact` is a no-op (`:61`).

`impl StoreTx` (`mem.rs:86`):

| method | body | line |
|---|---|---|
| `get` | reader reads map; writer checks delta first, then map | :87 |
| `put`/`del` | writer inserts `Some(v)` / `None` into delta; reader errors | :97 / :117 |
| `supports_par_put` | `false` (delta is behind `&mut`) | :109 |
| `commit` | drain delta into the map, applying tombstones | :158 |
| `range_scan(_tuple)` | reader: `map.range(lower..upper)`; writer: `CacheIter`/`CacheIterRaw` merging delta + map | :179 / :231 |
| `range_skip_scan_tuple` | `SkipIterator` (reader) / `SkipDualIterator` (writer) honoring validity | :201 |
| `range_count` | count the merged iterator | :253 |
| `total_scan` | iterate whole map / merged | :269 |

The interesting machinery is `CacheIterRaw` (`mem.rs:285`): a two-way merge of the write
delta and the committed map, skipping tombstones and preferring delta on key equality —
this is how a write tx reads its own uncommitted writes in order.

## 4. How the engine calls through the trait

The query runtime never touches `Storage`/`StoreTx` directly except through `SessionTx`:

```rust
pub struct SessionTx<'a> {                                   // runtime/transact.rs:24
    pub(crate) store_tx: Box<dyn StoreTx<'a> + 'a>,          // the persistent backend tx
    pub(crate) temp_store_tx: TempTx,                        // a mem tx for temp relations
    ...
}
```

**Acquire** (`runtime/db.rs:872`):

```rust
fn transact(&'s self) -> Result<SessionTx<'_>> {            // read tx
    SessionTx { store_tx: Box::new(self.db.transact(false)?),
                temp_store_tx: self.temp_db.transact(true)?, .. }
}
fn transact_write(&'s self) -> Result<SessionTx<'_>> { .. self.db.transact(true) .. } // :882
```

So `Db<S: Storage>` calls `S::transact`, boxes the resulting `Tx` as a trait object, and
hands it to the runtime. The backend type is erased at this seam (`Box<dyn StoreTx>`).

**Scan a relation** (`runtime/relation.rs`): `RelationHandle` owns a `RelationId` and turns
a logical scan into a byte range, then calls the tx:

```rust
fn scan_all(&self, tx) -> impl Iterator<Item=Result<Tuple>> {     // relation.rs:357
    let lower = Tuple::default().encode_as_key(self.id);
    let upper = Tuple::default().encode_as_key(self.id.next());
    tx.store_tx.range_scan_tuple(&lower, &upper)                   // :366
}
```

Variants: `scan_prefix` (`:428`), `scan_bounded_prefix` (`:469`), point `get` (`:385`),
`exists` (`:419`), all building byte bounds from `DataValue`s via `encode_as_key`.

**Write rows** (`query/stored.rs:288`+): the eval loop iterates result tuples and calls
`store_tx.put(&key, &val)` once **per row** (`stored.rs:350`, temp at `:348`); index rows
are separate per-row `put`s (`:328`). This is exactly the N+1 write shape sprefa rejects.
Key/val come from `encode_key_for_store` / `encode_val_for_store` on the handle (`:300`).

**Commit** (`runtime/transact.rs:132`): `commit_tx` just forwards to `store_tx.commit()`.
The temp (mem) tx is dropped without an explicit commit.

## 5. Good vs awkward for a plural relational consumer

Good (worth borrowing):

- **Erased backend behind one trait object** (`Box<dyn StoreTx>`), backend chosen at
  `Db::new`. sqlite vs mem vs rocks differ only in this file.
- **`transact(write: bool)` + RAII drop = rollback** is a clean tx lifecycle; the
  `committed` flag + Drop rollback (sqlite.rs:149) is a tidy pattern.
- **Native `count` over a selector** (`range_count`) instead of materializing rows.
- **A real bulk path exists** (`batch_put`) — proof the seam *can* be plural; it just isn't
  the one the query engine uses.
- **Reader/writer split with read-your-writes** (mem `CacheIter`) is a good model if
  sprefa ever wants an uncommitted overlay.

Awkward for sprefa (drop these):

- **KV range-scan is the only read primitive.** Everything is `lower..upper` byte bounds;
  the table is implicit in the prefix. sprefa wants `scan(table)` / `exec(sql)` against a
  real schema, not byte ranges.
- **Per-row `put` in the eval loop** (`stored.rs:350`) — the canonical N+1. sprefa's whole
  posture is `insert_rows(table, cols, rows)` once per tick.
- **Memcomparable encoding is mandatory and load-bearing** for correctness; it pushes
  ordering, column-splitting (key bytes vs rmp value), and type erasure into the key codec.
  sprefa already has SQLite columns and doesn't want a hand-rolled order-preserving codec.
- **`unsafe impl Sync` + statement-lifetime `transmute`** (sqlite.rs:127/173) is the cost
  of the borrowed-`Statement` cache; a plural API that prepares per-call sidesteps it.
- **No table/column vocabulary** anywhere in the trait, so schema, types, and counts can't
  be expressed without going around it.

## Proposed sprefa `RelStore` trait

Borrow the erased-backend + tx-lifecycle, drop KV range-scan for a plural relational API.

```rust
/// One physical relational store (SQLite-welded today). Cheap-clone handle.
pub trait RelStore: Send + Sync + Clone {
    type Tx<'s>: RelTx where Self: 's;
    fn kind(&self) -> &'static str;
    fn transact(&self, write: bool) -> Result<Self::Tx<'_>>;   // RAII: drop = rollback
}

/// A transaction. Reads are by table; writes are plural-only (no per-row put).
pub trait RelTx {
    fn insert_rows(&mut self, table: &str, cols: &[&str], rows: &[Row]) -> Result<usize>;
    fn delete_where(&mut self, table: &str, pred: &Selector) -> Result<usize>;
    fn scan<'a>(&'a self, table: &str, sel: &Selector)         // sel = cols + bounds + order
        -> Box<dyn Iterator<Item = Result<Row>> + 'a>;
    fn count(&self, table: &str, sel: &Selector) -> Result<usize>;
    fn exec(&mut self, sql: &str, params: &[Value]) -> Result<u64>;  // metered escape hatch
    fn commit(&mut self) -> Result<()>;
}
```

Kept: erased backend + `transact(write)` + drop-rollback + native `count`. Dropped:
`get/put/del` raw KV, `range_scan(lower,upper)` byte bounds, memcomparable key encoding,
`par_put`/`for_update`/skip-scan/validity. `insert_rows` is the single write chokepoint so
the per-tick N+1 counter has exactly one place to watch.
