# RCA: the exe-swap write storm

Date: 2026-07-17. Status: fixed by receipt (4.7GB/boot to 111MB/boot).
Span: roughly 2026-07-14 to 2026-07-17, three sessions, six distinct defects.

## Symptom

Every `cargo install` of the `dl` binary followed by a daemon boot wrote
gigabytes to disk (4.4 to 6.1GB measured across receipts), pinned CPU for
60 to 90 seconds, and beachballed the machine badly enough that the user
force-killed the daemon repeatedly. The kills made the next boot worse.
`dl daemon status` said "not running" while the process was mid-storm, and
nothing on disk could say what the daemon had been doing after a SIGKILL.

## Impact

- Machine unusable during any post-install boot, for weeks, intermittently.
- Force-kill culture: the user killed the daemon on sight, which armed the
  next boot to storm again (see defect 4).
- Trust: the system could not answer "why is it slow", so every storm cost a
  human diagnosis session instead of one command.

## The causal chain, in firing order

An exe-swap boot walks this chain. Each numbered item is a separate defect
that had to fall before the next one was visible.

```
cargo install (new build stamp)
  -> every extract family reads as never-extracted        [by design, kept]
  -> full corpus re-extract, all roots
  -> re-extracted rows DIFFER from last run               [6: nondeterminism]
  -> content digests move, change flags fire honestly
  -> full derived cascade: flow rails, whole-table
     DELETE+INSERT per rel, doubled through the WAL       [gigabytes]
  -> user kills mid-pass
  -> ALL derived rels read incomplete next boot           [4: crash window]
  -> next boot full-rebuilds everything again             [self-perpetuating]
```

### Defect inventory

| # | Defect | Mechanism | Fix | Commit |
|---|--------|-----------|-----|--------|
| 1 | N+1 write loops | per-row INSERTs in refresh paths | plural `insert_rows`, whole-table digest skip | 4d0d24bf |
| 2 | Dishonest change flags | scip/catalog/type/dataflow refreshers returned `Ok(true)` unconditionally; 14 rels "changed" every tick | return ORed per-write `rows_changed` | 4d0d24bf |
| 3 | Settle counted its own bookkeeping | `stmt_ms`/`rel_count`/`query_log` moved every tick, so the root never settled and the poll loop re-enqueued full ticks forever | `RelKind::bookkeeping()` excluded from `is_settled` | 4d0d24bf |
| 4 | Crash window | `rebuild_derived` wiped every derived rel upfront, marked completion once at the end; a kill left everything incomplete, forcing the next full rebuild | per-component unmark/wipe/run/mark; deferred source-digest saves; I/O guard | 7f4d9c58, 6afd2cf3, 5cf4be15 |
| 5 | Zero-row call flip | `refresh_call_rels` hardcoded `Ok(true)`; an exe-swap re-derive of an unchanged corpus claimed change and cascaded the flow rails | `call_flip_moved` cell set only on a real delta | f48749e0 |
| 6 | **Nondeterministic extraction (root)** | see below | ORDER BY on every file-set query; cached facts emitted in input order | 80617b6b |

Defects 1 through 5 each cut real waste, and each fix's receipt exposed the
next layer. After all five, an exe-swap boot still wrote 4.4GB with
`+0 -0 source facts` on the tick line. That contradiction named defect 6.

## Root cause (defect 6) in detail

Three facts combined:

1. **File-set queries had no ORDER BY.** `extract_file_set` and seven sibling
   queries selected from `_file` in rowid order. Reconcile rewrites `_file`
   rows, so rowid order drifted between runs.
2. **The parse cache changed emission order.** `cached_facts_profiled`
   emitted cache hits in file order, then parsed misses appended after. A
   cold run and a warm run over the same file set emitted facts in different
   orders.
3. **Dedup was first-wins on a lossy key.** df_node ids are `file:line:col`
   with no repo component. The daemon serves multiple checkouts of the same
   repo, so the same path exists at different content. `seen_node.insert(id)`
   kept whichever repo's row arrived first; kind/var/fn_sym rode along from
   that arbitrary winner.

