/**
 * tasks.d.ts — v6/dl plan-as-types. The MVP slice ledger in the type system.
 * Twin of v6/plans/2026-07-24-v6-dl-mvp-slice.md; this file is the checkable half.
 * Convention follows v6/sprefa-store/js/tasks.d.ts: types are the plan, comments
 * are the receipts. No runtime code.
 *
 * Slice goal: .dl text -> Langium bridge -> lowered rx graph + SQLite tick store
 * -> extract/sg host effects -> diag rel -> v5 LSP (--diag-db compat view),
 * all fronted by http (curl is the CLI).
 *
 * Laws (owner): full words everywhere (/query not /q, /subscribe not /sub);
 * trailing-`_` elision legal (fewer args than arity = rest are wildcards);
 * rxjs-maximal lowerings (named exported operators, the pipe is the doc);
 * min tests max coverage (one golden per epic; unit tests only where a golden
 * can't reach). This file + the plan doc ARE the jira board — update on land.
 *
 * M7 recomposition (2026-07-24, types-header consolidation): this file stopped
 * mixing roles. It keeps ONLY the plan ledger (EpicLedger) plus the pin/law
 * comment blocks the owner named as standing here (LiteralSeeds pin,
 * NamedArgLaw, DiagHeadDefaultLaw, SpineRelName, ExtractBinDefault). Every
 * other type this file used to declare now lives in src/0_types.ts (the
 * package's C-header-style type system, one file, organized by pipeline
 * section); this file re-exports each of them (`export type X = import(...)`)
 * so external importers of tasks.d.ts keep working unchanged. NamedArgLaw and
 * DiagHeadDefaultLaw are pure prose law-types (never imported as types by any
 * src/test file, only referenced in comments) and stay declared ONLY here.
 * SpineRelName and ExtractBinDefault ARE real cross-file types (src/4_ingest.ts
 * and src/1_hosts.ts import them) but by the same owner ruling stay declared
 * ONLY here too — those two src files import them straight from this file, a
 * scoped, documented exception to "every src file imports from 0_types.ts".
 */

// ─────────────────────────────────────────────────────────────────────────────
// Epic index (dependency order). Each `done` flips only with its listed evidence.
// ─────────────────────────────────────────────────────────────────────────────

