# v6 rest-of-arc — epic golden plan (cut 2026-07-23, receipts verified same day)

> Direction vector (owner, 2026-07-23 PM): TS/rxjs engine is the prototype lab; stress
> it, land the extractor seam, prove json-rx (= JSON-able rxjs) by round-trip, serve
> over http with `--inline` as the one-subscription lifetime, parser late (DCG
> candidate), prolog as third surface, one-shot Rust gen as the exit experiment.
> Process law: **process lifetime = sum of held subscriptions.** No daemon concept.
> Every doc claim below carries a verified-at; distrust anything without one.

## The spine (read this table, skip the rest until dispatch)

| epic | one line | kill-number | depends on | user decision inside |
|---|---|---|---|---|
| E1 | stress gun on the TS engine | wake median ≈3/255; RSS slope ~0 under churn | — | which retract is production (see F1) |
| E2 | jsonl→SQLite→dirty ingest seam | stmts O(N/CHUNK); frontier = exactly the changed cone | — | jsonl line schema (with extractor worktree) |
| E3 | real-corpus stress | db bytes vs v5 4-6x; wake selectivity on real program | E1 E2 + extractor CLI | corpus list |
| E4 | stratified negation, v5 parity | v5 shapes green + non-stratifiable diagnostic | — (lower.ts only) | none (temporal VETOED, see F10) |
| E5 | json-rx extract + round-trip | lower(prog) ≡ instantiate(extract(prog)) digest-equal | E4 partial | — |
| E6 | serve: registry + http + ghcacher demo | kill route ⇒ poll cone cold (timer silent) | E4 + F10 resolution | MCP/LSP process ownership |
| E7 | parser (DCG candidate) | gh-cache.dl parses to the E4 golden AST | ast.ts contract | parser lib vs DCG |
| E8 | prolog surface (SLG over store) | tabled query ≡ datalog closure; cycles terminate | E7 | — |
| E9 | one-shot Rust gen | generated crate passes the ported goldens | E5 | go/no-go on results |

E1/E2 parallel now. E4 parallel to E1-E3 (disjoint files). E5→E6 sequential. E7 anytime
against the AST contract. E8/E9 gated.

---

## Recon facts (observed 2026-07-23, this session, against the working tree)

Observed (verified by read/grep/test-run):
- `v6/sprefa-store/js/`: 6,788 TS lines. `pnpm typecheck` clean; `pnpm test` 51/51.
  Versions: rxjs ^7.8.2, @libsql/client ^0.17.4, better-sqlite3 ^13.0.1 (labs only),
  tsgo `@typescript/native-preview` 7.0.0-dev, pnpm 11.10.0, Node 24 native TS.
- Lowering: `src/lower/lower.ts` — acyclic combineLatest pipes + recursive in-stratum
  naive fixpoint (landed today); materialized-member and aggregate-head strata defer
  via `RecursiveStratumDeferred`. `src/lower/rulegraph.ts` is dependency-free.
- Engine: `src/engine/engine.ts` — cascade (cx_*), reconcile (rx_*), reach, temporal.
  `with_txn` bracket landed today (BEGIN IMMEDIATE via single-statement execute;
  adapter's `executeMultiple` finally-ROLLBACK guard at sqlite3.js:161 forbids
  multi-statement inside a bracket). `propagate` pops via heap (Rust BTreeSet parity,
  engine.rs:1194).
- **Zero-count finding (doc rot, both languages):** DECISIONS.md pins "production
  retract = counting upsert + SCC *nested fixpoint*; DRed = oracle only." But Rust
  `retract_scc` (engine.rs:296) delegates to `retract_scc_two_pass` (:304) — a
  DRed-shaped over-delete+rederive — `nested` occurs 0 times in engine.rs, and the
  four `scc_scope/scc_frontier/scc_next/scc_live` TEMP tables are created
  (engine.ts create_schema; GraphNs) and referenced by **zero** other lines in TS.
  The pinned production retract is not implemented anywhere. E1 task 1.4 settles it
  by measurement; the doc gets amended to match the winner. RESOLVED same day:
  owner closed retraction (see DECISIONS.md); E1 task 1.4 is now an optional
  timing record, not a ruling.
