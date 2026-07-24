# v6 dl MVP slice — ast-grep -> diags -> LSP, http-fronted (2026-07-24)

Scope ruling (owner): first light-up, not v5 parity in full. ast-grep facts pumped
into `diag`, served over LSP (v5 binary reused via compat view), everything driven
by curl. rx + sqlite + rust + shell. TS/node owns the loop. Parser via Langium,
grammar-first. New package `v6/dl/`. Don't touch json-rx. Plan-as-types twin:
`v6/dl/tasks.d.ts`.

## The golden demo, CLI-eye view (what "done" literally looks like)

The fixture program, `fixtures/sg-rail.dl` — a no-console rail over a fixture repo:

```
rel console_hit(path: text, start: int, end: int, text: text).

sh sg(pattern: text, path: text, start: int, end: int, text: text) =
  `sg run --pattern '{pattern}' --json $path`.

console_hit(path, start, end, text) <-
    file(path),
    sg?("console.log($$$ARGS)", path, start, end, text).

diag(path, line, col, end_line, end_col, severity, code, msg, hint) <-
    console_hit(path, start, _, text),
    span_line(path, start, line, col),
    "warn" = severity, "no-console" = code,
    "console.log left in source" = msg.
```

The session, verbatim (curl is the CLI):

```bash
# 1. boot
pnpm -C v6/dl serve &                        # listens on :7171, logs routes

# 2. load the program
curl -s -X POST localhost:7171/edb/program --data-binary @fixtures/sg-rail.dl
# -> {"loaded":true,"rels":["console_hit","diag",...],"minted":["__req_sg","__resp_sg"]}

# 3. tell it a file exists / changed
curl -s -X POST localhost:7171/edb/file_changed -d '{"path":"fixtures/corpus/bad.ts"}'
# -> {"tick":1,"changed":[["node",41],["edge",40],["file",1],...]}
#    (extract ran, spine landed; sg? fired once, __resp_sg rows committed tick 2)

# 4. read the diagnostics
curl -s localhost:7171/idb/diag
# -> {"rows":[{"path":"fixtures/corpus/bad.ts","line":3,"col":2,"severity":"warn",
#              "code":"no-console","msg":"console.log left in source",...}]}

# 5. watch live (second terminal)
curl -N localhost:7171/subscribe/diag
# data: {"tick":2,"rel":"diag","inserts":[{...no-console row...}],"retracts":[]}

# 6. fix the file, tell it again
sed -i '' '/console.log/d' fixtures/corpus/bad.ts
curl -s -X POST localhost:7171/edb/file_changed -d '{"path":"fixtures/corpus/bad.ts"}'
# -> {"tick":3,"changed":[["node",-2],["diag",-1],...]}      # WEIGHTS, not magic
# the -N terminal prints: data: {"tick":3,"rel":"diag","inserts":[],"retracts":[{...}]}

curl -s localhost:7171/idb/diag
# -> {"rows":[]}

# 7. one-shot query
curl -s -X POST localhost:7171/query -d '? console_hit(path, _, _, _).'
# -> {"rows":[]}

# 8. the editor, meanwhile: dl --lsp --diag-db ~/.local/state/dl/mvp.sqlite
#    published the squiggle at step 4 and cleared it at step 6.
```

Step 6 is the whole thesis in one line: the file fix retracts spine facts, the
diag row dies through the delta plane, and NOTHING diag-specific ran.

## Recon facts (observed, not guessed)

- `v6/sprefa-store/js` (`sprefa-store-engine` 0.0.0, pnpm@11.10.0, type:module):
  deps `@libsql/client ^0.17.4`, `better-sqlite3 ^13.0.1`, `rxjs ^7.8.2`. Tests via
  `node --test --experimental-transform-types`. Exports: engine/lib/spine/algo/
  measure/oracle/tasks (index.ts). NO lodash today.