Result: two rebuilds of an identical corpus produced different rows for
df_node, df_lit, df_param, doc_comment, loop_over, and nest. This is the
part that made the storm unfixable from inside the change-detection
machinery: the digests were working correctly. The rows really were
different. Every "dishonest writer" fix sharpened the system's honesty, and
the honest answer remained "yes, the data moved", because the extractor was
rolling dice. The system was telling the truth about garbage.

The trigger signature in the logs, once `reason=` existed:

```
[tick] files 0/7744 parsed, +0 -0 source facts, derived rebuilt | 22193ms, trigger=full, reason=-
```

`reason=-` means need_full was false: a *scoped* rebuild, driven purely by
moved source digests, with zero reconcile-level fact changes. That line is
the storm's fingerprint.

### How it was found

The discovery was accidental and is worth recording: a kimi worker was asked
to prove row-count equivalence for a rule refactor and reported baseline
counts that differed run to run (flow_edge 235,427 vs 235,439 on the same
corpus). Its first draft called this "run-to-run jitter". The review
rejected jitter as an unproven claim and required either exact equality or a
named moving rel. The re-run with edits stashed showed five source rels'
digests moving across identical rebuilds. Equivalence work and storm
diagnosis converged on the same defect from opposite directions.

## Fix verification

Double-swap receipt, 2026-07-17, four served roots:

| boot | writes | cpu | derived phase |
|------|--------|-----|---------------|
| swap 1 (old arbitrary winners replaced by sorted winners, one-time) | 6.1GB | 72.9s | full cascades |
| swap 2 (identical source, fresh build stamp) | **110.9MB** | **8.5s** | 8.2 / 0.6 / 15.6 / 2.7 ms |

Steady state after settle: 0.0MB written per 60s window, rss 18MB.

## Contributing factors

- **The exe-stamp cache namespace is correct and kept.** A new binary may
  extract differently, so distrusting old extractions is right. The design
  only works when re-extraction is deterministic; that invariant was assumed,
  never stated, and never tested.
- **No write ledger.** Writes flow through two seams (`Db::insert_rows` and
  the derived `timed()` closure) that both hold rows-affected in hand and
  drop it. Attribution had to come from a 2-second OS sampler (why.jsonl)
  correlated against phase timestamps. With a per-tick (rel, rows) ledger,
  one query would have named every writer.
- **The daemon wrote no perf.jsonl** (a OnceLock seeded by the first-created
  engine, the config view, pinned the path), so `full_reason` was computed
  and then discarded for months of daemon ticks.
- **Kills destroyed the evidence.** Until why.jsonl (1105fe9d), a SIGKILL
  left nothing on disk; every storm diagnosis started from zero.

## What prevention now exists

- `dl daemon why` reads the on-disk trail after any exit, including SIGKILL:
  phase, detail, root, job, cumulative cpu and I/O at 2s resolution.
- Per-root perf.jsonl carries `full_reason` and `changed_rels` per tick;
  `reason=` rides the `[tick]` stderr line.
- A kill mid-rebuild now costs one component, its completion marker scoped
  by the unmark/wipe/run/mark bracket.
- `cached_facts_profiled` has a regression test pinning output order across
  cache states. File-set queries carry ORDER BY.

## Open follow-ups

- Extraction determinism is enforced by ordering, still unstated as an
  invariant. A rail (two-rebuild digest compare over the extract families)
  would catch a future nondeterministic extractor at commit time rather than
  at the next storm.
- The lossy df_node id (`file:line:col`, no repo) still exists; ordering
  made the winner stable rather than principled. Ref-spine work owns this.
- A genuine full rebuild (program edit) still writes GBs by design:
  whole-table DELETE+INSERT per derived rel in SQL. The derived-layer
  content skip is the remaining arc.
- Per-tick (rel, rows-written) ledger at the two write seams.