- Known N+1 (unruled): `reach.multi_source_walk` inserts halt/start rows one
  statement per row and bypasses `stmt_counter` (direct `executeMultiple`).
- Extraction: type math committed on worktree branch `plan/extract-golden-plan`
  (sprefa-seed/_3_extract, 5 commits, cargo check exit 0). Owner reports a CLI with
  streaming jsonl output near done — **not verified in any tree I read** (guessed).
- Spine tables (js + rs `spine`): 9 tables; node identity UNIQUE
  `(family, file_id, byte_start, kind)` = `ux_node_identity`; Family = Df/Call/Type/
  Module; rel mint `rels.create_rel_table` (WITHOUT ROWID composite PK).
- Labs: `labs/fixpoint.ts` (while/expand/expandAsync/sql, golden-equal),
  `labs/prolog.ts` SLG/SLD (5/5 per chat_log/20260723.4, not re-run today).
- Measured (reactor-claims, 20260723.4): libsql heavy query 482ms with 0ms loop gap
  vs better-sqlite3 422ms compute + 633ms loop block.
- v5 targets to beat (delivery plan, verified numbers in doc): `_strings` 65.4MB
  (~92% coordinate mints), df family 163.6MB for one fact set; measured wake median
  3 of 255 rels.
- Doc freshness (git log -1): README/delivery/interfaces/daemon/demand = 07-20
  (pre-pivot; Rust-crate + daemon vocabulary stale); DECISIONS/AGENTS/ARCHITECTURE/
  MAP = 07-22/23 (current, minus the retract pin above).

Guessed (must be re-verified at dispatch):
- The extractor CLI's flag surface and jsonl framing (owner's worktree, unread).
- v5 wake-median reproducibility on the current corpus.
- Node MCP SDK fitness (`@modelcontextprotocol/sdk`) for the prototype MCP arm.

## Plan boundary / lowering boundary

- **Authoring surfaces:** hand-built TS AST (now); dl text via E7 parser; `tasks.tsp`
  TypeSpec authoring (idea, unplanned).
- **Canonical representation:** the typed AST (`src/lower/ast.ts`) + RelKind algebra
  (`tasks.d.ts`). THE contract every epic codes against.
- **Runtime IR:** the rxjs operator graph (live), and its serialized twin json-rx
  (E5). The operator set is pinned, so the graph is a pure function of AST+strata —
  extraction never introspects rx internals.
- **Target runtimes:** Node+rxjs (control plane) over SQLite (data plane) now;
  generated Rust (tokio watch/broadcast/mpsc + verbatim SQL) as E9's experiment.
- **Diagnostics ownership:** parser owns parse errors + source maps (E7); lowering
  owns kind/stratification errors (incl. non-stratifiable negation, E4); engine owns
  runtime receipts (stmt counts, RSS, tick reports).
- Semantics must always be expressible as (AST, strata, SQL, BufferPolicy). rxjs is
  delivery. Anything only expressible as rx runtime behavior is a design bug (E9's
  precondition).

---

## E1 · the stress gun (TS engine under fire)

**Goal:** falsifiable numbers for the two planes before any real corpus: wake
selectivity, RSS slope, tick latency, and the retract ruling.

**Contract** (`js/src/labs/stress.ts`, new; reuses `engine/measure.ts` sampler):
```ts
export interface GunConfig {
  rels: number; strataDepth: number; diamondWidth: number;
  rowsPerRel: number; churnTicks: number; churnRowsPerTick: number; seed: number;
}
export interface GunReport {
  peakRssMib: number; rssSlope: number;          // MiB per 100 churn ticks
  wakeMedian: number; wakeP95: number;           // rels recomputed per tick
  stmtsPerTick: number; msPerTickP50: number; msPerTickP95: number;
}
export function synthProgram(cfg: GunConfig): { prog: Program; sources: Sources };
export async function runGun(cfg: GunConfig, backend: "rx" | "sql"): Promise<GunReport>;
```

