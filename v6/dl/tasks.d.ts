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
 */

import type { Observable } from "rxjs";
import type { Program, RelDecl, RelRef } from "sprefa-store-engine/src/lower/ast.ts";
import type { FactLine } from "sprefa-store-engine/src/engine/ingest.ts";

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
  M3_ingest_diff: { done: false; evidence: "re-POST same file = zero deltas; edit = per-file retract+insert" };
  M4_hosts: { done: false; evidence: "sg? fires once per digest; builtin-vs-sh parity rows byte-equal" };
  /** 5.3 DONE 2026-07-24 (dl/m5-lsp f4fdddbe, merged): --diag-db mode in src/lsp.rs,
   *  persistent-connection data_version poll (per-connection pragma law), retraction =
   *  empty publish; scripts/lsp_capture.mjs harness. 5.1/5.2/5.4/5.5 open. */
  M5_diag_lsp: { done: false; evidence: "curl transcript golden + LSP publish/clear pair + delta dump" };
  M6_http: { done: false; evidence: "tests/golden/curl-session.sh green against live server" };
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared row/value shapes.
// ─────────────────────────────────────────────────────────────────────────────

export type Value = string | number | boolean | null;
export type Row = Record<string, Value>;

/** Retention forms (owner ruling 2026-07-23): the decl's one capacity knob. */
export type Retention = 0 | 1 | "all";

// ─────────────────────────────────────────────────────────────────────────────
// M1 · bridge (grammar/dl.langium + src/0_ast_bridge.ts)
// ─────────────────────────────────────────────────────────────────────────────

export interface HostDecl {
  readonly name: string;
  readonly columns: readonly { name: string; ty: "text" | "int" }[];
  /** raw backtick executor body */
  readonly template: string;
  /** inferred from `{name}` / `$name` refs in template (datalog-first ruling:
   *  no mode syntax in this slice) */
  readonly inputCols: readonly string[];
}

export interface LoadDiag {
  readonly code:
    | "unknown-rel"
    | "arity-mismatch"
    | "minmax-frontier"      // Key parsed; Min/Max parsed but rejected this slice
    | "mutation-frontier"    // name!(...) parsed but rejected this slice
    | "non-stratifiable"
    | "parse";
  readonly message: string;
  readonly line: number;
  readonly col: number;
}

export interface BridgeOk {
  readonly kind: "ok";
  /** probes already rewritten: `h?(in..,out..)` -> minted __req_h rule +
   *  __resp_h EDB ref (the timecut, Lloyd-Topor free-variable law) */
  readonly program: Program;
  readonly hosts: readonly HostDecl[];
  readonly retention: ReadonlyMap<string, Retention>;
  readonly queries: readonly RelRef[];
  readonly minted: readonly string[]; // deterministic: __req_<h>, __resp_<h>, __lit_<n>
  /** ORCHESTRATOR PIN 2026-07-24 (ast.ts HeadTerm has no literal form, and Compare
   *  filters without binding): a literal head/probe-input value (`"warn" = severity`
   *  with severity otherwise unbound; a Lit arg to a probe) is rewritten to a minted
   *  single-row constant rel `__lit_<n>(value)` joined in the body. This map holds
   *  rel name -> its one row's value; the runtime seeds rel___lit_<n> at boot. Numbering
   *  is first-appearance order, so re-bridge is stable. */
  readonly literalSeeds: ReadonlyMap<string, Value>;
}
export interface BridgeErr { readonly kind: "err"; readonly diags: readonly LoadDiag[] }
export type BridgeResult = BridgeOk | BridgeErr;

/** bridge() takes the builtin rel decls (spine + diag) as a second argument so
 *  `file(path)`/`span_line(...)`/`diag(...)` refs don't raise unknown-rel. A builtin
 *  headed by a program rule becomes IDB (derived); otherwise it stays EDB. */
export type Bridge = (dlText: string, builtinRels: readonly RelDecl[]) => BridgeResult;

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
//   file(path)                                                   -- 1 col
//   node(path, family, start, end, kind, name)                   -- name nullable
//   edge(path, family, kind, from_start, from_end, to_start, to_end)
//   sig(path, owner_start, owner_end, slot, pos, ty)
//   site(path, start, end, callee, callee_path)                  -- callee_path nullable
//   const(path, owner_start, owner_end, field, text, kind)       -- field nullable
//   span_line(path, start, line, col)  -- computed at ingest from file bytes: one row
//                                         per distinct span offset seen in any node/site
//                                         record of that file; line/col 0-based
// ─────────────────────────────────────────────────────────────────────────────
export type SpineRelName = "file" | "node" | "edge" | "sig" | "site" | "const" | "span_line";

/** extract binary (task 0.3): env DL_EXTRACT_BIN overrides; default is the worktree
 *  debug bin. extract is CONSUME-ONLY (owner amendment 2026-07-24): never edit its
 *  crate or worktree; limitations get filed, worked around, or deferred. */
export type ExtractBinDefault =
  "/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan/v6/sprefa-extract/target/debug/extract";

// ─────────────────────────────────────────────────────────────────────────────
// M2 · schema + runtime (src/2_schema.ts, src/3_runtime.ts)
// ─────────────────────────────────────────────────────────────────────────────

