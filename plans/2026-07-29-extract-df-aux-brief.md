# extract df-aux brief (codex luna): df_param/df_arg records + resolve precision

User waived the extractor-freeze for this lane (2026-07-29 morning, second
waiver after --resolve). Scope = v6/sprefa-extract ONLY. The receipts behind
every item: plans/2026-07-29-flow-interproc-scout.md (sections 2 and 5) and
the flagship-flow named stops (v6/dl/fixtures/flagship-flow.dl6 header).

## Deliverables, in commit-worthy order

1. **df aux wire records** (scout gap 1, P0). The DfF `Aux` slot is `()` and
   deferred (src/types.rs around :1326 — line numbers stale, re-find by
   symbol). Add two flat JSONL records to the df family:
   - `record=param family=df`: parameter node span + `pos` (typed-parameter
     indexing, receiver/self OMITTED from the count, matching v5's contract
     in the v5 tree at src/engine/decls.rs `df_param` docs — read it from
     ~/projects/sprefa root, it is the same repo).
   - `record=arg family=df`: call/new node span + argument node span +
     signed slot with receiver = -1, named-argument source slots, closure
     arguments included, matching v5 `df_arg` (src/engine/decls.rs).
   Emit from ALL FOUR language projectors (rust, ts, go, kotlin — the df
   node/edge halves are already parity-green per src/types.rs status rows;
   extend the same projectors). Flat top-level fields only, the resolved_edge
   precedent: no nested spans that a text-host projection cannot read.
2. **Kotlin arm in `resolve_call_edges`** (scout gap 2 extractor half):
   src/bin/extract.rs dispatches ts/rust/go/prolog only; add kotlin.
3. **Flat owner identity on `sig` records** (flagship-flow named stop): sig
   records carry callable identity only as a nested `owner` span object, so
   text-host projections cannot join signatures to resolved callees. ADD
   top-level flat fields (additive, do not remove or rename the nested form —
   golden parity tests pin existing shapes).
4. **Call-site identity on `resolved_edge`** (scout precision loss): the
   record has no call-site span, so same-name multi-site callers merge. Add
   additive top-level caller-site span fields.
5. **Schema + tests.** `extract --schema` text updated for every new record/
   field; each new record gets CLI golden coverage in the shape of
   tests/1_resolve_cli.rs (checked-in golden JSONL, exact stdout pin); df aux
   gets at least one cross-language fixture asserting receiver -1 and
   named-argument slots.

## Laws
- Rust changes confined to v6/sprefa-extract. Nothing else in the tree.
- Additive record shapes only; existing records keep their exact fields
  (golden_parity.rs is the referee — it must keep passing without loosening).
- No new dependencies.
- Line numbers in this brief and the scout doc are stale; re-find by symbol.
- If an item cannot be done additively or within scope, STOP that item and
  name it in the final summary.

## Validation (report exact counts)
- `cargo test --features cli` full suite green (0_prolog ledger is green at
  53 files as of c26b4e0e; if your df work adds .pl fixtures it may need the
  ledger refreshed — that is allowed, it is a pin not a freeze).
- `cargo build --release --features cli --bin extract` clean.
- New golden tests listed with their fixture paths.
- Budget: max 4 full `cargo test --features cli` runs; targeted `--test`
  runs otherwise.

## Final summary shape
Base-sha verification; per-deliverable outcome (or named stop); new record
schemas as they print in `extract --schema`; test counts before/after; the
exact first-5-lines JSONL of `--resolve` and a df-aux extraction over
src/**/*.rs as smoke receipts.