**Pseudocode:**
```ts
// synthProgram: seeded PRNG -> layered DAG of derived rels (chains + diamonds +
//   one recursive stratum per 32 rels), EDB leaves fed by ReplaySubject(1).
// runGun: lower (rx) or mint tables + SQL fixpoint (sql). subscribe sinks.
//   loop churnTicks: mutate churnRowsPerTick rows in random leaves ->
//     rx: source.next(newSet); sql: loader + mark_changed + propagate.
//   per tick: sample RSS (measure.ts), stmt_counter delta, recompute count
//   (propagate's return = the wake meter), hrtime.
// report: aggregate; digests of every sink compared across backends (cross-oracle).
```

**Instance timeline:** everything per-run; :memory: db; no persistence; process exits.

**Storage and identity:** rel name = identity in rx backend; `(rel,row)` dense keys in
sql backend; the cross-backend digest is the agreement key.

**Recursive tasks:**
- 1.1 `synthProgram` generator (seeded, deterministic; snapshot 3 shapes).
- 1.2 rx-backend runner over `lowerProgram` + injected subjects.
- 1.3 sql-backend runner over minted rel tables + the labs SQL-fixpoint pattern +
      reconcile `mark_changed`/`propagate`.
- 1.4 retract shoot-out: counting `retract` vs `retract_scc_two_pass` vs
      `retract_dred_cte` on cyclic synth graphs — correctness vs the golden
      survivors oracle, then latency/stmts. **Retraction is closed; record the
      three variants' timings for the archive only.**
- 1.5 wire `runGun` into a `pnpm stress` script; report printed as one table.

**Lowering path:** none new; consumes E0 surfaces.

**Done condition:** `pnpm stress` prints GunReports for both backends at 3 config
sizes; digests agree; the retract ruling is committed to DECISIONS.md.

**Epic golden test:** fixed seed 0xC0FFEE, {rels:255, strataDepth:8, diamondWidth:4,
churnTicks:100} → rx/sql sink digests byte-equal; wakeMedian asserted < 16 (coarse
gate; the printed number is the real deliverable); rssSlope asserted < 1 MiB/100
ticks on the sql backend.

## E2 · ingest seam (extractor jsonl → SQLite → dirty)

**Goal:** the socket the extractor worktree lands into: streamed fact lines become
spine rows and a seeded dirty frontier, batched, metered.

**Contract** (`js/src/engine/ingest.ts`, new):
```ts
export type FactLine =                       // CANDIDATE framing — final schema is
  | { t: "str";  id: number; s: string }     // pinned WITH the extractor worktree (F2)
  | { t: "file"; hash: string; size: number; lines: number }
  | { t: "node"; family: number; file: number; start: number; len: number;
      kind: number; name: number | null }
  | { t: "edge"; family: number; src: number; dst: number; kind: number }
  | { t: "rel";  name: string; row: readonly unknown[] };
export interface IngestReport { lines: number; stmts: number; changed: [number, number][]; }
export async function ingestJsonl(
  store: Store, rels: RelStore, lines: AsyncIterable<string>, rev: number,
): Promise<IngestReport>;
```

**Pseudocode:**
```ts
// parse each line (JSON.parse, discriminate on t); accumulate per-table arrays.
// flush at CHUNK boundaries via the existing batch inserts
//   (Store.nodes_insert_batch / edges_insert_batch / files_insert_batch /
//    flush_strings; rel lines via rels.create_rel_table once + batched INSERT).
// diff against ux_node_identity / edge PK (ON CONFLICT DO NOTHING + changes())
//   to compute the actually-new set -> changed cells.
// mark_changed(changed, rev)  // seeds reconcile; propagate is the caller's move.
```

**Instance timeline:** one ingest per extractor run per rev; the AsyncIterable is the
CLI's stdout via readline; report returned, nothing retained.

**Storage and identity:** spine tables own rows; node identity = the ux_node_identity
quadruple; rel rows = the composite PK from `create_rel_table`; `changed` speaks
`(rel,row)` dense keys. Ingest never updates in place: weights/new rows only.

**Recursive tasks:**
- 2.1 FactLine parse + per-table accumulators + CHUNK flush (N+1 metered).
- 2.2 new-row detection (`changes()` per batch; no per-row reads).
- 2.3 `mark_changed` seeding + an `ingest → propagate → derived re-emits` wire-up
      helper for tests and E3.
