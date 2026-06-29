# node2vec reactive perf — keep the embed off the hot path + guard the anti-pattern

Status: PLAN. Feature landed (node2vec(edge) operator, `_node_embeddings`,
`node_sim`, `examples/node2vec-callgraph.dl`, 2 tests green: it 307/0/3, lib
163/0/1). This plan is the follow-on that makes it survive a `--watch`/`--daemon`
session where git checkouts re-tick reactive rules.

## Problem

`eval_node2vec_rule` (engine.rs:4758) recomputes the WHOLE embedding every tick —
unconditionally, synchronously, under the daemon's `eng.lock()`. node2vec is a
GLOBAL op (random walks touch the whole graph), so it is not cheaply
incrementalizable like a SQL rule. Both tick paths end with:

```
for r in &node2vec_rules { self.eval_node2vec_rule(r)?; }   // engine.rs:2122, :2448
```

So every `git checkout` → re-tick → full re-embed, stalling any query that wants
the engine lock. eval_node2vec_rule is, right now, the first instance of the very
anti-pattern W5 will lint for.

## Current world state (file map)

| thing | location | role |
|---|---|---|
| `eval_node2vec_rule` | engine.rs:4758 | recompute (the target) |
| call sites | engine.rs:2122 (full `tick`), 2448 (`tick_paths`) | per-tick run |
| `embed_graph(edges, cfg)` | embed/node2vec.rs | walks + skip-gram (the cost) |
| `knn_rows(pool, k)` | engine.rs:~33 | shared cosine top-k (text + node) |
| `_node_embeddings(node,graph,dim,vec)` | engine.rs DDL ~2526 | vector store |
| `load_rel_digest`/`save_rel_digest` | engine.rs:2647/2654 | `_reldigest` blake3 persistence, keyed by arbitrary rel string |
| `ConditionCache.digest: [u8;32]` | engine.rs:1214 | the scc/closure precedent: recondense only when edge rows' digest moved (:1269) |
| daemon lock | daemon.rs:147/161 | `eng.lock()` held across the whole tick |
| self-host lint rails | examples/lint-imports.dl, the no-touch guard | `--check` exit-2 = Claude blocking-hook code |
| doc_tag spine | typegraph.rs (Rust `#`-sections, JSDoc/KDoc `@`-tags) | parse surface a `@recompute` marker can ride |

The scc/closure operators ALREADY solve "expensive global graph op on a reactive
rule" via the per-edge content digest. W1 copies that mechanism verbatim.

## Workstreams (ordered)

W1 → W2 → W5 are independent and cheap; do them first. W3 is the structural lever
(needs the daemon threading). W4 is deferred until measured.

---

### W1 — digest-skip (the immediate no-op win)

Hash the edge rows before embedding; skip embed AND knn when unchanged. A
checkout changes file *content*, but the call/type graph only moves for files
that changed, so most checkouts leave the edge set identical → digest matches →
node2vec is a no-op.

```rust
// engine.rs, in eval_node2vec_rule, after reading `edges`:
//
//   let digest = blake3_edges(&edges);                 // order-independent XOR-fold
//   let key = format!("node2vec:{edge}");              // reuse _reldigest table
//   if self.load_rel_digest(&key)? == Some(digest)
//       && head_table_nonempty(head) {
//       return Ok(());                                  // GUARD: rows already correct
//   }
//   ... embed + knn + persist ...
//   self.save_rel_digest(&key, &digest)?;

fn blake3_edges(edges: &[(String, String)]) -> [u8; 32]
//   XOR-fold blake3(a "\0" b) over rows so the digest is order-independent
//   (the edge rel's row order is not stable across rebuilds). Mirrors
//   source_rule_digests' XOR-fold (engine.rs:2666).
```

Storage: one extra `_reldigest` row per node2vec graph (`node2vec:<edge>`). No new
table. Uniqueness: `_reldigest.rel` PK already.
Sequence: read edges → digest → compare → (skip | recompute+save).
Files: engine.rs only. ~25 lines.
Rationale: turns the common checkout into a no-op; same guard the codebase already
trusts for Tarjan. Also makes the engine pass W5's own rail.

---

### W2 — recent-digest vector cache (checkout-thrash free)

W1 keeps ONE current vector set per graph. Bouncing between branch A and branch B
re-embeds on every switch (each is "changed" vs the other). Keep the last N
(digest → vectors) so A↔B is a hit both ways.

```rust
// _node_embeddings gains the edge-digest in its key:
//   PRIMARY KEY (node, graph, edge_digest)
// On recompute: INSERT new (node,graph,digest) rows; prune graphs to the most
// recent N distinct digests (LRU by a _node_emb_seen(graph,digest,tick) side row).
// node_sim is filled from the digest that matches THIS tick's edges.
```