export interface EpicLedger {
  /** DONE 2026-07-24: pnpm test 2/2, tsgo clean, sg 0.39.9 via @ast-grep/cli devDep
   *  (bought, not global), link: dep so node type-stripping reaches store sources.
   *  Commits 84744e7a + 7e2992ab on v11. */
  M0_scaffold: { done: true; evidence: "pnpm -C v6/dl test runs; store imports typecheck" };
  /** DONE 2026-07-24 (dl/m1-store 49cd2101 + dl/m1-bridge 744fca3e..5d75b464, merged
   *  v11): 16/16 dl tests + 75/75 store tests, tsgo clean both packages. Golden pins
   *  minted __lit_0(pattern)/__req_sg/__resp_sg/__lit_1..4 exactly per the plan's
   *  worked expectation. Two Langium keyword-shadowing bugs found+fixed (text/int/agg
   *  names as ID + bridge validation; retention as INT + regression test `b = 0`). */
  M1_grammar_bridge: { done: true; evidence: "golden/bridge.sg-rail.json snapshot green" };
  /** DONE 2026-07-24 (dl/m2-runtime 306183a9, merged v11; 22/22 dl tests, tsgo clean):
   *  golden ticks [[grandparent,2],[parent,3]] / [] / [[grandparent,-2],[parent,-1]].
   *  Named pipeline stages exported (applyEdbTxn/injectSources/diffAgainstTables/
   *  applyDerivedTxn/clearScratchRels + pure diffDerivedRel). Deviation recorded:
   *  rel(0) physical DELETE runs inside applyDerivedTxn's txn (tap can't await IO
   *  before commit's correlation resolves); the tap stage does rx-side bookkeeping. */
  M2_tick_runtime: { done: true; evidence: "add/noop/remove golden: zero-delta noop + weight retract" };
  /** DONE 2026-07-24 (dl/m3-ingest b7e26d59 + integration NULL-safe fix, merged v11;
   *  31/31, tsgo clean). Golden v1: const 1/edge 36/file 1/node 41/site 1/span_line 39;
   *  v2 edit: edge -10/node -10/site -1/span_line -8. Integration find: runtime
   *  pre-check + DELETE used `(cols) IN (VALUES...)` which never matches NULL columns;
   *  replaced with sqlAnyRowMatch (`col IS val` OR-groups) in 3_runtime.ts. */
  M3_ingest_diff: { done: true; evidence: "re-POST same file = zero deltas; edit = per-file retract+insert" };
  /** DONE 2026-07-24 (dl/m4-hosts 5ebb60c0, merged v11; 36/36 total, tsgo clean).
   *  Fire-once proven via effect_cache dump; parity byte-equal with the sh decl
   *  reshaping ast-grep's nested JSON via jq in the template (executor stays generic).
   *  M7 note: 1_hosts.ts duplicates the 53-bit digest fold (numbering law blocks
   *  importing 2_schema); consolidate into the 0-level types/util module at M7. */
  M4_hosts: { done: true; evidence: "sg? fires once per digest; builtin-vs-sh parity rows byte-equal" };
  /** DONE 2026-07-24, all five tasks. 5.3: dl/m5-lsp f4fdddbe (persistent-connection
   *  data_version law). 5.1/5.2/5.5: dl/m5-diag 86b9101c. 5.4 CLOSED at final
   *  integration: live session proof — delta rows diag/2/+1 then diag/3/-1, GET
   *  /idb/diag [] after fix, LSP capture pair pinned in
   *  fixtures/golden/lsp.publish-clear.jsonl. Zero diag-specific retraction code. */
  M5_diag_lsp: { done: true; evidence: "curl transcript golden + LSP publish/clear pair + delta dump" };
  /** DONE 2026-07-24 (dl/m6-http fcea017f+c298665b, merged; 52/52; curl-session.sh
   *  green 3x in-branch + 2x re-pinned post-kwargs). Findings recorded: line-deletion
   *  offset coincidence (edit must shift bytes), console_hit persists by cache design
   *  (only diag retracts), fixture sh sg shadowed builtinSg (builtins now listed last). */
  M6_http: { done: true; evidence: "tests/golden/curl-session.sh green against live server" };
  /** DONE 2026-07-24 (dl/m1-kwargs 94f1bf8b, merged): named args in bodies/heads/
   *  probes/queries via one shared resolveNamedArgs + Member grammar production
   *  (future JSON5 object-member twin); # comments; showcase fixture; elision law
   *  unified (unfilled slot = wild). Store untouched. */
  M1_kwargs: { done: true; evidence: "48/48 post-merge; golden re-pinned additively; store 75/75 zero-diff" };
  /** DONE 2026-07-24 (dl/m7-types 5a39592a, merged): src/0_types.ts C-header types
   *  file + src/0_digest.ts shared fold; this file recomposed to plan-ledger-only
   *  with re-exports; zero behavior change (52/52, curl golden PASS, fixtures
   *  byte-identical, store 75/75). */
  M7_types_header: { done: true; evidence: "0_types.ts consolidation; goldens byte-identical" };
  /** DONE 2026-07-24 PM (dl/m8-witness 3b68ca55 + dl/m8-super 366a513e..000b8e7f,
   *  merged): witness plumbing + supersession per IdentityWitnessLaw below. 63/63;
   *  curl transcript honest (console_hit/hit_count -> [] for real); churn stress
   *  fires==30==N*M, delta rows==216 exact, p50 2.08ms/commit, rss +10.22MB;
   *  stress gun within noise of the pinned baseline. Cache-row DELETE on
   *  supersession = orchestrator latest-wins interpretation, flagged in
   *  1_hosts.ts (A->B->A re-fires). */
  M8_identity_witness: { done: true; evidence: "console_hit retracts for real; fire-once per full digest; churn receipts exact" };