- 2.4 fixture jsonl (checked in, ~200 lines) + the golden below.
- 2.5 schema pinning session with the extractor worktree (F2) — adjust FactLine,
      re-run golden, THEN the worktree merges against a green socket.

**Lowering path:** none; this is the two-hand seam (Rust writes stream, TS loads).

**Done condition:** fixture ingests green; stmt count printed and O(lines/CHUNK);
re-ingesting the same fixture yields `changed = []` (idempotence).

**Epic golden test:** fixture jsonl → tables populated (counts asserted) →
`dirty()` returns exactly the cone of the changed rows → one derived rel recomputes
and its digest matches a from-scratch reference; second ingest of identical lines
produces zero changed cells and zero propagate recomputes (early cutoff observed).

## E3 · real-corpus stress (extractor + engine, end to end)

**Goal:** the first honest 500-repo-shaped numbers on real code.

**Contract** (`js/src/labs/corpus.ts`, new): `runCorpus(repos: string[], cfg): CorpusReport`
— spawns the extractor CLI per repo (execa, concurrency-capped = the machine law),
pipes stdout into `ingestJsonl`, then runs a demand set (subscriptions over N query
rels), churns by touching files + re-extracting changed ones.

**Pseudocode:** spawn → ingest → subscribe → churn loop → report {dbBytes per table,
extract ms, ingest ms, wake median, RSS}. Per-table bytes via dbstat (the v5 health
pattern).

**Instance timeline:** one run per invocation; db on disk (not :memory:) so bytes are
real; deleted at end unless `--keep`.

**Storage and identity:** as E2; corpus digest = XOR of sink digests, printed for
run-to-run comparability.

**Recursive tasks:** 3.1 CLI spawn + pipe plumbing (blocked on extractor landing);
3.2 dbstat bytes report; 3.3 v5-comparison table (`_strings` 65.4MB / df 163.6MB
targets); 3.4 churn-and-re-extract loop; 3.5 the sprefa repo itself as fixture
corpus #1.

**Done condition:** report on ≥3 real repos; the two thesis numbers (bytes ratio,
wake selectivity) printed beside their v5 baselines.

**Epic golden test:** sprefa corpus → deterministic ingest counts snapshot →
subscribe `reaches_from`-shaped query → touch one file → wake set ⊂ its cone
(asserted), full-corpus bytes ratio printed. Kill-number: if bytes ratio < 3x or
wake is unselective, the thesis needs surgery before E6.

## E4 · stratified negation, v5 parity (temporal sub-arcs VETOED 2026-07-23)

> **Owner veto, 2026-07-23 PM:** 4.2 (@next) and 4.3 (@async) are out — "not ready,
> plain and simple." The `@` sigil is dead in the v6 surface. Async will never be a
> sibling primitive of next: async/await = yield + next + Promise, one mechanism.
> The whole temporal design moved to Frontier F10. E4 = negation only, at v5 parity
> (v5 receipts: BodyItem::Neg src/ast.rs:370; `!rel(args)` with `_` wildcards,
> examples/anim-deck.dl:49, arch-conformance.dl:40; forcing-edge diagnostic
> src/typecheck.rs:1201; negated rel complete before readers, src/engine/derive.rs:347).

**Goal:** stratified negation matching v5's observable semantics.

**Contract** (additive to `ast.ts`; new arms in `lower.ts` + `src/lower/temporal.ts`):
```ts
export interface NegRelRef { readonly kind: "notrel"; readonly rel: string;
                             readonly args: readonly Arg[] }   // 4.1
export type BodyPred = RelRef | Compare | NegRelRef;
export interface RelTemporal { readonly next?: boolean;        // 4.2 state carry
                               readonly effect?: EffectDecl }  // 4.3
export interface EffectDecl { readonly mode: "switch" | "concat";
                              readonly clockSecs?: number }
// lowering additions:
//   negation -> anti-join in equiJoin/stratumFixpoint (stratified-only; polarity
//     edges in rulegraph; negative edge inside an SCC = typed diagnostic).
//   @next    -> StateRel = BehaviorSubject<Row[]>; emission gated by digest
//     cutoff via reconcile.verify (RelStore.verify wraps it).
//   @async   -> effects subscribe ONLY to the tick/propagate path (glitch law:
//     raw combineLatest diamonds double-fire); switchMap(from(effect)) for
//     cancel-stale, concatMap for long; clock(N) = shared interval salt.
```