- `src/lower/ast.ts`: the pinned parser target. `Program{decls, rules}`,
  `Arg = Var | Lit` (Wild is NegArg-only — positive `_` is an explicit deferral in
  the header), `CmpOp eq..ge`, `AggFn max|min|sum|count`, helpers `v/lit/relRef/
  notRel/headVar/headAgg`. NO host decls, NO arith/interp/call, NO JSON terms.
- `src/lower/lower.ts`: `lowerProgram(prog, sources) -> LoweredProgram`;
  combineLatest = join, map = equi-join/select/project/agg; recursive strata =
  in-stratum naive fixpoint (all-lazy, no aggs) else `RecursiveStratumDeferred`.
  `buildRuleGraph`/`stratify` in rulegraph.ts (NonStratifiableError exists).
- `src/engine/ingest.ts`: `ingestJsonl(store, rels, lines, rev) -> IngestReport`;
  stream-position refs; two-phase drain; batched pre-check SELECT per chunk (libsql
  `executeMultiple` returns undefined — no rowsAffected); reconcile addressing
  `rel*1e9+row`; seeds `rx_memo` batched. APPEND-ONLY today (header note 5 says
  revision path "not reachable in THIS append-only ingest model").
- Fixture: `tests/engine/fixtures/ingest.jsonl` (`{"t":"str","id":0,...}` lines).
- `extract` (worktree `.claude/worktrees/extract-golden-plan/v6/sprefa-extract`,
  bin `target/debug/extract`, src 2,895 lines): JSONL per line, `record`
  node|edge|sig|site|const, `family` cst|type|call|df, half-open byte spans,
  ts/tsx/js full, any ast-grep grammar = cst only, unknown ext = exit 0. Phase-1:
  NO name resolution. NOTE: extract's line shapes (`record=node...`) and
  ingest.ts's FactLine (`{"t":"str"...}`) are DIFFERENT schemas — the F2 pin is
  now a mapping task in this plan (M3.2), not a meeting.
- v5 LSP `src/lsp.rs`: runs the v5 engine IN-PROCESS (`eng.diags()`, line 125),
  publishes per file (`textDocument/publishDiagnostics` :399,:414,:1344),
  save-driven disk-truth, `diag_mute` support. It does NOT read a foreign db —
  the compat-view reuse therefore needs a small new source mode (M5.3).
- v5 `diag` schema (src/engine/decls.rs:249,:263): fixed 9 cols
  `path/line/col/end_line/end_col/severity/code/msg/hint`, named-args partial
  write, defaults severity=warn end_line=line, 0-based positions. `hover_note`
  6-col sibling. `diag_stage` 2-col routing.
- Langium 4.3.1 / langium-cli 4.3.0 (npm, checked today). Chevrotain 12.0.0.
- Fork rulings in force (DECISIONS.md "Surface rulings"): postfix `?`/`!` effect
  sigils = the timecut; rx names for time builtins; comma unordered; slash-liberal
  idents; diag = plain rel + FIRST RETRACTION INSTANCE golden; JSON5-shape types.
- Owner rulings this session: retention `rel(0)/rel(1)/rel`; tick shape (b)
  current+delta-log; `Key/Min/Max` column wrappers; datalog-first (host inputs
  inferred from executor template); extract = first builtin host; sh-vs-builtin
  parity golden; lodash blessed for object ergonomics; core graph algos readable
  (array methods, combinatorics-explaining tests); labs are build-up, not
  throwaway.

## Boundary

- **Authoring surface**: `.dl` text, MVP grammar subset (M1 list). JSON5 terms,
  time aggs, arith/interp: FRONTIER, not this slice.
- **Canonical form**: `ast.ts Program` + (new, dl-side) `HostDecl[]` — the bridge
  emits both; store's ast.ts is NOT forked, only additively extended where ruled
  (positive `_`).
- **Runtime IR**: `LoweredProgram` (rx graph) + SQLite tables (rel_* current,
  delta log, effect_cache) in one GraphNs.
- **Targets**: node process (http + rx + libsql). Rust: `extract` subprocess +
  patched v5 `dl --lsp --diag-db` front. Shell: sh host executors.