  /** DONE 2026-07-24 PM (codex/m9-storage-plane d238bc8d..3047d6cb, merged
   *  72fb7f48): the rel plane stops storing strings as keys. Every relbase_* is
   *  all-INTEGER, composite-PK, WITHOUT ROWID, minted through the store's
   *  create_rel_table on its strings/files spine; rel_* views restore the text
   *  surface so every reader above the storage line is byte-identical.
   *  delta.rel_id and both effect_cache digests are INTEGER; ingest paths are
   *  DL_ROOT-relative (owner ruled the process.cwd() capture acceptable).
   *
   *  Receipts on the 251-file rxjs corpus, before vs after on the same driver:
   *  db 40.9MB -> 9.2MB, index share 45.9% -> 1.24% against the <=15% gate,
   *  zero TEXT join columns (the remaining TEXT is the strings payload itself,
   *  the effect_cache host/state labels, and store-spine repos/store_meta).
   *  Review gate re-run at merge rather than taken from the agent's summary:
   *  65/65 dl, 75/75 store, typecheck clean, curl-session golden PASS twice
   *  outside the sandbox, schema proof by sqlite_master dump.
   *
   *  Carries the ONE-SQLITE-INTERFACE ruling (owner, same session): open_db()
   *  in the store's lib.ts is the single connection constructor, leaving zero
   *  @libsql/client imports in dl src+tests (scripts/dbstat_report.mjs is the
   *  one documented exception, a plain-node diagnostic with a sqlite3-CLI
   *  fallback). Horn (c) stands: dl fact rels ride the store's spine, they do
   *  not live inside its node/edge tables. Store needed zero changes. */
  M9_storage_plane: { done: true; evidence: "251-file corpus: 40.9MB -> 9.2MB, index 45.9% -> 1.24%, zero TEXT join columns" };

