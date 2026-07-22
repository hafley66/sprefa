# reactor-lab — is salsa resident, and does it eat the RAM budget?

On the real `salsa` 0.28 crate. Answers two questions with running code, not assertion.

```
cargo run --example salsa_behaves            # how salsa behaves (the event log)
cargo run --release --example salsa_ram -- rows   1000 10000
cargo run --release --example salsa_ram -- digest 1000 10000
./examples/ram.sh                            # the full RSS sweep, one process each
```

## Is salsa resident? YES. Always. It never touches disk.

Salsa is an in-memory memo table. Both strategies below hold their memos in RAM. The
question that actually matters is **what** it keeps resident — and that is decided by
what a tracked query RETURNS, not by the framework.

`examples/salsa_ram.rs`: same inputs, same computation, same 10M derived facts. The
only change is the return type of the tracked query.

| total facts | `rows` returns `Vec<u64>` | `digest` returns `u64` |
|---|---|---|
| 1M  | 9.6 MB (memo +8.2) | 1.7 MB (memo +0.4) |
| 5M  | 48.6 MB (+47.3)    | 1.7 MB (+0.4) |
| 10M | **79.9 MB (+78.5)** | **1.7 MB (+0.4)** |

- **`rows`**: salsa caches every fact → RSS scales **linearly with facts**. This is
  salsa eating the budget, the same wall as putting the cascade in RAM. At 10M `u64`
  it is 78 MB; with real string edges it is the GB wall.
- **`digest`**: the query folds the same facts into one `u64` (the rows are dropped —
  in the real system they are written to the sqlite cascade). Salsa's memo holds 1000
  `u64`s ≈ 8 KB → RSS **flat at 0.4 MB**, 1M or 10M, identical.

Same framework, two memory profiles that differ by ~200× and diverge further as facts
grow. **The reactor uses the `digest` row: salsa holds digests + the dep graph (O(rels),
KB); the cascade holds the facts (O(facts), on disk).** Salsa is always resident; the
design choice is whether that residency is O(facts) or O(rels).

## How salsa behaves — the event log IS the perf instrument

`examples/salsa_behaves.rs`: three source files → `edge_count(file)` → `total_edges`.
Salsa emits `WillExecute` (the body ran) and `DidValidateMemoizedValue` (reused from
memo). Counting `EXECUTE` events per edit is exactly how you measure salsa's effect on
performance — it is the amount of real recomputation a change forces (rust-analyzer
profiles this way).

| step | EXECUTE | VALIDATE | what it proves |
|---|---|---|---|
| cold build | 4 | 0 | everything runs once |
| re-query, no change | **0** | 0 | `verified_at == R` fast path — literally zero events |
| edit b (1→3 edges) | 2 | 2 | only b's `edge_count` + `total` re-run; a, c validated free |
| edit a, **same** `edge_count` | 1 | 3 | a re-ran, but `total_edges` **VALIDATED not executed** — early cutoff / backdating |

The two perf measures, both demonstrated here:
1. **work per change** = `EXECUTE` count from the event log (2 of 4 queries on a real
   edit; 1 of 4 when the derived value is unchanged).
2. **memory** = peak RSS via `getrusage` (the table above).

## Verdict

- Salsa is resident. Not conditional. In RAM.
- Whether it eats the budget is a design choice: memoize rows (O(facts), eats it) or
  memoize a digest with rows on disk (O(rels), does not). The reactor is the latter.
- The event log gives us the recompute-work metric; RSS gives us the memory metric.
  Both are cheap to measure and are the gates any reactor change must pass.
