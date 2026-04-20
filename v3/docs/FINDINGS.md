# v3 perf ∩ plugin findings

Teaching document. Every measurement here is load-bearing for v3
design choices. Every lever was probed on the linux kernel corpus
(63,482 .c/.h files, 1.34 GB, ast-grep pattern `printk($$$)` for the
scan leg; regex-extracted string literals for the write leg).

Organization:

1. Baseline race
2. The three perf levers
3. v3 effect-dispatch surface
4. Topology taxonomy (four batchers)
5. Perf affirmation under the plugin arch
6. sqlite write-leg baseline
7. Design rules that fell out
8. Followups queued for v3 proper

Measurement machine: Apple M2 class, 8 effective workers, macOS Darwin
24.1. All numbers are p50 of 3 to 5 trials unless stated.

---

## 1. Baseline race

Four scanners over the linux kernel, same corpus, same pattern kind
(C `printk` call sites, 16,627 matches).

| Tool | Wall | Aggregate MB/s | Shape |
|---|---|---|---|
| `grep -rc` | 14.2 s | 95 | single-core byte scan |
| `rg -c` | 1.7 s | 789 | parallel SIMD byte scan |
| `ast-grep` CLI 0.42 | 15.7 s | 85 | parallel tree-sitter parse |
| sprefa probe (initial W=8) | 20.7 s | 65 | crossbeam bounded queue + 8 workers |

Observation: tree-sitter parse caps near 13 MB/s per core on the C
grammar, regardless of binding (Rust, C, Go, Zig). Swapping the binding
has zero effect on raw parse throughput because parsing happens inside
the C runtime library and the generated C parser table.

The sprefa probe at 20.7 s sat 32% behind ast-grep CLI on identical
parse work. The question was where the gap lived.

---

## 2. The three perf levers

### 2.1 Topology (outcome: flat)

Two topologies measured side by side on the same corpus:

- **hopp-v2** — `crossbeam_channel::bounded(cap)` producer thread,
  W worker threads consuming batches via a swappable `BatchPolicy`.
  Push-driven, backpressure lives in the bounded channel.
- **walk-parallel** — `rayon::par_iter` over the pre-enumerated
  `Vec<PathBuf>`. Pull-driven, split-and-assign, work-stealing.

```
W=1   hopp-v2 19.75s   walk-parallel 20.32s
W=4   hopp-v2 5.75s    walk-parallel 5.59s
W=8   hopp-v2 3.91s    walk-parallel 3.64s
```

Result: within trial noise. Topology swap was a wash for homogeneous
CPU-bound parse work.

Explanation: both topologies end up with W worker threads each holding
one file at a time. The crossbeam queue cost amortizes to a few hundred
ns per item, negligible next to the ~2 ms parse. Work-stealing avoids
the MPMC coordination but splits the already-known path range instead,
which costs a comparable amount of bookkeeping.

RSS: walk-parallel came in ~25% lighter at W=8 (161 MB vs 221 MB),
because rayon holds fewer intermediate buffers than a saturated
crossbeam inbox at cap=4096.

### 2.2 Allocator pin (outcome: RSS win, wall flat)

Swapping the global allocator to jemalloc via `tikv-jemallocator`:

```
W=8 RSS   system malloc 639 MB  → jemalloc 466 MB  (-27%)
W=8 wall  no change within noise
```

Tree-building workloads (tree-sitter CST, CST walker intermediates)
allocate heavily; jemalloc's arena layout reduces fragmentation peak.
Wall throughput held steady because the existing allocator was fast
enough on the hot path; RSS was where the pressure showed.

Biome's bench crate pins jemalloc at the top of every benchmark file
for the same reason (`crates/biome_js_parser/benches/js_parser.rs:11-25`).

### 2.3 Fixed-string prefilter (outcome: 6× speedup)

The lever. Source: `ast-grep/crates/cli/src/utils/mod.rs:157-160`.

```rust
let fixed = matcher.fixed_string();
if !fixed.is_empty() && !file_content.contains(&*fixed) {
    return None;
}
```

`Pattern::fixed_string()` returns the longest non-metavar literal
substring the pattern requires. For `printk($$$)` the fixed string is
`"printk"`. ast-grep runs `file_content.contains("printk")` (SIMD byte
search) before invoking the tree-sitter parse. Files lacking the
literal skip parse entirely.

