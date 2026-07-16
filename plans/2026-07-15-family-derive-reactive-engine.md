# Family-derive reactive engine for extraction families

## Context

The bounded-reactive-storage checkpoint (`60a000b8`) landed owner-scoped call
delta reactivity for one family. The shape it produced is sound but does not
generalize: each extraction family that wants incremental reactivity must
re-implement, by hand and in SQL, the same memo machinery:

- a per-family `refresh_*_rels` full rebuild ([call.rs:122](../src/engine/extract/call.rs#L122))
- a per-family `refresh_*_rels_delta` ([call.rs:9](../src/engine/extract/call.rs#L9), [node.rs:84](../src/engine/extract/node.rs#L84))
- per-family preflight gates (six in call, plus the pending alias gate)
- per-family row projection, support recompute, and retraction
- one inlined call-only engine frozen into call's shape ([storage/call.rs:212](../src/storage/call.rs#L212), 1295 LOC)

The committed resume order treats this as seven independent blockers per
family. The structural reading is different: blockers 1, 3, 4 are the same
failure class (the memo's self-reported freshness is a lie), and blockers 2, 5
are the same class (propagation is not atomic). Both classes are free in a
reactive runtime that owns row-level reconciliation, and absent here because
that runtime does not exist.

A prior attempt exists in this repo's history. The v4 runtime
(`v3/crates/effect_runtime/src/v2/`, commits `be00eb04` through `3ab6c3c7`)
ported React/Redux/react-query vocabulary: `Component::render`, `Memoize<C>`,
`Query<F>`, `EffectDispatch` (saga), `EventBus` + `BusListener`, `Node::Yield`.
It got the scheduler right and still leaked, because the memo cached render
trees (`Node<Next>`) while the relation rows lived in a separate artifact the
family synced by hand. The memo and the row update were two systems bridged
manually. v4 also had a `DdStore` differential-dataflow `Store` impl
(`0b03be6a`), positioned as a storage backend beneath a React-shaped runtime,
inverting the layering: the substrate that natively owns dynamic dep capture
sat under a memo layer that re-implemented it statically and worse.

A throwaway spike ([`family_spike.rs`](../family_spike.rs), working tree,
untracked) validated the target shape on toy data. It hosts five families
(call_edge, call_support, type_edge, node_parent, doc_line) on one 67-line
engine. Each family is an 8 to 11 line `derive` fn. Deps are captured by
intercepting reads during derive (MobX/SolidJS model), not declared in an
array (React `useMemo` model). Measured behavior: retracting one owner's
input row rederives only the families that read it; re-deriving an unchanged
state is idempotent (+0/-0); an alias-bucket change propagates with no gate
and no preflight, because the dep capture recorded the read. The scaling
curve measured is `engine_constant + N_families * ~10`, against today's
`N_families * ~350` plus a per-family stale-closure hunt.

The alias bug from [delta reactivity and fact ownership](2026-07-14-delta-reactivity-and-fact-ownership.md)
is the canonical instance of the failure class this engine deletes. In
`useMemo` terms it is a missing entry in the dependency array. Dynamic
per-row dep capture cannot forget a dep it never had to list.

## Decisions

1. **Dep capture over dep declaration.** Families read inputs through a
   tracked `Ctx`; the engine records which input rows each family read. This
   is the SolidJS/MobX `computed` model, not the React `useMemo` array model.
   The alias gate, the six call preflights, and the marker-digest validity
   check all become the engine's universal dep-validity step. Rejected:
   declared dep arrays (v4 `with_domain`; static, undercaptures, the bug).
2. **Host families, do not rewrite extraction.** The resolution machinery in
   `refresh_call_rels:157-220` (by_name, sym_at, def_by_file, imports,
   aliases, SCIP occ) moves into `Ctx` methods unchanged. A family's `derive`
   body is the existing emit loop, extracted verbatim. This is a refactor of
   where the machinery lives, not new logic.
3. **Coarse family-level selectivity first, fine-grained later.** Step 0
   rederives the whole family on any affected input; selectivity is at the
   family granularity (unrelated families do not rederive). This matches the
   "option A" tier and is what `reproject_sqlite_call_affected_keys` already
   scopes for call. Per-output-row provenance (option B, selective rederive
   of individual output keys) is deferred until coarse proves slow on real
   corpora.
4. **The differential rail gates every port.** No family is hosted on the
   engine until a test asserts byte-identical public relation rows against
   the existing full refresh. The old path stays the source of truth and is
   not deleted until the rail is green. This is slice 1 of the earlier
   semantic-completeness set, generalized.
5. **Call first, node second, then the full-only families.** Call is the
   measured vertical with a known broken memo. Node is the second data
   point: it already has a delta path, so porting it with zero new
   soundness tests is the proof the engine generalizes beyond call. The
   full-only families (type, doc, text, dataflow) inherit delta for free.
6. **SQLite stays the carrier.** `Ctx::scan` is typed `SELECT` over the
   existing `_call_*` / interned-string tables; deps are recorded by stable
   row key (owner_id, site_id, sid), not vector index. One transaction spans
   rederive plus reconcile (this plan's dependency on resume step 3).
7. **Do not generalize the `Storage`/`CallStore` seam by editing it.** The
   engine is a new module above storage. The existing seam
   ([storage/call.rs:137](../src/storage/call.rs#L137)) is left intact; the
   engine calls through it. A separate arc covers widening the seam.
8. **The spike is reference, not the implementation.** `family_spike.rs`
   validated the shape and the LOC curve on toy `Vec<Vec<String>>` data. The
   real `Ctx` is SQLite-backed with real resolution. The spike is discarded
   once step 0 lands.

## Design

The unit of memoization is the derived relation (collection), not the render
tree. A family is a pure `derive` from input collections to output rows. The
engine owns cold-load ordering, dep capture, affected-set computation,
re-derivation, row diff, and transactional reconcile.

```rust
// src/engine/family/mod.rs
trait Family: Send + Sync {
    fn name(&self) -> &'static str;
    fn inputs(&self) -> &'static [&'static str];
    fn derive(&self, ctx: &mut Ctx, out: &mut RowSink);
}

struct Ctx<'a> {
    db: &'a Db,
    deps: HashSet<DepKey>,            // (rel, stable_row_key) read this derive
    resolve_state: ResolveState,      // by_name, sym_at, def_by_file, imports, aliases, occ
}

impl<'a> Ctx<'a> {
    fn scan(&mut self, rel: &str) -> RowIter;             // SELECT, dep per row by sid
    fn unique_def(&mut self, repo, rev, file, callee, line) -> Option<Sym>;
    fn emit(&mut self, out: &mut RowSink, row: &[Value]);
}
```

<!-- todo(decision): decide whether Ctx::unique_def takes the full resolve signature or splits into occ/name/alias hops mirroring refresh_call_rels:200 -->

`Family::derive` for CallEdge is the loop at [call.rs:222+](../src/engine/extract/call.rs#L222)
minus the index-building lifted into `Ctx`. The resolution closure at
[call.rs:200](../src/engine/extract/call.rs#L200) becomes `Ctx::unique_def`,
parameterized by the resolve state built once per cold-load.

The engine is `apply_sqlite_call_owner_delta` generalized: the preflight,
`collect_sqlite_call_affected_keys`, `reproject_sqlite_call_affected_keys`,
the support recompute, and the generation advance all become engine
internals parameterized by the family's input and output shape, not
call-specific code.

<!-- todo(feature): extract the affected-key + reproject machinery from storage/call.rs:405-555 into the generic engine -->

## Sequencing

Each step is reversible. The old path stays until the rail is green.

- **Step 0.** Add `src/engine/family/mod.rs` (trait, `Ctx`, `RowSink`),
  `src/engine/family/call_edge.rs` (one `Family` impl). No change to the tick
  path. `refresh_call_rels` untouched.
- **Step 1.** Differential rail: run `refresh_call_rels` and the CallEdge
  Family path against the same fixture; assert every public call relation
  row identical. When green, the abstraction is real for one projection
  family.
- **Step 2.** Host `call_support` (aggregation, GROUP BY shape) as a second
  Family. Rail asserts `support_count` identical. Proves the engine handles
  aggregation, not just projection.
- **Step 3.** Wire the Family path as an alternative entry in
  `refresh_call_rels` behind a flag. Old path remains the fallback. Both run
  in tests. This step depends on the caller-owned transaction (resume step
  3) landing first.
- **Step 4.** Port `node` to the engine. It already has a delta path, so it
  is the second data point. If node ports with zero new soundness tests, the
  engine generalizes.
- **Step 5.** Flip the flag. Delete `refresh_call_rels`'s body, the six call
  preflights, and the alias gate. Port type, doc, text, dataflow as `Family`
  impls; they gain delta for free.

<!-- todo(decision): sequence the alias gate (patch the current memo, slices 1-4) versus hosting call on the engine (delete the bug class). Hosting is more code before payoff but removes the gate entirely -->

## Verification

- **Rail (per family).** A test per hosted family asserting the engine's
  public relation output is byte-identical to the existing `refresh_*_rels`
  full rebuild on a shared fixture. This is the memo-soundness proof. Slice
  1 of the semantic-completeness set, generalized to every family.
- **Selectivity.** Retract one owner's input row; assert only families whose
  captured deps include that row rederive, and unrelated families emit
  +0/-0. The spike demonstrates this on toy data; the rail proves it on
  real SQLite.
- **Idempotency.** Re-derive an unchanged state; assert +0/-0 for every
  family.
- **Alias class.** Change an alias bucket; assert the affected families
  rederive with no gate and no preflight, output correct. This is the proof
  the bug class is deleted, not patched.
- **Scaling assertion.** After node ports (step 4), assert the engine source
  line count is unchanged from after step 0, and the per-family line count
  is under 30. Documents that the curve stays `engine + N * small`.
- **Generalization proof.** Step 4 (node) ports with zero new soundness
  tests. If node needs special-case code, the engine is not generic and this
  plan is not done.

<!-- todo(perf): measure engine-hosted call rederive wall time vs the 73 ms/1000-file baseline from reproducible-reactivity-evidence, once step 3 lands -->

<!-- todo(feature): port type_rels, doc, text, dataflow as Family impls in step 5 -->

## Staffing

- Base SHA: `60a000b8` (checkpoint: bounded reactive storage and call-owner delta).
- Implementer: agent, worktree optional (steps 0 to 2 are additive and isolated).
- Suite budget: existing `cargo check --lib`, the six call-storage tests, the
  rust call extraction test, the path-tick test, and the new per-family rail.
  Do not run the broad default tick interactively; the release probe stays
  the gated measurement per [reproducible reactivity evidence](2026-07-15-reproducible-reactivity-evidence.md).
- Dependency: step 3 requires the caller-owned transaction from the
  checkpoint's resume order (step 3) to land first.