- **Diagnostics ownership**: grammar/load errors = Langium + bridge (returned on
  the /edb/program POST); program-derived diags = the `diag` rel; LSP publishes;
  exit semantics = a reader (M5.5).
- **Test law**: minimum tests, maximum coverage — one golden per epic exercising
  the whole path beats N unit tests; unit tests only where a golden can't reach
  (the diff combinatorics, SSE teardown). Lowerings stay rxjs-maximal and
  readable: named exported operators, the pipe IS the documentation.

## Epics

### M0 · package scaffold `v6/dl/`

**Goal**: a bootable pnpm package composing sprefa-store-engine, numeric-prefix
file layout (chris-js-style), test wiring identical to the store's.

**Contract**
```ts
// package.json deps: langium@4.3.1, rxjs@^7.8.2 (dedupe with store), lodash-es,
// sprefa-store-engine (file:../sprefa-store/js), @libsql/client (via store).
// devDeps: langium-cli@4.3.0, @typescript/native-preview, @types/lodash-es.
// scripts: test (node --test --experimental-transform-types), typecheck (tsgo),
//          grammar (langium generate), serve (node src/main.ts)
```

**Filesystem (thought out, pinned)**
```
v6/dl/
  package.json  tsconfig.json  tasks.d.ts        # plan-as-types (written with this plan)
  grammar/dl.langium                              # M1
  src/
    0_ast_bridge.ts   # langium document -> ast.ts Program + HostDecl[] + minted stage rels
    1_hosts.ts        # HostDef registry; sh executor; builtin extract + sg
    2_schema.ts       # decl -> CREATE TABLE rel_*, delta log, effect_cache DDL
    3_runtime.ts      # DlRuntime: attach, lower, tick loop, apply-deltas
    4_ingest.ts       # extract subprocess -> record->FactLine map -> ingestJsonl; re-ingest diff
    5_diag.ts         # diag builtin decl + v5 compat view maintenance
    6_http.ts         # POST /edb/:rel, GET /idb/:rel, GET /subscribe/:rel (SSE), POST /query
    main.ts
  tests/            # *.test.ts per src file, combinatorics-explaining names
  fixtures/         # *.dl programs, *.ts corpus files, *.jsonl extracts, golden/*.json
```