On the linux kernel, most .c/.h files lack `printk`. Running the
prefilter via `str::contains` is ~100× cheaper than parsing them.

Wiring the same line into the sprefa probe's `scan_one`:

```
W=8 walk-parallel no prefilter    23.0 s   58 MB/s
W=8 walk-parallel + prefilter     3.64 s   369 MB/s
                                  6.3× speedup
```

Match count bit-identical before/after: 16,627. The prefilter is
correctness-preserving for metavar patterns because `fixed_string()`
extracts a literal that a matching file is required to contain. Any
file missing the literal cannot produce a match.

This lever alone closed the gap to ast-grep CLI and passed it:

```
ast-grep CLI       15.7 s
sprefa probe W=8   3.64 s   (4.3× faster than ast-grep CLI)
```

The sprefa probe pulls ahead of ast-grep CLI because the probe skips
the CLI's output formatting, per-file `SgLang::from_path` dispatch,
and json/colorized printer. The prefilter evens the parse cost; the
absence of output work is the remaining gap.

### 2.4 Hypotheses ruled out along the way

- Different binding language (Rust, C, Zig, Go): parse work is inside
  the same C parser table. Same throughput across bindings.
- mmap vs `read_to_string`: ast-grep CLI uses `read_to_string` too.
  UTF-8 validation cost is uniform across both paths.
- Adaptive batching: walk-parallel (zero batching) tied hopp-v2.
  Batching axis was a wash on homogeneous CPU work.

---

## 3. v3 effect-dispatch surface

The effect system lives in `crates/effect_runtime/src/lib.rs`.

### 3.1 Two traits, one registry

```rust
// Per effect kind. One struct per effect, one impl per effect kind.
pub trait EffectKind: Send + 'static {
    type Response: Send + 'static;
}

// Owns dispatch for one effect kind. Batching and concurrency policy
// live inside `run`; the op author implements one of these per
// (effect, topology) pair.
pub trait Batcher<E: EffectKind>: Send + Sync + 'static {
    fn run(&self, req: E) -> BoxFuture<'static, E::Response>;
}
```

Registration is a single call:

```rust
let ctx = RtCtxBuilder::new()
    .register::<ScanFile, _>(Passthrough::new(count_fn))
    .register::<InsertHits, _>(BoundedBatched::new(16, 8, 1, insert_fn))
    .build();
```

Emission is typed end to end:

```rust
let (bytes, matches): (u64, u64) = ctx.put(ScanFile { path }).await;
```

`Box<dyn Any>` lives inside the framework only, behind `TypedEntry<E,
B>`. Op authors observe `E` in and `E::Response` out. No downcast
visible at the call site.

### 3.2 rxjs analogy

Each effect kind maps to a `Subject<E>`. Each batcher maps to an
operator chain the subscriber applies:

```js
// Passthrough
effects$.pipe(concatMap(run)).subscribe();

// WorkSteal
effects$.pipe(mergeMap(run, { concurrency: W })).subscribe();

// BoundedWorkSteal
new Subject({ buffer: cap })
    .pipe(mergeMap(rayonSpawn, { concurrency: Unlimited }))
    .subscribe();

// BoundedBatched
new Subject({ buffer: cap })
    .pipe(bufferCount(max_batch), mergeMap(runBatch, { concurrency: W }))
    .subscribe();
```

Backpressure is the bounded-Subject layer. Topology is the operator
chain. Effect authoring stays independent of both.

### 3.3 Typed erasure surface

Framework private, op-invisible:

- `BatcherEntry` trait with `submit(Box<dyn Any>) -> BoxFuture<Box<dyn Any>>`
- `TypedEntry<E, B>` implements `BatcherEntry` by downcasting `Any`
  into `E` inside `submit`, running the concrete batcher, and boxing
  the response
- `RtCtx::put<E>` boxes the request, looks up the entry by
  `TypeId::of::<E>()`, awaits the erased future, and downcasts the
  response back to `E::Response`

Cost per `put` at this layer:

- `Box<dyn Any + Send>` allocation for the request (~100 ns)
- `HashMap<TypeId, Arc<dyn BatcherEntry>>` lookup (~50 ns)
- Downcast inside `TypedEntry::submit` (~50 ns)
- Response boxed and downcast again (~150 ns)

Total ~350 ns per put, plus the topology-specific cost.

---

## 4. Topology taxonomy