Lifetime: vectors persist in the `--db` (already a real table), so a warm daemon
restart reuses across process restarts too.
Storage: add `edge_digest` col + `_node_emb_seen(graph, digest, last_tick)` LRU
side table; default N = `SPREFA_N2V_CACHE` = 4.
Uniqueness: (node, graph, edge_digest).
Files: engine.rs DDL + eval_node2vec_rule. ~40 lines.
Rationale: checkout thrash between two branches becomes 2 embeds total, then
cache hits forever. Pure storage; no threading.

---

### W3 — switchMap background recompute (off the hot path, CANCELLABLE)

Even with W1, a graph-CHANGING checkout pays the full embed synchronously under
`eng.lock()`, stalling queries. Demote node2vec to a slow-refresh tier: on a graph
change, serve the last-good vectors, recompute on a worker, swap in when done.
**switchMap semantics (the hard requirement):** a newer graph change SUPERSEDES an
in-flight embed — cancel the stale worker, never let an old recompute overwrite
newer vectors.

```rust
// New state on Engine (or a sidecar the daemon owns; see lifetimes):
struct N2vWorker {
    gen: AtomicU64,                 // bumped on every graph change = the switchMap key
    handle: Mutex<Option<JoinHandle<()>>>,
    tx_result: Sender<N2vResult>,   // worker -> engine, drained at tick start
}
struct N2vJob { graph: String, edges: Vec<(String,String)>, digest: [u8;32], gen: u64 }
struct N2vResult { graph: String, digest: [u8;32], gen: u64, pool: Vec<(String,Vec<f32>)> }

// eval_node2vec_rule becomes NON-BLOCKING:
//   read edges; digest; if unchanged (W1) return.
//   g = gen.fetch_add(1)+1;                       // supersede any in-flight
//   spawn worker with N2vJob{ ..., gen:g };        // switchMap: latest wins
//   leave node_sim serving the PREVIOUS digest's rows (stale-but-valid)
//
// worker body (embed/node2vec.rs::embed_graph, made cancellable):
//   for each skip-gram epoch / walk batch:
//       if job.gen != shared_gen.load() { return; } // COOPERATIVE CANCEL (switchMap)
//   send N2vResult on completion.
//
// at the START of the next tick (under eng.lock, cheap):
//   drain tx_result; for each result where result.gen == gen.load():
//       persist pool to _node_embeddings; refill node_sim via knn_rows.
//   stale-gen results are DROPPED (the cancel that lost the race).
```

Cancellation is two-layer: (1) the worker checks `gen` between epochs and bails
early so a superseded embed stops burning CPU; (2) the result-drain re-checks
`gen` so a worker that finished just as a newer one started cannot commit. Both
are the switchMap guarantee — only the latest subscription's output is kept.

Lifetimes:
- `N2vWorker` lives on the daemon `DaemonState` (NOT inside `Engine`, which is
  behind a single `Mutex` — the worker must run WITHOUT the engine lock).
- One worker thread reused across jobs (or spawn-per-job with the gen guard;
  spawn-per-job is simpler, gen makes it correct). Default: spawn-per-job.
- The shared `gen: AtomicU64` is the only cross-thread state besides the channel.
Storage: none new (writes go through W1/W2 tables at drain time).
Sequence: tick detects change → bump gen → spawn → (serve stale) … worker runs
off-lock … next tick drains live-gen results → commit.
Files: daemon.rs (worker + drain hook), engine.rs (eval becomes enqueue; a
`drain_node2vec_results` called at tick start), embed/node2vec.rs (epoch-level
cancel check via a passed `&AtomicU64` + expected gen).
Gate: `SPREFA_N2V_ASYNC=1` opt-in first (one-shot `dl` runs stay synchronous so
`--query-json` still sees fresh rows; only the daemon goes async).
Rationale: the embed never blocks a query again; switchMap keeps a rapid sequence
of checkouts from stacking N embeds — only the final state is computed.

Risk: one-shot CLI (`dl prog.dl`) must NOT go async (it would exit before the
worker commits). Keep async behind the daemon + env gate; synchronous path stays
the default and the test path.

---

### W4 — warm-start incremental training (DEFERRED, measured-need only)

Reuse the previous vectors as the skip-gram init instead of random, re-walk only
from changed nodes' neighborhoods. Real ML-eng work; a known node2vec variant.
Do NOT build speculatively — only if W1–W3 prove insufficient under a measured
high-churn `--daemon` session. Left as a stub heading so the sequencing is on
record.

---

### W5 — static recompute-guard rail (catch it at coding time)