**Tasks**
- [x] 0.1 package.json + tsconfig (mirror store's), pnpm install resolves. DONE
  2026-07-24: dep is `link:` (not `file:`) — pnpm copies file: deps into
  node_modules where node refuses type-stripping; `@types/node` added for the
  same reason. sg bin bought as `@ast-grep/cli` devDep (0.39.9), allowBuilds in
  pnpm-workspace.yaml.
- [x] 0.2 empty src files with header contracts; `pnpm test` green (2/2 scaffold
  tests). DONE 2026-07-24.
- [x] 0.3 extract binary path: env `DL_EXTRACT_BIN`, default the worktree debug path
  (recorded in tasks.d.ts). DONE 2026-07-24.

**Done**: `pnpm -C v6/dl test` runs; imports from sprefa-store-engine typecheck.
**DONE 2026-07-24** (commits 84744e7a, 7e2992ab). Receipts banked for later epics:
sg over corpus/bad.ts reports byteOffset 147-167 line 3 col 2; extract emits a cst
call_expression node with span exactly {147,167}, 79 records total — the span_line
join lines up byte-for-byte.
**Golden**: none (scaffold); M1's golden is the first real gate.

### M1 · grammar (Langium) -> ast.ts bridge

**Goal**: `.dl` text -> `Program` + `HostDecl[]`, grammar-first, with the sigil
timecut split done in the bridge as rule minting (Lloyd-Topor-style).

**Grammar scope (MVP subset, in)**: `rel`/`rel(0)`/`rel(1)` decls; typed cols
`name: text|int` + `Key(text)`/`Min(int)`/`Max(int)` wrappers (parsed; Min/Max
lowering deferred — load error if used, "frontier" message); facts; rules
`head <- body.`; positional args, vars, lits, `_` wildcard (positive too), **trailing-`_`
elision** (fewer args than arity = the rest are wildcards — `console_hit(path)`
legal); `!rel(...)` negation; cmp `= != < <= > >=`; head aggs count/sum/min/max;
`sh name(cols) = \`template\`.` host decls (backtick body raw); postfix probes
`name?(args)` in bodies; `? rel(args).` query lines. **Out (frontier)**: JSON5
terms, `=~`, arith, interp, `latest()`, `interval!`, named args, slash idents
beyond `[a-z_][a-z0-9_/]*`, `name!(...)` mutation probes (parse, reject at load:
"mutations land with a later slice").

**Contract**
```ts
// 0_ast_bridge.ts
export interface HostDecl {
  readonly name: string;
  readonly columns: readonly { name: string; ty: "text" | "int" }[];
  readonly template: string;              // raw backtick body
  readonly inputCols: readonly string[];  // inferred: {name} / $name refs in template
}
export interface BridgeOk {
  kind: "ok";
  program: Program;            // ast.ts — probes REWRITTEN (see minting)
  hosts: readonly HostDecl[];
  retention: ReadonlyMap<string, 0 | 1 | "all">;
  queries: readonly RelRef[];
  minted: readonly string[];   // names of stage rels the bridge created
}
export interface BridgeErr { kind: "err"; diags: readonly LoadDiag[] }
export function bridge(dlText: string): BridgeOk | BridgeErr;
```

**Pseudocode (the probe minting — the timecut, per fork ruling)**
```ts
// A rule body containing `h?(a1..ak, o1..om)` (h a HostDecl; a* bind inputCols,
// o* bind outputs) splits into:
//   1. request rule:  __req_h(a1..ak) <- <the body atoms that bind a1..ak>.
//      __req_h is a minted DERIVED rel; its CURRENT row set IS the demand set.
//   2. response rel:  __resp_h(a1..ak, o1..om)  minted EDB — executor-fed (M4).
//   3. original rule: probe replaced by RelRef(__resp_h, [a.., o..]).
// Free-variable rule decides column sets (Lloyd-Topor law). Multiple probes in
// one body = sequential minting, one per probe, left to right.
```

**Instance timeline**: LangiumDocument built per bridge() call, discarded after
AST mapping (no incremental doc services in the MVP loop); Program is immutable;
re-POST of a program = full re-bridge + runtime swap (M3).

**Storage/identity**: none (pure). Minted names are deterministic
(`__req_<host>`, `__resp_<host>`) so re-bridge is stable.

**Tasks**
- [x] 1.1 `grammar/dl.langium` + langium-cli generation wired into `pnpm grammar`;
  generated code committed (power-tool codegen blessed). DONE 2026-07-24. Grammar
  law learned twice: a Langium keyword's exact-match token beats the ID/INT
  terminal REGARDLESS of grammar position — so type names (text/int), agg names,
  and retention digits are all parsed as plain ID/INT and validated in the
  bridge, never spelled as keywords.
- [x] 1.2 AST mapping: decls/facts/rules/neg/cmp/aggs -> ast.ts constructors.
  DONE 2026-07-24 (src/0_ast_bridge.ts, 608 lines, single export bridge()).
- [x] 1.3 positive `_`: additive change in `v6/sprefa-store/js/src/lower/ast.ts`
  (`Arg = Var | Lit | Wild`) + equi-join/projection plumbing in lower.ts
  ("don't project, don't consistency-check") + tests. The ONE store-side edit.
  DONE 2026-07-24 (branch dl/m1-store 49cd2101, merged v11): lower.ts needed
  comment-only changes (tryBind was already structural over {args}); elision
  falls out of the `col < args.length` loop bound; store 75/75, tsgo clean
  after a 3-line wild guard in labs/stress.ts (integration commit 9bc51bf2).
- [x] 1.4 host decls + input inference (`{name}`/`$name` scan) + probe minting.
  DONE 2026-07-24. Literal head/probe args ride minted `__lit_<n>` single-row
  rels (orchestrator pin; ast.ts HeadTerm has no literal form); diag head
  defaults applied as a bridge rewrite (end_line/end_col reuse line/col, hint
  via null-seeded __lit).
- [x] 1.5 load diagnostics: unknown rel, arity mismatch, Min/Max-used, mutation-probe
  -> LoadDiag rows (these are BRIDGE diags, not the diag rel). DONE 2026-07-24
  (+ parse errors with line/col, + stratification pre-check -> non-stratifiable).

**Done**: every fixture .dl in `fixtures/` bridges; store suite still green after 1.3.
**DONE 2026-07-24**: golden/bridge.sg-rail.json green (16/16 dl tests, 75/75 store,
tsgo clean on v11 after merge).
**Golden (M1)**: `fixtures/sg-rail.dl` text -> snapshot of `{program, hosts,
minted, retention}` JSON (golden/bridge.sg-rail.json). Includes one probe rule so
the minting is pinned byte-for-byte.

### M2 · schema + tick runtime

**Goal**: Program -> SQLite tables + rx graph + the tick loop; shape (b) delta
log; retention enforcement.

**Contract**
```ts
// 2_schema.ts
export function ddl(decls: readonly RelDecl[], retention: Retention): string[];
// rel_<name>(cols..., PRIMARY KEY(all cols))  -- set semantics
// delta(rel TEXT, row_digest INT, tick INT, weight INT)  -- shape (b), pinned
// effect_cache(digest TEXT PK, host TEXT, state TEXT, requested_tick INT)
// store_meta(key TEXT PK, value)  -- 'tick' counter row

// 3_runtime.ts
export class DlRuntime {
  static async boot(cfg: { dbPath: string; bridge: BridgeOk }): Promise<DlRuntime>;
  commit(batch: EdbBatch): Promise<TickReport>;   // THE single write site
  rows(rel: string): Promise<Row[]>;               // current table read
  deltas$: Observable<DeltaEvent>;                 // /subscribe + LSP feed
  dispose(): Promise<void>;
}
export interface TickReport { tick: number; changed: readonly [rel: string, delta: number][] }
```

**Pseudocode (commit = the tick; rxjs-maximal — the loop IS a pipeline you can
read, not a method with rx sprinkled in)**
```ts
// The runtime is one visible rx graph. Every stage is a named, exported
// operator so the lowering reads like the marble diagram it is:
//
//   commits$: Subject<EdbBatch>                  // the ONE .next() site
//
//   tick$ = commits$.pipe(
//     concatMap(applyEdbTxn),        // with_txn: tick++, EDB writes, delta rows
//     tap(injectSources),            // next() each changed EDB rel's Subject
//   )
//   derived$ = tick$.pipe(
//     map(collectDerivedSets),       // lowered graph settled synchronously
//     map(diffAgainstTables),        // lodash differenceWith — readable set diff
//     concatMap(applyDerivedTxn),    // with_txn: derived writes + delta rows
//   )
//   deltas$ = derived$.pipe(
//     tap(clearScratchRels),         // rel(0) dies with its tick
//     mergeMap((report) => from(report.events)),
//     share(),                       // /subscribe + LSP + HostRunner all read this
//   )
//
// concatMap = ticks are serialized (no interleaved commits) — the operator IS
// the lock. share() = one graph, many readers. No hidden state outside the
// store; every operator's input/output type is in tasks.d.ts.
```

**Instance timeline**: DlRuntime boots once per program POST; a re-POST builds a
new runtime against the same db (tables re-DDL'd IF NOT EXISTS; program swap =
dispose old subscriptions, resubscribe). Subscriptions (LSP, SSE) attach to
`deltas$` and survive across program swaps.

**Storage/identity**: row identity = full column tuple (set semantics);
`row_digest` = `oracle.mix` XOR of fields (same law as ingest.ts note 6). One
writer (commit); readers use plain SELECTs. Uniqueness: PK on all cols makes
re-derivation idempotent at the db layer.

**Tasks**
- 2.1 ddl() + boot(): attach Store, run DDL, build sources (Subject per EDB rel,
  seeded from current tables), lowerProgram, subscribe derived outputs.
- 2.2 commit() as above; sync-settle assertion (no async hop in MVP graph).
- 2.3 delta log append + `deltas$`; tick counter in store_meta.
- 2.4 rel(0) scratch clearing; rel(1)+Key upsert (only wired, not exercised, if
  no fixture uses it — say so in the test name).
- 2.5 derived-diff module with the combinatorics test: |old ∪ new| membership
  cases enumerated (in/in, in/out, out/in) — the readable-algo law applied.

**Done**: a pure-datalog fixture (no hosts) runs: POST rows -> tick -> derived
rows queryable -> retract on re-POST without the rows.
**DONE 2026-07-24** (dl/m2-runtime 306183a9, merged v11, tasks 2.1-2.5 all landed;
22/22, tsgo clean). Golden: tick1 [[grandparent,2],[parent,3]], tick2 [] (idempotent
noop), tick3 [[grandparent,-2],[parent,-1]] + delta-log dump with matching +-1
weight pairs. rel(0) clearing runs inside applyDerivedTxn's txn (deviation, doc'd);
sync-settle generation counter bumps BEFORE BehaviorSubject.next (bug found+fixed).
**Golden (M2)**: fixture program + 3 commits (add, noop, remove) -> snapshot of
`[TickReport, delta table dump]` — proves idempotent re-commit (zero deltas) and
weight-retract.