Four batchers cover the axis cross-product of (queue vs queueless) ×
(coalesce vs single-item).

| Batcher | Queue | Concurrency | Coalesce | Use for |
|---|---|---|---|---|
| `Passthrough` | absent | caller task | absent | cheap sync compute, control case, batch-granularity handlers that fan internally |
| `WorkSteal` | absent | rayon pool | absent | known-bounded inputs, e.g. Vec of paths you par_iter over from inside a handler |
| `BoundedWorkSteal` | tokio mpsc(cap) | rayon pool | absent | CPU effects emitted at unknown rate by streaming ops; default for v3 scan/parse/match |
| `BoundedBatched` | crossbeam bounded(cap) | W threads | drain up to max_batch | amortizing effects (sqlite tx, git ODB writes, network RPC) |

### 4.1 When backpressure applies

Backpressure means a producer's `put().await` yields until the
downstream can accept. It applies when:

- Producer rate can exceed consumer rate
- Unbounded buffering would OOM

For v3 ops streaming through a pipeline, both apply. Every effect
emitted by a streaming op needs a bounded inbox. `Passthrough` and
`WorkSteal` omit the inbox and suit only the known-input case
(one-shot handler, pre-collected work).

### 4.2 Independence across kinds

Each kind lives in its own entry in the registry, each entry owns its
own inbox and workers. A slow `SqlInsert` handler blocks emitters of
`SqlInsert` only. Emitters of `ScanFile` keep flowing. No head-of-line
blocking across kinds.

### 4.3 Topology chosen by wiring, not by op

Op code:

```rust
let r = ctx.put(ScanFile { path }).await;
```

Same line compiles against all four batchers. The LSP process can wire
`ScanFile` to `Passthrough` for zero-overlap diagnostics. The daemon
can wire it to `BoundedWorkSteal` for concurrent scan. The op stays
identical.

---

## 5. Perf affirmation under the plugin arch

### 5.1 Per-file emission: 14% overhead

One `ctx.put(ScanFile { path }).await` per file, dispatched via
`BoundedWorkSteal`.

| Path | W=8 p50 | MB/s | Files/s |
|---|---|---|---|
| probe walk-parallel (raw par_iter) | 3.64 s | 369 | 17,451 |
| v3 BoundedWorkSteal (ctx.put per file) | 4.17 s | 322 | 15,246 |

Gap = 530 ms over 63,482 files = ~8.4 µs per-item overhead. Cost split:

- 1× tokio mpsc send().await roundtrip (~500 ns)
- 1× oneshot alloc + drop (~300 ns)
- 1× rayon spawn at item granularity, rather than range-split (~1–2 µs)
- 1× Box<dyn Any> alloc for erasure (~200 ns)
- Rest: atomic counters in mpsc send, task wake/park

Swept `cap ∈ {256, 1024, 4096}` and `submitters ∈ {16, 32, 64}`; the
curve was flat. The overhead is inherent to per-item dispatch through
the plugin surface.

### 5.2 Batch emission: parity

One `ctx.put(ScanBatch { paths: all_paths }).await`, handler uses
`par_iter` internally via the global rayon pool.

| Path | min | p50 | max | MB/s |
|---|---|---|---|---|
| walk-parallel probe | 3.80 s | 3.93 s | 4.13 s | 341 |
| v3 ScanBatch via Passthrough + par_iter | 3.57 s | 3.65 s | 3.68 s | 368 |

v3 batch came in 7% ahead of the raw probe in this run; within noise
bands the two are tied. The batch shape collapses the 8.4 µs × 63,482
per-item tax into a single ~8 µs tax per op invocation.

### 5.3 Rule of thumb

- **Batch emission** when the op knows the input set up front (scan a
  repo, rescan N files, rebuild index). One put per op invocation,
  handler fans to rayon internally. Overhead rounds to zero.
- **Per-file emission** when items arrive asynchronously (file
  watcher, LSP per-change, sporadic user action). The ~8 µs/item tax
  is amortized against the arrival interval. `BoundedWorkSteal` keeps
  the inbox bounded and propagates backpressure automatically.

Both surfaces compile against the same `Batcher<E>` trait; choice is a
single-line decision in `RtCtxBuilder::register`.

---

## 6. sqlite write-leg baseline

Workload: extract every C string literal from the linux kernel via
regex, insert tuples `(file_id, byte_start, byte_end, value)` into
sqlite.