  /** DONE 2026-07-24 PM (c350e88e header, c18efcf3 I-prefix, 2ded157e dead
   *  alias removal, 45b3225a dataflow lift): both packages carry a C-header
   *  contract file. sprefa-store gains src/engine/types.ts (I-prefixed contract
   *  per class + an I*Statics for the side `implements` cannot see + a DATAFLOW
   *  section declaring the free functions that move data across them); v6/dl's
   *  src/0_types.ts gains IDlRuntime, IHostRunner, and the entry-point types
   *  Ddl/IngestFile/ShHost/StartServer, with the pre-existing Bridge and
   *  ToFactLines finally bound to their implementations.
   *
   *  Nothing in TypeScript binds a free function to a declared type, so each
   *  one carries an AssertTrue<...> alias in its implementing file. Every rail
   *  was broken on purpose before being trusted (TS2416 on an instance drift,
   *  TS2344 on a statics drift and on an API drift). It caught a real one on
   *  the first run: IngestFile had been declared `(path) => Promise<TickReport>`
   *  since M3 while the implementation took `(rt, filePath)`, wrong for the
   *  whole arc because no implementation was ever checked against it. */
  M10_contract_headers: { done: true; evidence: "both headers enforced by AssertTrue proofs; caught the stale IngestFile declaration" };
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared row/value shapes — declared in src/0_types.ts; re-exported here.
// ─────────────────────────────────────────────────────────────────────────────

export type Value = import("./src/0_types.ts").Value;
export type Row = import("./src/0_types.ts").Row;
export type ColumnType = import("./src/0_types.ts").ColumnType;

/** Retention forms (owner ruling 2026-07-23): the decl's one capacity knob. */
export type Retention = import("./src/0_types.ts").Retention;

// ─────────────────────────────────────────────────────────────────────────────
// M1 · bridge (grammar/dl.langium + src/0_ast_bridge.ts) — types live in
// src/0_types.ts; re-exported here so external importers keep working.
// ─────────────────────────────────────────────────────────────────────────────

export type HostDecl = import("./src/0_types.ts").HostDecl;
export type LoadDiag = import("./src/0_types.ts").LoadDiag;

/** LiteralSeeds pin (ORCHESTRATOR PIN 2026-07-24, kept standing here per owner
 *  ruling): BridgeOk.literalSeeds maps a minted `__lit_<n>` rel name to its one
 *  row's value (the literal-binding rewrite's seed data, e.g. `"warn" = severity`
 *  with severity otherwise unbound, or a Lit arg to a probe); DlRuntime.boot
 *  inserts these rows at boot. Numbering is first-appearance order, so
 *  re-bridge is stable. Full field doc now lives with the BridgeOk type itself
 *  in src/0_types.ts. */
export type BridgeOk = import("./src/0_types.ts").BridgeOk;
export type BridgeErr = import("./src/0_types.ts").BridgeErr;
export type BridgeResult = import("./src/0_types.ts").BridgeResult;

/** bridge() takes the builtin rel decls (spine + diag) as a second argument so
 *  `file(path)`/`span_line(...)`/`diag(...)` refs don't raise unknown-rel. A builtin
 *  headed by a program rule becomes IDB (derived); otherwise it stays EDB. */
export type Bridge = import("./src/0_types.ts").Bridge;

/** OWNER ESCALATION 2026-07-24 PM (M9, SEVERITY MAX, engine-core storage): the dl
 *  rel plane shipped strings-as-keys with full-tuple PKs on rowid tables (measured:
 *  69-byte absolute-path TEXT per rel_edge row, index bytes 48% of file at 41 nodes,
 *  untyped columns) — v5's amplification disease (57%-index, 39x db/corpus) rebuilt
 *  beside its cure. The storage plane IS the engine core. Ruled design:
 *  1. Ride the store's machinery (interning/surrogate ids; spine-orm law: a table
 *     holds the id, NEVER the text beside it — v6/plans/2026-07-21-spine-orm.md).
 *  2. Dictionary string_ids for every text column (path first); span dictionary
 *     spans(span_id, path_id, start, end) so edge/site/const/node store span-id
 *     pairs, never path+offset composites.
 *  3. WITHOUT ROWID wherever PK = full tuple; subset PKs where a subset key exists.
 *  4. Paths stored DL_ROOT-relative (absolute never stored); the http layer resolves
 *     back to absolute at the response boundary so the surface stays byte-identical;
 *     diag_v5 emits the relative path and lsp.rs's cwd-join makes it absolute (LSP
 *     runs with cwd = DL_ROOT).
 *  5. Declared affinity (INTEGER/TEXT) on every column.
 *  6. Surface text resolves through generated read views; /idb, /query, diag_v5
 *     and all goldens byte-identical above the views.
 *  7. Delta log keeps row_digest; rel name interned to rel_id (it is free with the
 *     dictionary in place).
 *  Gates: dbstat before/after on a scaled corpus (hundreds of .ts files), rel-plane
 *  index bytes <= 15% of file, bytes/row per table, churn + store stress gun re-run,
 *  commit p50/p95 re-measured (regression = receipts + escalate).
 *  AMENDMENTS 2 (owner, binding):
 *  a. SEMI-NAIVE FLOOR: any fixpoint/closure/propagation loop runs the semi-naive
 *     delta-frontier form (the store cascade IS this). The commit pipeline's
 *     recompute-whole-sets diff is the called-out naive form. A surviving naive
 *     loop carries a receipt: where, why bounded, when it graduates.
 *  b. ONE PROCESS SPACE per algorithm class, uniformly: SQLite set-ops (cascade)
 *     OR in-RAM tight loops; never a smear (half a traversal in SQL, half in JS
 *     pulling rows per hop). Commit pipeline + HostRunner audited; the report
 *     states which space each algorithm closed into and why.
 *  c. ZERO STRING FOREIGN KEYS, absolute: no TEXT column referencing another row
 *     (path, rel name, host name, digest-as-hex included; keying digests become
 *     INTEGER). dbstat gate addition: any join column with TEXT affinity = FAIL.
 *  AMENDMENT 3 (owner): THE STRESS TEST IS THE GOLDEN. M9 acceptance = a pinned
 *  stress-golden committed in the dl suite: a scaled-corpus session (hundreds of
 *  real files, repeated churn edits, effect re-fires, retractions) whose output
 *  invariants are asserted like any golden — exact re-fire counts, delta-log
 *  growth formula, dbstat ratio ceiling (rel-plane index <= 15%, zero TEXT-affinity
 *  join columns), commit p50/p95 budget, rss ceiling. Sized at the largest corpus
 *  under a tolerable suite wall-clock; the degradation knee recorded on the board.
 *  Repeatable script, not a one-off report.
 *  AMENDMENT (owner, same day, hard evidence): the runtime uses the store's rx
 *  lowering but ONE engine function total (with_txn, 3_runtime.ts:60); ingest.ts is
 *  imported for types only; the live db holds zero store tables. M3 silently dropped
 *  the plan's "ingestJsonl stays the bulk path" and built a parallel string-keyed
 *  ingest. M9 therefore WIRES THE ACTUAL STORE: Store/GraphNs attach; the spine
 *  schema (surrogate ids + interning) is the fact plane; ingestJsonl or its
 *  per-file-diff extension is the write path; dl's rel_* plane serves DERIVED and
 *  host/scratch rels only, riding the same dictionary ids. A store seam that cannot
 *  serve (per-file diff, retention hooks, line/col table) gets an ADDITIVE store
 *  extension, never a re-implementation beside it; every extension recorded in the
 *  final report, which also audits the M2/M3 divergence (prompt vs built vs review). */
export type EngineStorageLaw =
  "ids in tables, text at surface; WITHOUT ROWID full-tuple PKs; affinity declared; paths root-relative";

/** OWNER ESCALATION 2026-07-24 PM (M8, identity-vs-witness): the effect cache was
 *  memoizing the ADDRESS without the VALUE WITNESS (digest = (host, template inputs)
 *  only), so sg?-over-file-content kept stale response rows after a file fix
 *  (console_hit ghosts; only diag died, via the span_line join). The law as ruled:
 *  - IDENTITY = template-referenced request args (the supersession group).
 *  - WITNESS  = probe args BEYOND the template inputs: legal, positional-only,
 *    land between inputs and outputs, join the digest as SALT (v5 precedent:
 *    wildcard clock-bucket salting of pending_effect), NEVER reach the executor.
 *  - __req_h columns = inputCols + salt_<i>; __resp_h columns = inputCols +
 *    salt_<i> + outputCols (responses self-describe their witness).
 *  - effect_cache splits identity_digest vs full_digest (full is PK; index on
 *    identity). Fire-once holds per FULL digest.
 *  - SUPERSESSION: when a new full digest fires within an identity group, the
 *    prior group's __resp_h rows retract in the SAME commit as the new inserts,
 *    through the ordinary weights plane (gh-cache latest-wins, engine-side).
 *  - The fixture witness: file(path, content_hash) spine rel (sha-256 hex of
 *    bytes, computed at ingest); probe spelling
 *    sg?("...", path, content_hash, start, end, text).
 *  Perf baseline BEFORE M8 (2026-07-24): dl suite 669ms; curl-session ~1s wall;
 *  store stress large-golden rx peakRSS 285.97MB slope 26.07, sql 304.38MB slope
 *  10.05, digestsAgree=true; retract 0.60ms / scc 6.93ms / dred_cte 0.67ms. */
export type IdentityWitnessLaw =
  "digest = identity(template inputs) + witness(salt args); supersession retracts prior witness rows via weights";

/** OWNER SCOPE CHANGE 2026-07-24 (named args INTO M1, out of frontier). Law:
 *  `rel(col: term, ...)` legal in body atoms and heads; named args resolve to
 *  positional slots at load (v5 semantics, ~1727 uses). Mixing: positional first,
 *  named after, NO positional after a named; duplicate name, name+position slot
 *  collision, or unknown column name -> LoadDiag "named-arg". Partial heads: an
 *  atom's/head's unfilled slots are wild in bodies; a diag head's unfilled slots
 *  take the DiagHeadDefaultLaw below; any other head's unfilled slot is a binding
 *  error. Grammar constraint (forward-compat): the `ident: term` production is ONE
 *  shared rule named `Member`, written to be reused verbatim by the future JSON5
 *  object-literal member rule (DECISIONS "JSON5 shapes"). */
export type NamedArgLaw = "positional-first, named-after, resolve-to-slots-at-load, Member production shared";

/** ORCHESTRATOR PIN 2026-07-24 — diag head-default law (bridge rewrite, task 5.1
 *  "pick in-code" resolved): when a diag-head rule leaves a head var unbound, the
 *  bridge substitutes: end_line := line (the bound var), end_col := col (the bound
 *  var), severity := "warn", code := null, hint := null (literals via __lit minting).
 *  An unbound path/line/col/msg stays a load error (arity/binding). */
export type DiagHeadDefaultLaw = "end_line=line end_col=col severity='warn' code=null hint=null";

// ─────────────────────────────────────────────────────────────────────────────
// Builtin spine rels (ORCHESTRATOR PIN 2026-07-24) — the EDB rels ingest writes
// and programs join. Declared code-side (4_ingest.ts exports spineDecls; 5_diag.ts
// exports diagDecl); the http layer passes them to bridge() and boot().
//   file(path, content_hash)                                     -- 2 cols; content_hash
//                                         is the sha-256 hex of the file's raw bytes
//                                         (M8-alpha, IdentityWitnessLaw below): the
//                                         witness column a salted probe joins against
//                                         so it re-fires on a content edit
//   node(path, family, start, end, kind, name)                   -- name nullable
//   edge(path, family, kind, from_start, from_end, to_start, to_end)
//   sig(path, owner_start, owner_end, slot, pos, ty)
//   site(path, start, end, callee, callee_path)                  -- callee_path nullable
//   const(path, owner_start, owner_end, field, text, kind)       -- field nullable
//   span_line(path, start, line, col)  -- computed at ingest from file bytes: one row
//                                         per distinct span offset seen in any node/site
//                                         record of that file; line/col 0-based
//
// Kept declared HERE (not src/0_types.ts) by owner ruling at M7: this is a
// pin/law block the ledger keeps standing. src/4_ingest.ts imports SpineRelName
// straight from this file.
// ─────────────────────────────────────────────────────────────────────────────
export type SpineRelName = "file" | "node" | "edge" | "sig" | "site" | "const" | "span_line";

/** extract binary (task 0.3): env DL_EXTRACT_BIN overrides; default is the worktree
 *  debug bin. extract is CONSUME-ONLY (owner amendment 2026-07-24): never edit its
 *  crate or worktree; limitations get filed, worked around, or deferred.
 *
 *  Kept declared HERE (not src/0_types.ts) by owner ruling at M7: this is a
 *  pin/law block the ledger keeps standing. src/1_hosts.ts and src/4_ingest.ts
 *  import ExtractBinDefault straight from this file. */
export type ExtractBinDefault =
  "/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan/v6/sprefa-extract/target/debug/extract";

// ─────────────────────────────────────────────────────────────────────────────
// M2 · schema + runtime (src/2_schema.ts, src/3_runtime.ts) — types live in
// src/0_types.ts; re-exported here.
// ─────────────────────────────────────────────────────────────────────────────

/** Tick shape (b), pinned in DECISIONS: flat rel_* current tables + this log. */
export type DeltaRow = import("./src/0_types.ts").DeltaRow;
export type EdbBatch = import("./src/0_types.ts").EdbBatch;
export type TickReport = import("./src/0_types.ts").TickReport;
export type DeltaEvent = import("./src/0_types.ts").DeltaEvent;
export type IDlRuntime = import("./src/0_types.ts").IDlRuntime;
export type IHostRunner = import("./src/0_types.ts").IHostRunner;

// ─────────────────────────────────────────────────────────────────────────────
// M3 · ingest (src/4_ingest.ts) — F2 pin; types live in src/0_types.ts.
// ─────────────────────────────────────────────────────────────────────────────

export type ExtractRecord = import("./src/0_types.ts").ExtractRecord;
export type Span = import("./src/0_types.ts").Span;
export type ToFactLines = import("./src/0_types.ts").ToFactLines;
export type IngestFile = import("./src/0_types.ts").IngestFile;

// ─────────────────────────────────────────────────────────────────────────────
// M4 · hosts (src/1_hosts.ts) — types live in src/0_types.ts.
// ─────────────────────────────────────────────────────────────────────────────

export type HostDef<Req extends Row = Row, Resp extends Row = Row> = import("./src/0_types.ts").HostDef<Req, Resp>;
export type EffectCacheRow = import("./src/0_types.ts").EffectCacheRow;
export type BuiltinHosts = import("./src/0_types.ts").BuiltinHosts;

// ─────────────────────────────────────────────────────────────────────────────
// M5 · diag (src/5_diag.ts) — v5 schema verbatim (src/engine/decls.rs:263); types
// live in src/0_types.ts.
// ─────────────────────────────────────────────────────────────────────────────

export type DiagRow = import("./src/0_types.ts").DiagRow;
export type DiagDecl = import("./src/0_types.ts").DiagDecl;
export type DiagV5View = import("./src/0_types.ts").DiagV5View;

// ─────────────────────────────────────────────────────────────────────────────
// M6 · http (src/6_http.ts) — curl is the CLI; localhost; no auth; type lives in
// src/0_types.ts.
// ─────────────────────────────────────────────────────────────────────────────

export type HttpSurface = import("./src/0_types.ts").HttpSurface;