### M3 · ingest: extract -> spine -> EDB

**Goal**: `POST /edb/file_changed {path}` runs extract, maps records to
FactLines, ingests, and RE-INGEST DIFFS (the retraction path diag needs).

**Contract**
```ts
// 4_ingest.ts
export function extractFile(path: string): AsyncIterable<ExtractRecord>; // spawn
export function toFactLines(recs: ExtractRecord[]): FactLine[];          // F2 mapping, pure
export async function ingestFile(rt: DlRuntime, store: Store, path: string): Promise<TickReport>;
```

**Pseudocode**
```ts
// ingestFile:
//   recs = drain extractFile(path)          // extract exits 0 always for known ext
//   lines = toFactLines(recs)               // node/edge/sig/site/const -> spine kinds
//   newSet  = fact rows scoped to `path`
//   oldSet  = SELECT current spine rows WHERE file = path
//   batch   = { insert: difference(new, old), retract: difference(old, new) }
//   return rt.commit(batch)                 // ONE tick; retraction rides commit
// NOTE: this supersedes ingest.ts's append-only stance for the per-file case;
// ingestJsonl stays the bulk path, ingestFile owns the diff path.
```

**Instance timeline**: extract child per call, reaped on drain; no watcher —
curl is the trigger (by design).

**Storage/identity**: spine identity rules from ingest.ts (ux_node_identity,
file hash); per-file scoping via the file column; F2 mapping table lives in
`toFactLines` with one test per record shape.