Numbers: 1,547,529 rows inserted, DB size 59 MB, 63,482 files scanned.

### 6.1 Breakdown

| Stage | Wall p50 | Throughput |
|---|---|---|
| Scan only (regex extract, no plugin emit) | 1.74 s | 890k hits/s |
| Scan + plugin emit with writer no-op | 1.73 s | 896k hits/s |
| Full pipeline (extract + sqlite insert) | 2.04 s | 758k rows/s |

Isolated sqlite insert throughput: 2.04 s − 1.73 s = ~310 ms for
1.55 M rows = **~5 M rows/sec**.

The whole-pipeline number is scan-limited. sqlite sits idle waiting
for the regex extractor to feed it. When the upstream can feed at 5 M
hits/sec or higher, the write leg absorbs it at the rate measured
above.

### 6.2 PRAGMA stack

```
page_size    = 8192      (set before any page is written)
journal_mode = WAL       (concurrent read with a single writer)
synchronous  = OFF       (benchmark mode; prod would be NORMAL)
temp_store   = MEMORY
mmap_size    = 1 GiB
cache_size   = -262144   (256 MiB)
locking_mode = EXCLUSIVE (skips reacquiring file lock per tx)
```

### 6.3 Statement shape

Multi-value INSERT, 512 rows per stmt execution, `prepare_cached` reused
across calls:

```sql
INSERT INTO strings (file_id, byte_start, byte_end, value)
VALUES (?,?,?,?), (?,?,?,?), ..., (?,?,?,?)
```

sqlite caps bind variables at 32,766 per statement (4 params × 512
rows = 2,048; well inside the limit). Rows beyond the last full chunk
go into a tail statement prepared on demand.

### 6.4 Transaction shape

- `BoundedBatched` coalesces up to 8 consecutive puts into one
  handler invocation.
- Handler runs one `BEGIN/COMMIT` per coalesced batch.
- With chunk size 256 files and typical 6,000 hits per file chunk,
  each coalesced tx holds ~50,000 rows.
- 1.55 M rows / 50k per tx = ~31 transactions total.

### 6.5 Plugin arch overhead in the write path

`ctx.put(InsertHits { hits })` passes:
- Box alloc for `InsertHits` payload (~100 ns)
- oneshot alloc for reply (~100 ns)
- crossbeam send into bounded writer inbox (~200 ns)
- worker dequeues, runs handler, reply sent (~200 ns)

Total ~600 ns per put. Per-batch overhead, not per-row. Amortized over
50k rows per tx: 12 picoseconds per row. Rounds to zero.

### 6.6 Correctness check

`assert_eq!(emitted_rows, on_disk_count)` passes every trial. The
framework preserves per-request reply identity through the batch
coalescer: each put's oneshot is tagged to its submission, not its
position in the coalesced batch.

---

## 7. Design rules that fell out

Short, opinionated. Each rule is backed by a measurement above.

1. **Prefilter before parse.** Any pattern with a non-metavar literal
   of length ≥ 2 gets a `.contains(fixed)` gate before CST walk.
   Strictly preserving; 6× speedup on sparse-match corpora.
2. **Pin jemalloc on any binary doing tree work.** RSS drops ~30%,
   wall stays flat. Cost: one line at the top of the binary.
3. **Topology follows work shape, chosen at registration.**
   - homogeneous CPU: `BoundedWorkSteal` (bounded inbox + rayon)
   - amortizing I/O (sqlite, git, RPC): `BoundedBatched`
   - cheap sync / batch-inner-parallel: `Passthrough`
4. **Emit at natural op granularity.** Scan-a-repo emits one
   `ScanBatch`. Watch-a-file emits one `FileChanged` per event. Per-item
   emission through the plugin surface has an 8 µs tax; batch
   emission has a per-op-invocation tax that rounds to zero.
5. **One bounded inbox per effect kind.** Each kind gets its own
   queue. Slow kinds block their own emitters; other kinds flow.
6. **sqlite writer is one thread.** sqlite serializes writes at the
   file lock. Parallel writer threads trade throughput for contention.
   Parallelism goes upstream of the writer (multiple extract workers
   feeding one writer).
7. **sqlite inserts use multi-value INSERT + one tx per coalesced
   batch.** Per-row `prepare_cached` loops land at ~720k/sec; 512-row
   multi-value + coalesced tx lands at ~5 M/sec isolated.