Runtime screams (W1's guard, the N+1 tick counter) fire after the fact. W5 is the
COMPILE/CODING-time scream: a sprefa dl `--check` program over the engine's own
source that fails when a recompute-shaped fn has no guard. Two halves: a marker
convention (so AI + humans declare intent) and the dl rail (so CI/LSP enforces).

**Marker convention** (what the AI is told to write):
> Any Rust fn that rebuilds a derived relation / embedding FROM SCRATCH (a
> recompute) must either (a) call a digest guard (`load_rel_digest` early-return
> skip), or (b) carry a `// @recompute unguarded: <reason>` line. A `for`-loop
> that runs such a fn per rule is part of the recompute and inherits the rule.

This goes in CLAUDE.md's "Style notes for this repo" next to the N+1 rule, and is
the line an agent reads before writing an `eval_*`/`refresh_*` that recomputes.

**The dl rail** (`examples/recompute-guard.dl`, riding ast/sg + the doc_tag spine):

```
# recompute-shaped fns: body calls a heavy global primitive
rel recompute_fn(file, line, name).
recompute_fn(f, l, n) <- scan("WORK","src/**/*.rs",f,rev),
    sg(f,rev, `fn $N($$$) -> $R { $$$ embed_graph($$$) $$$ }`, l), n = $N.
# (plus: a for-loop body calling eval_*_rule / refresh_*_rel from scratch)

# guarded: same fn body also early-returns on a digest miss
rel guarded_fn(file, name).
guarded_fn(f, n) <- recompute_fn(f, _, n),
    sg(f, rev, `fn $N($$$) { $$$ load_rel_digest($$$) $$$ }`, _), n = $N.

# opt-out marker (doc_tag @recompute unguarded)
rel waived_fn(file, name).
waived_fn(f, n) <- doc_tag(_, sym, "recompute", "unguarded", _),
    type_entity(sym, n, _, _, f, _).

# THE DIAG: recompute with neither a guard nor a waiver
diag("error", f, l, msg) <- recompute_fn(f, l, n),
    !guarded_fn(f, n), !waived_fn(f, n),
    msg = "unguarded recompute loop: add a digest skip or // @recompute unguarded: <reason>".
```

Enforcement loop: `dl examples/recompute-guard.dl --check` → exit 2 on any
unguarded recompute → wire into the pre-commit / Claude Code blocking hook (the
repo already uses exit-2 for `--check`). The LSP path (`--lsp`) surfaces the same
`diag` as a live squiggle while editing engine.rs.

Files: examples/recompute-guard.dl (new), CLAUDE.md (the convention line),
possibly a `@recompute` tag arm in `parse_rust_sections` if doc_tag doesn't
already pass arbitrary `@`-tags through (CHECK: it parses `@param/@returns/
@deprecated` for JSDoc/KDoc and `#`-sections for rustdoc — a Rust `// @recompute`
LINE comment may need a `match`/regex rule instead of the doc_tag spine; the
regex form is the safe default, no engine change).
Rationale: makes the anti-pattern un-mergeable, not just un-performant. Same
self-hosting move as lint-imports / no-touch. The first row it flags is today's
eval_node2vec_rule — W1 clears it, proving the rail.

Open question (W5): sg/ast can find `embed_graph(...)` in a body, but "an
unguarded `for` loop over a recompute fn" is harder to express structurally than
"a fn that calls embed_graph without load_rel_digest". Start with the fn-level
rule (high signal, low false-positive); the loop-level form is a follow-on only
if a real miss slips through.

## Sequencing summary

1. **W1** (engine.rs, ~25 lines) — digest-skip. Immediate; clears W5's first hit.
2. **W5** (examples/recompute-guard.dl + CLAUDE.md) — the rail, now green because
   W1 guarded the one recompute. Lands the convention while it's fresh.
3. **W2** (storage) — recent-digest cache for branch thrash.
4. **W3** (daemon threading, env-gated) — switchMap async tier. The structural
   fix; do after W1/W2 make the synchronous path already-cheap for the common case.
5. **W4** — deferred; warm-start incremental, only on measured high-churn need.

## Test plan

- W1: tick twice on an unchanged graph → second `eval_node2vec_rule` is a no-op
  (assert via a recompute counter, mirror `recondensed`).
- W2: A→B→A digest sequence → third state is a cache hit (counter unchanged).
- W3: under `SPREFA_N2V_ASYNC`, fire two graph changes back-to-back → exactly one
  result commits, its gen is the latest, the superseded worker's result is
  dropped (assert the cancelled gen never wrote).
- W5: `dl examples/recompute-guard.dl --check` exits 0 on the guarded tree; revert
  W1 in a fixture → exits 2 pointing at eval_node2vec_rule.
```