**Tasks**
- 3.1 spawn wrapper + JSONL parse (async iterable; MVP corpus sizes, no
  backpressure work).
- 3.2 **F2 pin**: record->FactLine mapping (extract `record=node` etc -> spine
  `t:node` etc), including which extract families land in which spine rels; the
  ambiguities ingest.ts note 1 flagged get answered here in code.
- 3.3 per-file diff + commit; delete-file case (empty newSet).
- 3.4 wire `POST /edb/file_changed` (M6 provides routing; handler lives here).

**Done**: same file POSTed twice = second TickReport has zero deltas; edited
file = retract+insert deltas only for that file.
**Golden (M3)**: fixture corpus file v1 -> ingest -> spine row snapshot; edit to
v2 -> ingest -> delta snapshot (retracts old spans, inserts new). Language-
neutral: fixtures are JSONL in / JSON row dumps out.

### M4 · host rels: sh executor + builtin sg + extract

**Goal**: the `?` probe machinery: demand rows -> digest-cached effect ->
response rows land as a commit. `sg` ships as BOTH a builtin HostDef and a
user-space `sh` decl — the parity golden.

**Contract**
```ts
// 1_hosts.ts — pluggable trait shape (assoc-types instinct, TS spelling)
export interface HostDef<Req extends Row = Row, Resp extends Row = Row> {
  readonly name: string;
  readonly requestCols: readonly string[];   // = inputCols
  readonly responseCols: readonly string[];
  run(req: Req): AsyncIterable<Resp>;        // det|multi both fit
}
export function shHost(decl: HostDecl): HostDef;      // template -> spawn
export const builtinSg: HostDef;    // ast-grep: sg run --pattern <p> --json <path>
export const builtinExtract: HostDef; // extract <path> --family <fams>
export class HostRunner {
  constructor(rt: DlRuntime, store: Store, hosts: readonly HostDef[]);
  // subscribes rt.deltas$ for __req_* rels; on new request row:
  //   digest = mix(host, ...reqRow); if effect_cache hit -> skip (the ? law)
  //   else insert cache row 'pending', run(), collect rows,
  //   rt.commit({ insert: __resp_* rows }), cache 'done'
}
```