8. **Op code stays topology-agnostic.** `ctx.put(E).await` compiles
   against every batcher. Swapping topology is a one-line edit in the
   wiring, and the op remains untouched.

---

## 6.5 Git blob-walk baseline (git2 vs shell-out)

Workload: walk every blob reachable from HEAD, read its bytes, count
byte occurrences of the literal `"printk"`. Linux kernel, 93,866 blobs,
1.59 GB of inflated content, 38,095 matches.

Two paths measured:

- **Path A: git2** — `Repository::open` → `head().peel_to_tree()` →
  `tree.walk(PreOrder)` collecting blob oids → per-oid
  `odb.read(oid)` under a `Mutex<Repository>`. This mirrors
  `v2/src/readers/_2_git.rs::GitBlobReader`.
- **Path B: shell-out** — one `git ls-tree -r HEAD` subprocess to
  collect oids (parse lines, keep blob rows only) → one long-lived
  `git cat-file --batch` subprocess piped stdin ← oids, stdout →
  frames `<oid> blob <size>\n<bytes>\n`. Writer thread feeds stdin,
  reader thread on main parses frames.

| Path | Wall p50 | MB/s | Blobs/s |
|---|---|---|---|
| git2 + `odb.read` per blob | 4.20 s | 360 | 22,354 |
| shell-out `ls-tree \| cat-file --batch` | 1.98 s | 765 | 47,435 |

Shell-out is 2.12× faster. Bit-identical blob count, byte total, and
match count across both paths.

### Why shell-out wins

1. `git cat-file --batch` reads the pack index once and walks it with
   internal state reuse. Each `odb.read` from git2 reacquires the
   pack object handle, crosses an FFI boundary (`git_odb_object_new`,
   `git_odb_object_data`, `git_odb_object_free`), and runs Rust's
   `Drop` through that crossing at end of iteration.
2. The stdin/stdout pipe overlaps three stages: writer feeds oid N+K
   into cat-file stdin, cat-file inflates blob N, reader parses
   blob N−1. Within-process git2 serializes walk, read, process
   inside one thread under the Mutex.
3. `--batch` prints all frames through a single pipe buffer, so
   inflated output blob bytes go straight from zlib into the pipe
   with no intermediate allocation per blob. git2's `OdbObject` wraps
   each inflated blob in a freshly allocated handle that owns
   inflated bytes.

### Implication for v3

The `GitBlobReader` behind a `ReadBytes` effect should default to the
shell-out shape for bulk walks. The structure maps naturally onto
`BoundedBatched`:

- writer thread: takes `ReadBytes { oids: Vec<Oid> }` batches off the
  bounded inbox, writes oids to the cat-file stdin pipe
- reader thread: parses frames, returns `(oid, bytes)` tuples in oid
  order, fires oneshot replies
- worker count: 1 for the pipe pair; parallelism goes upstream of the
  reader by fanning many `ReadBytes` puts into the inbox

Per-blob overhead is a single pipe write (ascii oid + newline) and a
single pipe read (header + body). Both amortize over the batch size.

git2 stays useful for:

- random-access point reads (one blob, one commit, one ref lookup)
- write paths (building trees, creating commits, updating refs) where
  cat-file has no counterpart
- introspection the shell-out path cannot reach (reflogs, submodule
  configs, merge bases) though `git rev-list`, `git merge-base` shell
  variants exist and would be measured in a future pass

### Cost of forking per walk

`git cat-file --batch` is spawned once per walk, lives for the full
walk, exits when stdin closes. `git ls-tree -r HEAD` is also spawned
once and exits after emitting the oid list. Fork + exec cost is ~2-5
ms on darwin, paid twice per walk. At 2-second walks that is <0.5%
overhead, negligible. A long-lived cat-file process (kept alive across
walks) would eliminate the exec cost entirely at the price of managing
a subprocess lifecycle inside the runtime.

---

## 7.5 Join with prior v3 design docs

The perf work here sits on top of four design docs already in
`v2/docs/` and `v3/docs/`. This section joins the two
bodies: rows where the perf measurements confirm, extend, or leave
untouched the design claims.

### Inner join (measurement confirms design)

Rows where a claim existed in the design docs and the perf work
produced a number that holds it up.