**Pseudocode (4.2 core):**
```ts
// stateRelFor(decl): const subj = new BehaviorSubject<Row[]>(seed);
//   upstream$.pipe(map(rows -> [rows, digestOf(rows)]))
//     .subscribe(async ([rows, d]) => {
//        if (await relStore.verify(relId, 0, d, rev())) subj.next(rows); // moved
//     });  // unchanged digest -> no emission -> downstream stays quiet (304 shape)
```

**Instance timeline:** BehaviorSubjects live for engine lifetime; effect observables
cold per demand; the clock interval is one shared instance; unsubscribe tears arms
down (subscription law).

**Storage and identity:** @next digests live in rx_memo via verify (rel-keyed);
effects are not durable in this epic (durable effect rows = a later arc; the
isomorphism doc's spilled-locals idea stays parked).

**Recursive tasks:**
- 4.1 negation: 4.1.1 AST + constructor; 4.1.2 polarity edges + stratifiability
  diagnostic (names the cycle); 4.1.3 anti-join both paths; 4.1.4 goldens (evens
  via NOT, set-difference oracle, non-stratifiable snapshot).
- 4.2 @next: 4.2.1 RelTemporal on decls; 4.2.2 StateRel wiring + verify cutoff;
  4.2.3 golden (carry updates only on digest move).
- 4.3 @async/clock: 4.3.1 EffectDecl + arm; 4.3.2 tick-path-only wiring;
  4.3.3 marble-timed golden with injected mock effects.

**Lowering path:** diagnostics owned here (kind errors, stratification); temporal
syntax for the eventual parser is decision F4 — until then AST-only.

**Done condition:** the gh-cache slice (poll/resp/etag-carry/stars) runs on
hand-built AST + mock effects, all prior tests green.

**Epic golden test:** marble timeline: clock ticks t0,t1,t2; t1 responds 304
(unchanged digest) → change_log gains nothing at t1, gains one row at t2 (200);
diagnostics snapshot for `p <- not p` (non-stratifiable) and for an effect declared
off-tick.

## E5 · json-rx: extract + round-trip proof

**Goal:** json-rx = JSON-able rxjs, proven by round-trip, extracted from the graph
(pin 0723.2: extracted, never a lowering target).

**Contract** (`js/src/lower/jsonrx.ts`, new):
```ts
export interface JsonRxGraph {                       // versioned; v = 1
  v: 1; rels: JsonRelNode[]; edges: JsonDepEdge[];  // node = rel + kind + op chain
}
export interface JsonRelNode { name: string; kind: RelKind;
  op: "source" | "join" | "union" | "fixpoint" | "state" | "effect";
  rules?: SerializedRule[] }                         // rules re-use ast.ts shapes (JSON-safe already)
export function extractJsonRx(prog: Program): JsonRxGraph;
export function instantiateJsonRx(g: JsonRxGraph, sources: Sources): LoweredProgram;
```

**Pseudocode:** extract = buildRuleGraph + stratify + per-stratum op tagging (the
operator set is pinned, so AST+strata fully determine the live graph — no rx
introspection). instantiate = a lowerProgram twin that walks JsonRxGraph instead of
Program; both share the pure kernels (equiJoin/applySelection/projectAndAggregate/
stratumFixpoint), which move to `src/lower/kernel.ts` (rx-free, the json-rx spec's
pure math).

**Instance timeline:** extraction is pure/synchronous; instantiation cold like
lowerProgram; a JsonRxGraph is an artifact (checked-in snapshots).

**Storage and identity:** rel name = node id; the JSON is canonicalized (sorted
keys) so snapshots diff cleanly; graph digest = hash of canonical JSON.

**Recursive tasks:** 5.1 kernel extraction to `kernel.ts` (no behavior change, tests
green); 5.2 `extractJsonRx` + canonical serializer; 5.3 `instantiateJsonRx`;
5.4 round-trip goldens over every existing lower.test program; 5.5 the E4 temporal
arms serialize too (state/effect ops).

**Lowering path:** dl AST → json-rx → live rxjs; with E7 landed the left edge
becomes dl text. Diagnostics: version mismatch + unknown-op errors owned here.

**Done condition:** for every fixture program: sink digests of
`lowerProgram(prog)` === `instantiateJsonRx(extractJsonRx(prog))`; snapshots
committed.

**Epic golden test:** authoring AST → json-rx JSON snapshot → instantiate → identical
emission timeline (incl. one recursive and one @next program) → diagnostics snapshot
for a hand-corrupted graph (bad version, unknown rel ref).

## E6 · serve — subscription registry, http, the ghcacher demo

**Goal:** the golden demo's first face: a dl program serving its own interface,
process alive only because subscriptions exist.

**Contract** (`js/src/serve/`, new):
```ts
export class SubscriptionRegistry {
  subscribe(rel: string): () => void;    // refcount++; returns unsubscribe
  activeCone(): ReadonlySet<string>;     // union of subscribed cones (rulegraph)
  get size(): number;                    // 0 => a --inline process may exit
}
export interface RouteDecl { path: string; method: "GET" | "POST";
                             reqRel: string; respRel: string }
export function serveHttp(lowered: LoweredProgram, reg: SubscriptionRegistry,
                          routes: RouteDecl[], port: number): Server; // node:http now
```

**Pseudocode:** request → row into reqRel's Subject (id, path, body) → tick →
respRel emission with matching id → response; the live route holds a standing
subscription on respRel's cone (that demand keeps the poll clock hot). `--inline`
= subscribe, take(1), print, exit — same code path, registry hits 0, process ends.

**Instance timeline:** server = a subscription holder among others; registry refcount
governs cone activation and process exit; SIGINT = unsubscribe-all.

**Storage and identity:** subscription = (rel, refcount) resident; cones from
rulegraph; nothing persisted.

**Recursive tasks:** 6.1 registry + cone activation gating (cold rels stay cold —
assert no recompute off-cone); 6.2 http arm (node:http; axum is the production
front, F5); 6.3 the ghcacher program on E4 arms + this registry; 6.4 `--inline`
lifetime; 6.5 MCP tool arm via the node MCP SDK (prototype; production = rmcp, F5);
6.6 LSP arm parked behind F5.

**Lowering path:** RouteDecl is data (the interface-registry idea from the 07-19
interfaces plan, minus @in/@out syntax — a served interface is a host-fed RelKind
row; grammar = F4).

**Done condition:** ghcacher slice served locally: correct responses, 304 discipline
observable, and the kill-number below.

**Epic golden test:** start server → GET /stars answers from derived state → DELETE
the route (unsubscribe) → clock arm goes silent within one interval (asserted by
spying the effect fn) → re-add route → cone warms. Registry at 0 exits an
`--inline` run. Timeline snapshot of the whole sequence.

## E7 · parser (DCG candidate) — parallel, bound by ast.ts

**Goal:** dl text → typed AST with source maps; the first self-hosting move if the
DCG route wins.

**Contract:** `parseDl(text: string): { prog: Program; spans: SourceMap } | DlParseError[]`.
Grammar covers the v6 surface only (decls, facts, rules, negation, comparisons,
aggregates, temporal/host-rel annotations per F4) — v5's @in/@out grammar is
explicitly not a target.

**Recursive tasks:** 7.1 lib bake-off (F3: peggy vs lezer vs chevrotain vs DCG on
`labs/prolog.ts`'s engine) — a 1-day spike each on the same 30-line grammar sample,
receipts in this file; 7.2 grammar for the pinned surface; 7.3 source maps;
7.4 golden: gh-cache.dl subset parses to the exact hand-built AST used by E4's
goldens (deepStrictEqual); 7.5 diagnostics snapshots (malformed decl, unbound head
var, non-stratifiable program via E4's check).

**Instance timeline / storage:** parse per file-change; AST immutable per parse;
spans keyed by AST node id.

**Done condition:** round-trip with E4 fixtures, zero hand-editing.

**Epic golden test:** dl text → AST (=== hand-built) → source map spot-checks →
lowering runs → diagnostics snapshot set.

## E8 · prolog third surface (SLG over the store) — gated on E7 interest

**Goal:** goal-directed queries (backtracking, tabling) over the same relations.

**Contract:** `solve(goal: Atom, opts): Observable<Subst>` where clause resolution
reads EDB from SQLite (bounded SELECTs by bound-arg prefix) and tables per canonical
goal (the switchMapCache shape: cache keyed by goal, table = termination on cycles).

**Recursive tasks:** 8.1 lift labs/prolog.ts SLG onto SQLite-backed fact fetch;
8.2 goal-keyed table as the demand cache; 8.3 golden: tabled `ancestor` ≡ datalog
closure digest; cyclic KB terminates; 8.4 DCG on this engine = the E7 bake-off entry.

**Done condition / golden:** same-store cross-check: prolog answers === datalog
closure rows on 3 fixtures, cycles included; table hit-rate printed.

## E9 · one-shot Rust generation (the exit experiment)

**Goal:** generate a Rust crate from (json-rx graph + SQL strings + BufferPolicy)
and pass the ported goldens. Trinity → tokio: BehaviorSubject→`watch`,
Subject→`broadcast`, ReplaySubject(N)→bounded `mpsc`, pipes→`Stream`s; SQL verbatim
(already byte-identical Rust↔TS today).

**Recursive tasks:** 9.1 goldens ported to a language-neutral fixture format (rows
in/rows out JSON) — do this early, it is cheap and de-risks everything; 9.2 codegen
spike on one acyclic fixture; 9.3 recursive + @next fixtures; 9.4 the report:
what survived one-shot, what needed hands.

**Done condition:** the generated crate passes 9.1's fixture suite; the honest
gap list is the deliverable either way.

**Epic golden test:** json-rx fixture → generated crate → `cargo test` green on the
neutral fixtures → timeline parity on the @next marble case.

---

## Frontier (deferred decisions + evidence to resolve)

- **F1 · production retract.** Nested-fixpoint pin is phantom code (recon). Evidence:
  E1 task 1.4 shoot-out. Owner rules; DECISIONS.md amended same commit.
  RESOLVED 2026-07-23 — retraction closed by owner; DECISIONS.md amended.
- **F2 · jsonl fact schema.** Owned jointly with the extractor worktree; E2's
  FactLine is the candidate. Evidence: the worktree's actual emitter.
- **F3 · parser route.** peggy / lezer / chevrotain / DCG-on-SLG. Evidence: E7.1
  spike receipts (LOC, error quality, source-map cost, speed on gh-cache.dl).
- **F4 · v6 surface syntax** for temporal/host-rel/negation (replaces @in/@out/@next
  spellings). Human-reviewed before E7.2 freezes grammar; AST stays the contract
  meanwhile.
- **F5 · production front.** node:http prototype vs axum(+rmcp,+tower-lsp) front.
  PARTIALLY RESOLVED 2026-07-23 (owner): the SQLite data plane returns to Rust at
  the json-rx gen point (E9) — TS SQLite bindings are frozen (DECISIONS.md pin;
  the measured libsql native RSS creep is accepted lab noise, no binding work).
  Remaining open: the http/MCP/LSP process shape for the prototype phase only.
- **F6 · set-vs-element semantics.** Current lowering is set-per-emission; 0723.2
  leaned element for bounded rels. Evidence: E1 wake/latency numbers; flip only
  with a measured win (the kernels in E5.1 localize the blast radius).
- **F7 · SqliteObservable pushdown** (0723.2 left-field): pure-op pushdown into SQL
  with de-op at the host-RAM boundary. Spike only if E1/E3 show TS-heap joins as
  the bottleneck; would subsume the groupBy BOOKMARK.
- **F8 · multi_source_walk N+1** batching vs SQL-verbatim parity. Evidence: one
  grep of the Rust original + a micro-bench; then rule.
- **F9 · durable effects** (spilled-locals rows, crash-resume sagas from the
  isomorphism doc). RETIRED 2026-07-23 (owner): no saga emulation; FRP
  unidirectional only. Subject/BehaviorSubject are the imperative hatches.