**Instance timeline**: HostRunner lives with the runtime; one in-flight run per
digest (cache row is the lock); errors land as cache state 'error' + a
`__resp_*` row with error columns (QueryState shape per fork ruling) — stream
never dies.

**Storage/identity**: effect_cache digest = mix over (host, request tuple) —
identical law to v5 pending_effect; response rows are ordinary EDB rows
(replayable, durable = the effect cache-as-table).

**Tasks**
- 4.1 shHost template fill ({col} raw / $col env) + spawn + line-split rows
  (MVP: JSON-lines-or-whitespace contract documented in the decl).
- 4.2 HostRunner demand subscription + digest cache + commit-on-response.
- 4.3 builtinSg (`sg run --pattern ... --json`) mapping matches to rows
  (path, start, end, text).
- 4.4 builtinExtract exposure as a host (so a program can demand extraction of a
  path — same machinery as file_changed but demand-driven).
- 4.5 parity golden harness: same pattern via builtinSg vs `sh` sg decl.

**Done**: a fixture program with one `sg?` probe fires exactly once per distinct
request row (cache proves it), responses join downstream.
**Golden (M4)**: fixture .dl with `sg?(pattern, path, ...)` -> timeline snapshot:
[req row appears, cache pending, resp rows commit, derived rel updates] +
SECOND identical request = no new cache row. Parity: builtin vs sh rows
byte-equal.

### M5 · diag + v5 LSP front

**Goal**: `diag` as the builtin 9-col rel (v5 schema verbatim); v5 `dl --lsp`
grows a `--diag-db <path>` source mode reading a compat view; retraction golden.

**Contract**
```ts
// 5_diag.ts
export const diagDecl: RelDecl;  // path/line/col/end_line/end_col/severity/code/msg/hint
// compat view (created by ddl()):
// CREATE VIEW diag_v5 AS SELECT path, line, col, end_line, end_col,
//   COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag;
```
```rust
// src/lsp.rs (v5 tree, additive): --diag-db <sqlite>
// poll PRAGMA data_version; on change: SELECT * FROM diag_v5;
// group by path; publishDiagnostics per file (reuse existing publish fns
// :399/:414); eng.diags() path untouched. The view IS the interface —
// "LSP becomes its own interfacing", the drag stops at this column list.
```