| Design claim | Source | Perf confirmation |
|---|---|---|
| Effect = request type + response type + dispatcher unifies batching / N+1 / cancellation / push-pull | `appendix/convergent-evolution-effect-dispatcher.md` | four batcher topologies compile against one `Batcher<E>` trait; same op call site works across all four; 8 tests green |
| Author-edit fanout for a new effect = 1 file | `appendix/v3-min-author-ops.md` | adding `ScanOne`, `ScanFile`, `ScanBatch`, `ExtractStrings`, `InsertHits` each took one file in `src/effects/` or `src/bin/`; `src/lib.rs` stayed untouched |
| Ops call `ctx.put(Effect)` only; batcher owns N+1 collapse | `v3-plugin-author-surface.md` row C4 | per-file put lands at ~8 µs overhead, batch put lands at parity; both measured on 63k linux kernel files |
| `ReadBytes` effect obeys content contract (A → B → C) | `v3-plugin-author-surface.md` row C2 | prefilter lever proves the contract-honoring path stays hot; `Pattern::fixed_string()` reads `cursor.content` (PATH B) before falling through to read |
| sqlite-backed `QueryStore` effect sits behind one dispatcher | `v3-plugin-author-surface.md` rows A7, D7 | `InsertHits` via `BoundedBatched` with single writer thread reaches ~5M rows/sec isolated throughput |
| Approval policy as enum field on one `MutationHandler` | `v3-plugin-author-surface.md` row D2 | `BoundedBatched` with `workers=1` shapes the serial-writer variant; the same batcher with `workers=N` shapes parallel dispatch; approval policy becomes one knob |
| Cancellation stays orthogonal to dispatcher | `convergent-evolution-effect-dispatcher.md` | current `Batcher::run` returns `BoxFuture`; adding a `CancellationToken` arg is additive on the framework side, no op-code change |

### Left join on the design side (design rows without perf numbers yet)

Design claims that the current perf work leaves untouched. Queued in
section 8 below.

| Design row | Meaning | Status |
|---|---|---|
| A3 sub-lang body extract | tolerant `set_included_ranges` with discontinuous holes | perf untouched |
| A7 emit schema | per-op row shape, sqlite DDL generation, scanner-hash | perf untouched beyond "insert arbitrary rows fast" |
| B2 completion inside args | partial-eval lane for op-owned token space | perf untouched |
| C6 capture stamping via `CaptureKind` trait | one-file extension of capture payloads | perf untouched |
| C8 scan-pointer stamping | command-side vs content-side sigil, `Tri` verified | perf untouched |
| D6 mutation cache (Skip / Stale / Emit) | hash-based effect cache | perf untouched |
| D7 store persist triggered by effect | effect-driven `QueryStore` writes | partially covered by sqlite bench (insert path only) |

### Right join on the perf side (measurements the design docs had omitted)

Numbers and rules the current work produced, which the design docs
stood silent on.

| Perf finding | Maps into |
|---|---|
| Fixed-string prefilter closes 6× of the parse-leg gap | belongs in `_1_ast-grep-extension.md` as a property of the lowered ast-grep `Pattern`; readable from `cursor.content` before CST walk |
| jemalloc pin drops RSS ~30% with wall flat | binary-level footer; add to each bench binary and to production bin targets; matches biome's methodology |
| Per-item plugin surface costs ~8 µs; batch-granularity costs ~0 | informs row C1 "pipe transform": ops with known-input sets should emit one batch effect, not N per-item effects |
| sqlite write leg at 5M rows/sec needs one writer thread + multi-value INSERT + one tx per batch + PRAGMA stack | belongs in row D7 "store persist" as the default wiring for the `QueryStore` batcher |
| Rayon work-stealing and crossbeam bounded-queue tie on homogeneous CPU work | informs row C4: the runtime default for scan/parse effects is rayon, the default for writes is crossbeam-bounded-batch, both live behind `Batcher<E>` and ops cannot tell the difference |
| Prefilter lives on effect metadata, not topology | new cross-cutting concern: `EffectKind::fixed_string() -> Option<&str>` or equivalent, honored by every batcher without special-casing |

### Reading order for v3-plugin-author-surface × perf findings

1. `v3/docs/convergent-evolution-effect-dispatcher.md` — the shape
2. `v3/docs/v3-plugin-author-surface.md` — the rows
3. `v3/docs/v3-min-author-ops.md` — the metric
4. This `FINDINGS.md` — the numbers that rows C1, C2, C4, D2, D7 will
   be measured against