/** Tick shape (b), pinned in DECISIONS: flat rel_* current tables + this log. */
export interface DeltaRow {
  readonly rel: string;
  readonly row_digest: number; // oracle.mix XOR law (ingest.ts note 6)
  readonly tick: number;
  readonly weight: 1 | -1;
}

export interface EdbBatch {
  readonly insert: ReadonlyMap<string, readonly Row[]>;
  readonly retract: ReadonlyMap<string, readonly Row[]>;
}

export interface TickReport {
  readonly tick: number;
  readonly changed: readonly [rel: string, delta: number][];
}

export interface DeltaEvent {
  readonly tick: number;
  readonly rel: string;
  readonly inserts: readonly Row[];
  readonly retracts: readonly Row[];
}

export interface DlRuntime {
  /** THE single write site; one call = one tick (store-owned counter). */
  commit(batch: EdbBatch): Promise<TickReport>;
  rows(rel: string): Promise<Row[]>;
  readonly deltas$: Observable<DeltaEvent>;
  dispose(): Promise<void>;
}

// ─────────────────────────────────────────────────────────────────────────────
// M3 · ingest (src/4_ingest.ts) — F2 pin lives HERE as the mapping's types
// ─────────────────────────────────────────────────────────────────────────────

/** extract's JSONL contract (recon: extract --schema, worktree bin). */
export type ExtractRecord =
  | { record: "node"; family: "cst" | "type" | "call" | "df"; span: Span; kind: string; name: string | null }
  | { record: "edge"; family: "cst" | "df"; kind: string; from: Span; to: Span }
  | { record: "sig"; family: "type"; owner: Span; slot: "param" | "ret"; pos: number; ty: string }
  | { record: "site"; family: "call"; span: Span; callee: string; callee_path: string | null }
  | { record: "const"; family: "type"; owner: Span; field: string | null; text: string; kind: "lit" | "template" };
export interface Span { readonly start: number; readonly end: number }

/** The F2 mapping (task 3.2): extract records -> the store's FactLine union.
 *  Pure; one test per record shape; answers ingest.ts note 1's ambiguities. */
export type ToFactLines = (recs: readonly ExtractRecord[], path: string) => readonly FactLine[];

/** Per-file re-ingest DIFF (supersedes ingest.ts's append-only stance for the
 *  per-file case): retraction rides commit, no diag-specific code (M5.4). */
export type IngestFile = (path: string) => Promise<TickReport>;

// ─────────────────────────────────────────────────────────────────────────────
// M4 · hosts (src/1_hosts.ts)
// ─────────────────────────────────────────────────────────────────────────────

/** Pluggable trait with assoc types (owner style law), TS spelling. */
export interface HostDef<Req extends Row = Row, Resp extends Row = Row> {
  readonly name: string;
  readonly requestCols: readonly string[];
  readonly responseCols: readonly string[];
  run(req: Req): AsyncIterable<Resp>; // det and multi both fit
}

/** effect_cache row: digest = mix(host, ...requestTuple) — the v5
 *  pending_effect law. `?` = idempotent (cache hit skips; fork ruling). */
export interface EffectCacheRow {
  readonly digest: string;
  readonly host: string;
  readonly state: "pending" | "done" | "error";
  readonly requested_tick: number;
}

/** Builtins shipped in-box; sh decls become HostDefs via shHost(decl).
 *  extract path override: env DL_EXTRACT_BIN, default the worktree debug bin. */
export interface BuiltinHosts {
  readonly sg: HostDef;      // ast-grep: sg run --pattern <p> --json <path>
  readonly extract: HostDef; // extract <path> --family <fams>
}

// ─────────────────────────────────────────────────────────────────────────────
// M5 · diag (src/5_diag.ts) — v5 schema verbatim (src/engine/decls.rs:263)
// ─────────────────────────────────────────────────────────────────────────────

export interface DiagRow {
  readonly path: string;
  readonly line: number;     // 0-based
  readonly col: number;
  readonly end_line: number; // default: line
  readonly end_col: number;
  readonly severity: "error" | "warn" | "info" | "hint"; // default warn
  readonly code: string | null;
  readonly msg: string;
  readonly hint: string | null;
}
export type DiagDecl = RelDecl; // the builtin decl instance lives in 5_diag.ts

/** The LSP interface IS this view's column list ("LSP becomes its own
 *  interfacing"): v5 `dl --lsp --diag-db <sqlite>` polls PRAGMA data_version,
 *  SELECTs diag_v5, publishes per file. Additive Rust change in src/lsp.rs. */
export type DiagV5View = "CREATE VIEW diag_v5 AS SELECT ... FROM rel_diag";

// ─────────────────────────────────────────────────────────────────────────────
// M6 · http (src/6_http.ts) — curl is the CLI; localhost; no auth
// ─────────────────────────────────────────────────────────────────────────────

export interface HttpSurface {
  "POST /edb/program": { req: "text/plain .dl"; res: { loaded: true } | { diags: LoadDiag[] } };
  "POST /edb/file_changed": { req: { path: string }; res: TickReport };
  "POST /edb/:rel": { req: { rows: Row[] }; res: TickReport };
  "GET /idb/:rel": { res: { rows: Row[] } };
  "GET /subscribe/:rel": { res: "SSE DeltaEvent stream; unsubscribes on socket close" };
  "POST /query": { req: "? rel(args)."; res: { rows: Row[] } };
}