**Instance timeline**: LSP process independent of the node loop; connects to the
same sqlite file read-only; data_version poll at 500ms (save-driven cadence,
matching v5's disk-truth stance).

**Tasks**
- 5.1 diagDecl builtin + partial-head defaults (severity warn, end_line=line) in
  the bridge (named-cols deferral: MVP rules write all 9 positionally or use a
  minted default rule — pick in-code, document).
- 5.2 diag_v5 view in ddl().
- [x] 5.3 v5 lsp.rs `--diag-db` mode (Rust, additive, no engine boot in that mode).
  DONE 2026-07-24 (branch dl/m5-lsp f4fdddbe, merged v11): --diag-db threads
  cli/mod.rs -> lib.rs -> lsp.rs run_lsp; branch returns before any engine boot;
  500ms poll on a PERSISTENT read-only rusqlite connection — `PRAGMA data_version`
  is per-connection (a fresh connection's first read never moves; verified
  empirically), so reopen-per-poll would never see a change; retraction = empty
  publish for paths that vanished; scripts/lsp_capture.mjs harness (no deps).
  cargo check clean on the touched files; manually verified publish/clear/update.
- 5.4 retraction wiring test: file fix -> ingestFile diff -> spine retract ->
  fixpoint drops diag row -> delta -> view -> LSP clears. ZERO diag-specific
  retraction code (the fork's "first retraction instance" claim, proven).
- 5.5 `--check` reader: `GET /idb/diag` + a 10-line script exits 2 on any
  severity=error row (the open one-liner, closed here as http-flavored).

**Done**: editor shows a squiggle from a .dl rail over a fixture repo; fixing
the file clears it without restart.
**Golden (M5)**: THE SLICE GOLDEN — curl transcript snapshot:
```
POST /edb/program        (sg-rail.dl)
POST /edb/file_changed   (fixtures/corpus/bad.ts)   -> tick N, diag +1
GET  /idb/diag                                       -> [the row]
(fix bad.ts on disk)
POST /edb/file_changed                               -> tick N+k, diag -1
GET  /idb/diag                                       -> []
```
plus the LSP publish/clear message pair captured from a --diag-db session, plus
the delta-table dump proving the diag row died through weights.

### M6 · http front

**Goal**: curl is the CLI. node:http, no framework unless routing pain proves
otherwise, no auth, localhost.

**Contract** (full words, no short names)
```
POST /edb/program          text/plain .dl body -> bridge -> runtime (re)boot
                           200 {loaded} | 400 {diags: LoadDiag[]}
POST /edb/file_changed     {path} -> ingestFile -> TickReport
POST /edb/:rel             {rows: Row[]} -> commit insert batch -> TickReport
GET  /idb/:rel             -> {rows} (current table)
GET  /subscribe/:rel       -> SSE stream of DeltaEvent (curl -N)
POST /query                `? rel(args).` text -> one-shot SELECT -> {rows}
```

**Pseudocode**: thin router; SSE = `deltas$.pipe(filter(byRel))` per connection,
write `data: {...}\n\n`, teardown on socket close (refCount honesty — a dropped
curl must unsubscribe).

**Tasks**
- 6.1 router + body plumbing + error surfaces (400 with diags).
- 6.2 SSE with teardown test (subscription count returns to baseline).
- 6.3 /query: parse via grammar's query rule, bind lits, SELECT with WHERE.
- 6.4 `main.ts` boot: db path + port via env; startup log lists routes.

**Done**: the M5 golden transcript runs against a live server started by
`pnpm -C v6/dl serve`.
**Golden (M6)**: `tests/golden/curl-session.sh` executes the whole M5 transcript
against a real server; stdout snapshot-compared. This is the epic-of-epics gate.

## Frontier (deferred, with the evidence that will resolve each)

- JSON5 terms + island lexing (Langium multi-mode lexing feasibility spike).
- `latest()`/time aggs + minting; needs the delta-log read path exercised (M2 data).
- `interval!`/timers (no fixture needs time in this slice).
- rel(1)+Key upsert semantics under weights (needs first stateful fixture).
- `name!(...)` mutation probes (fire-once law; needs a mutating fixture).
- named args + partial heads (diag ergonomics pressure will force it — watch M5.1).
- scalar kernel (`=~`, arith, interp) — first rail that needs a regex filter.
- Min/Max lattice columns + in-fixpoint pruning (first depth recursion).
- extract multi-file/multi-rev coordinates + repo/rev spine walk (E3 corpus).
- Langium-generated LSP for editing .dl files themselves (free-ish later win).
- /q beyond single atoms; program swap semantics beyond full reboot.
- lodash: only where object ergonomics demand (mapValues etc); array stdlib first.