5. `v3/docs/PRIOR_ART.md` — Rust-ecosystem prior-art survey and gap
   analysis for the effect-runtime crate

---

## 8. Followups queued for v3 proper

### Close to zero work

- Lift the prefilter from ad-hoc CLI flag to effect metadata.
  `ScanFile::fixed_string() -> Option<&str>` on the payload; all
  batchers honor it without knowing about patterns.
- Thread-local tree-sitter Parser reuse across ast-grep calls for
  every supported language. Currently only Cpp uses the reuse path
  because of a facade-type mismatch. Saves ~1.6% per measured.
- Dynamic topology swap via `ArcSwap<HashMap<TypeId, …>>` on the
  registry so LSP can flip `ScanFile` from serial to parallel when
  entering a batch mode.

### Design work

- Cancellation propagation through the plugin surface. Current
  `put().await` has no cancel hook; a `CancellationToken` parameter
  on `put` would let ops abandon outstanding effects on reparse.
- Policy-layer inside `BoundedBatched`: time-windowed vs count-capped
  drain, pareto-aware tuning per effect kind.
- Effect-to-store materialization. `InsertHits` currently takes a
  concrete Vec; a streaming variant that accepts a `Stream<Item=Hit>`
  would let the writer batch without the op materializing the full
  chunk in memory.
- `sh[]` effect contract (fingerprinted shell runs with approval +
  cache). Drafted in `v2/docs/_7_lsp-as-op.md` and
  `v2/docs/_8_string-redirection.md`; would use `BoundedBatched` with
  a serial worker or `Passthrough` depending on approval policy.

### Measurements queued

- Apply the four batchers to actual v3 op cohort once the grammar
  lands; re-run linux kernel + swc corpora with the real op set.
- Profile the 8 µs per-item tax under flamegraph to see whether the
  tokio mpsc can shed atomic ops on the hot path.
- git ODB read via `BoundedBatched` with a `Mutex<Repository>` worker,
  same shape as the sqlite writer. Previously measured at ~1M blobs/s
  single-writer on swc corpus; worth re-running under v3 wiring.
- Long-tail RSS behavior under 500 repos sequentially; confirm the
  16 GB budget target from `project_v2_memory_budget`.

---

## Appendix: files that implement each claim above

Framework crate (`v3/crates/effect_runtime/`):

- `src/lib.rs` — `EffectKind`, `Batcher`, `TypedEntry`, `RtCtx`,
  `RtCtxBuilder`
- `src/telemetry.rs` — `Span`, `Telemetry`, `EffectReport`, `summary`

Topology library (`v3/crates/effect_runtime/src/batchers/`):

- `passthrough.rs`
- `work_steal.rs`
- `bounded_work_steal.rs`
- `bounded_batched.rs`

Effect demos (`v3/experiments/effect_proof/src/effects/`):

- `read_bytes.rs` + `count_lines.rs` — original surface tests
  (ReadBytes → Vec<u8>, CountLines → usize)
- `scan_one.rs` — toy effect used by topology tests

Benches (`v3/experiments/effect_proof/src/bin/`):

- `ast_grep_v3_bench.rs` — ast-grep parity bench (per-file vs batch
  mode on linux kernel)
- `sqlite_v3_bench.rs` — sqlite insert baseline (extract + insert
  pipeline, with `--scan-only` and `--skip-insert` modes for
  breakdown)
- `git_tree_bench.rs` — git2 vs shell-out `git ls-tree | cat-file
  --batch` baseline, both registered as `Passthrough<WalkRepo, _>`
  batchers

Tests (`v3/experiments/effect_proof/tests/`):

- `surface.rs` — the four original surface tests
- `topology_choice.rs` — six tests including 2000-concurrent-
  submitter burst through `BoundedWorkSteal` with cap=16

Probe that established the baseline (outside this crate):

- `v2/examples/throughput_probe_v2.rs` — `--topology hopp-v2 |
  walk-parallel`, `--effect synthetic | git-odb-real | sqlite-real |
  rayon-ast-grep`, `--arrival dirac | pareto | burst | sine | ar1 |
  stepfn`, `--policy opportunistic | windowed:US`,
  `--breakdown`, `--no-prefilter`, `--max-bytes N`
