/**
 * 0_types.ts — the entire v6/dl type system in one file, C-header style. Every
 * other src file imports its cross-file types from here (numbering law: 0 is
 * the base — nothing this file imports may be package-local). Sections below
 * mirror the pipeline order: values/rows -> decls/retention -> bridge ->
 * schema/tick -> ingest -> hosts -> diag -> http -> runtime interface.
 *
 * tasks.d.ts (the plan ledger) re-exports most of the types below rather than
 * declaring them, so external importers of tasks.d.ts keep working. Four
 * items stay declared ONLY in tasks.d.ts and are NOT duplicated here, by
 * owner ruling (M7 recomposition): the two pure-prose law types NamedArgLaw
 * and DiagHeadDefaultLaw (never imported as types anywhere, referenced only
 * in comments), and SpineRelName / ExtractBinDefault (real cross-file types,
 * kept standing in the ledger as pin/law blocks — 4_ingest.ts and 1_hosts.ts
 * import those two specifically from ../tasks.d.ts, a scoped exception to
 * the "every src file imports from 0_types.ts" rule, documented at each of
 * those two import sites).
 *
 * This file is types/interfaces/type-aliases ONLY — no runtime code, no
 * classes with bodies, no functions. Doc comments below were moved verbatim
 * from tasks.d.ts (and, for CacheDb/DlServer, from src/1_hosts.ts and
 * src/6_http.ts) — not rewritten, except where a comment referenced its old
 * location.
 */

import type { Observable } from "rxjs";
import type { Program, RelDecl, RelRef } from "sprefa-store-engine/src/lower/ast.ts";
import type { FactLine } from "sprefa-store-engine/src/engine/ingest.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Values / rows — the shared scalar + record shapes every later section builds on.
// ─────────────────────────────────────────────────────────────────────────────

export type Value = string | number | boolean | null;
export type Row = Record<string, Value>;
export type ColumnType = "text" | "int";

// ─────────────────────────────────────────────────────────────────────────────
// Decls / retention.
// ─────────────────────────────────────────────────────────────────────────────

/** Retention forms (owner ruling 2026-07-23): the decl's one capacity knob. */
export type Retention = 0 | 1 | "all";

// ─────────────────────────────────────────────────────────────────────────────
// Bridge (M1 · grammar/dl.langium + src/0_ast_bridge.ts)
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
    | "named-arg"            // kwarg law violation: positional after named, duplicate
                             // name, name+position slot collision, unknown column name
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
  /** OWNER ESCALATION 2026-07-24 PM (M9, columnType flow): per-rel resolved column
   *  affinity, positional and parallel to the rel's `program.rels` column list.
   *  `"int"` = a numeric column stored raw INTEGER; `"text"` = a text column the
   *  storage plane stores as a `strings` dictionary id (interned) and resolves back
   *  to text at the read view. The storage DDL (2_schema.ts) reads this to declare
   *  affinity per column and to decide which columns intern; without it every rel_*
   *  column is untyped (the v5 amplification disease). Resolution law + tie-breaks
   *  live at the inference site in 0_ast_bridge.ts (buildColumnTypes). */
  readonly columnTypes: ReadonlyMap<string, readonly ColumnType[]>;
}
export interface BridgeErr { readonly kind: "err"; readonly diags: readonly LoadDiag[] }
export type BridgeResult = BridgeOk | BridgeErr;

/** bridge() takes the builtin rel decls (spine + diag) as a second argument so
 *  `file(path)`/`span_line(...)`/`diag(...)` refs don't raise unknown-rel. A builtin
 *  headed by a program rule becomes IDB (derived); otherwise it stays EDB. */
export type Bridge = (dlText: string, builtinRels: readonly RelDecl[]) => BridgeResult;

// ─────────────────────────────────────────────────────────────────────────────
// Schema / tick (M2 · src/2_schema.ts, src/3_runtime.ts)
// ─────────────────────────────────────────────────────────────────────────────

/** Tick shape (b), pinned in DECISIONS: flat rel_* current tables + this log. */
export interface DeltaRow {
  /** The rel's DENSE TAG (0..n-1, declaration order), resolvable to a name through the
   *  `rel_tag` table. Not the rel name's string-dictionary id: that id space is shared
   *  with every interned string in the database, so it is sparse and cannot be packed
   *  into `cascade.key(tag, row_id)`. */
  readonly rel_tag: number;
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

// ─────────────────────────────────────────────────────────────────────────────
// Ingest (M3 · src/4_ingest.ts) — F2 pin lives HERE as the mapping's types
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

// ─────────────────────────────────────────────────────────────────────────────
// Hosts (M4 · src/1_hosts.ts)
// ─────────────────────────────────────────────────────────────────────────────

/** Pluggable trait with assoc types (owner style law), TS spelling. */
export interface HostDef<Req extends Row = Row, Resp extends Row = Row> {
  readonly name: string;
  readonly requestCols: readonly string[];
  readonly responseCols: readonly string[];
  run(req: Req): AsyncIterable<Resp>; // det and multi both fit
}

/** effect_cache row (M8-beta reshape, IdentityWitnessLaw): full_digest = mix(host,
 *  ...identityCols, ...saltCols) is the PRIMARY KEY — fire-once per WITNESS, not per
 *  address. identity_digest = mix(host, ...identityCols) groups every witness of the
 *  same identity (the supersession group; indexed, not unique — a row per witness can
 *  share it). `?` = idempotent on full_digest (cache hit skips; fork ruling); a NEW
 *  full_digest within a live identity_digest group triggers supersession (1_hosts.ts's
 *  HostRunner) instead of a second independent fire. */
export interface EffectCacheRow {
  readonly full_digest: number;
  readonly identity_digest: number;
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

/** HostRunner's cacheDb param (1_hosts.ts, moved here — a cross-file contract:
 *  6_http.ts's side connection and tests/2_helpers_hosts.ts's fixture client
 *  both satisfy this shape). cacheDb is a THIRD structural param (pinned
 *  resolution, per the escalation ruling): HostRunner receives the runtime
 *  only for deltas$/commit, and a libsql-shaped client for the effect_cache
 *  reads/writes it needs on its own — the same db the runtime booted with
 *  (tests pass that same client, or a fresh open_db() on the same file
 *  path; both are documented as acceptable). */
export interface CacheDb {
  execute(stmt: string | { sql: string; args: unknown[] }): Promise<{ rows: unknown[] }>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Timecut arm protocol (FRONTIER · forward contract, not this slice). A probe/
// mutation with a match block — `h?(in){ next(out) -> .., error(e) -> .., .. }`,
// same block on `h!(..)` — materializes the host's `HostDef.run` lifecycle as
// rows on `__resp_<h>` (a `kind` discriminant), and the `{ }` arms pattern-match
// them. The arm set IS an rxjs 7.8.2 `tap` observer, mimicked VERBATIM
// (dist/cjs/internal/operators/tap.js): same forward grammar, same order, same
// terminal rules. Unidirectional (source -> downstream), so a datalog forward
// grammar expresses it; nothing here is bespoke JS control flow.
// ─────────────────────────────────────────────────────────────────────────────

/** The Observable Contract as a discriminant, materialized onto `__resp_<h>`.
 *  Grammar (tap.js:16-38):  subscribe -> next* -> (complete | error | unsub) -> finalize
 *  - subscribe: sync at demand, before any value (tap.js:16) = `__req_<h>` appears.
 *  - next:      per value, peek-then-forward (tap.js:20-21). No block = implicit
 *               single `next(out)` arm binding the host's output cols ("assume next").
 *  - complete:  terminal, sets isUnsub=false (tap.js:24-26). = normal return.
 *  - error:     terminal, sets isUnsub=false (tap.js:29-31). Propagates like await/
 *               yield (Subscriber.js:82-89): an unmatched `error` bubbles up the
 *               timecut chain and faults at the top (ConsumerObserver.error rethrow,
 *               :120-133,188). A matched `error(e)` arm is a `catch`.
 *  - unsub:     teardown with NO terminal, isUnsub stayed true (tap.js:34-35). The
 *               switchMap-cancel case: demand withdrawn before a terminal.
 *  - finalize:  ALWAYS, exactly once, after the terminal (tap.js:37) = `finally`. */
export type NotifyKind = "subscribe" | "next" | "complete" | "error" | "unsub" | "finalize";

/** The three mutually-exclusive terminals. A terminal sets isStopped (Subscriber.js:
 *  59,68); any later notification for the same req_id is a stopped-notification and
 *  dropped (:48,55,64). complete XOR error XOR unsub; finalize is the shared epilogue. */
export type TerminalKind = Extract<NotifyKind, "complete" | "error" | "unsub">;

/** A materialized notification row on `__resp_<h>`: the host's output cols plus the
 *  discriminant, the per-invocation id, and `seq` (orders notifications WITHIN a
 *  req_id — the total order rxjs has for free; a tick alone does not order intra-tick
 *  events). `err` is set only for kind="error"; complete/unsub/finalize carry neither
 *  output nor err. */
export interface NotifyRow extends Row {
  readonly req_id: number;
  readonly kind: NotifyKind;
  readonly seq: number;
  readonly err: string | null;
}

/** One `{ }` arm: the kind it listens for, and the rule head it rewrites to (minted
 *  Lloyd-Topor, same as a probe body). Which kinds are matched declares what the
 *  author listens for; unmatched next binds outputs, unmatched error bubbles,
 *  unmatched complete/unsub end quietly (the await-parity defaults above). This
 *  supersedes `takeUntil` for the self-terminal case: match the terminal arm.
 *  (Cross-terminal "take until ANOTHER stream fires" is unspecified this slice.) */
export interface TimecutArm {
  readonly on: NotifyKind;
  readonly head: RelRef;
}

// PARKED (revisit, not this slice): retention `rel(N)` splits into two modes — replay/
// multicast (ReplaySubject(N), non-consuming, push, glitch-free via the tick) vs
// channel (consuming, unicast, pull, backpressure as an occupancy gate). Blocking
// channels bring deadlock + fairness + unicast constraints. Flatten-operator selection
// (concatMap default / exhaustMap / mergeMap; switchMap rejected for `!` as it cancels
// an irreversible effect) rides the host decl. None of it lands here yet.

// ─────────────────────────────────────────────────────────────────────────────
// Diag (M5 · src/5_diag.ts) — v5 schema verbatim (src/engine/decls.rs:263)
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
// Http (M6 · src/6_http.ts) — curl is the CLI; localhost; no auth
// ─────────────────────────────────────────────────────────────────────────────

export interface HttpSurface {
  "POST /edb/program": { req: "text/plain .dl"; res: { loaded: true } | { diags: LoadDiag[] } };
  "POST /edb/file_changed": { req: { path: string }; res: TickReport };
  "POST /edb/:rel": { req: { rows: Row[] }; res: TickReport };
  "GET /idb/:rel": { res: { rows: Row[] } };
  "GET /subscribe/:rel": { res: "SSE DeltaEvent stream; unsubscribes on socket close" };
  "POST /query": { req: "? rel(args)."; res: { rows: Row[] } };
}

/** DlServer: startServer()'s public return shape (6_http.ts, moved here — a
 *  cross-file contract: tests/6_http.test.ts imports the type). */
export interface DlServer {
  close(): Promise<void>;
  readonly port: number;
  activeSubscribeCount(): number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Class contracts — I-prefixed, one per class in this package. src/3_runtime.ts
// and src/1_hosts.ts hold the concrete classes; every other file depends on the
// interface only, never the class, so nothing imports upward past the numbering
// law. Same convention as sprefa-store-engine/src/engine/types.ts.
// ─────────────────────────────────────────────────────────────────────────────

export interface IDlRuntime {
  /** THE single write site; one call = one tick (store-owned counter). */
  commit(batch: EdbBatch): Promise<TickReport>;
  rows(rel: string): Promise<Row[]>;
  readonly deltas$: Observable<DeltaEvent>;
  dispose(): Promise<void>;
}

/** The static side of the DlRuntime class. Its constructor is private: `boot` is the
 *  only way in (it must await DDL, lowering, and the interner hydrate). */
export interface IDlRuntimeStatics {
  boot(cfg: {
    dbPath: string;
    bridge: BridgeOk;
    extraDdl?: readonly string[];
  }): Promise<IDlRuntime>;
}

/**
 * The effect driver. Subscribes `runtime.deltas$`, sees `__req_<host>` rows appear, runs
 * the host process, and commits `__resp_<host>` rows back. It is a SUBSCRIBER, not a
 * queue: the effect_cache row is the dedupe, and concatMap is the serialization lock.
 *
 * It holds the runtime only for deltas$/commit, so the parameter is typed by IDlRuntime
 * rather than the class, and takes its own CacheDb handle for the effect_cache reads and
 * writes it does on its own.
 */
export interface IHostRunner {
  /** Idempotent: a second call while already subscribed is a no-op. */
  start(): void;
  /** Unsubscribes. Safe to call without a prior start(). */
  dispose(): void;
}

/** The static side of the HostRunner class: its constructor is the whole surface. */
export interface IHostRunnerStatics {
  new (runtime: IDlRuntime, hosts: readonly HostDef[], cacheDb: CacheDb): IHostRunner;
}

// ─────────────────────────────────────────────────────────────────────────────
// DATAFLOW · the top-level functions that move data across the interfaces above.
//
// One named type per entry point, so the header shows what a stage consumes and
// what it produces without opening the implementation. Each implementing file
// exports an `AssertTrue<...>` alias proving its function still matches; nothing
// else binds a free function to a declared type.
//
// The pipeline in one read: text enters through `Bridge`; a file enters through
// `IngestFile`; both land in `IDlRuntime.commit`, the single write site. `Ddl`
// mints the tables that commit writes into. `ShHost` turns a parsed `sh` decl
// into a `HostDef` that `IHostRunner` can run. `StartServer` wires all of it to
// the http surface above.
// ─────────────────────────────────────────────────────────────────────────────

/** decl set -> the DDL statement list (delta, effect_cache, store_meta, tick seed, views). */
export type Ddl = (
  decls: readonly RelDecl[],
  retention: ReadonlyMap<string, Retention>,
  columnTypes?: ReadonlyMap<string, readonly ColumnType[]>,
) => string[];

/** One file on disk -> extract records -> spine rows -> one tick. Retracts what the file
 *  no longer says, including the delete-file case (a missing path retracts everything).
 *
 *  Per-file re-ingest is a DIFF (this supersedes ingest.ts's append-only stance for the
 *  per-file case): retraction rides commit, with no diag-specific code (M5.4).
 *
 *  This declaration used to sit up in the ingest section MISSING its first parameter,
 *  written `(path: string) => Promise<TickReport>` while the real `ingestFile` had taken
 *  `(rt, filePath)` since M3. Nothing caught it because no implementation was ever
 *  checked against it. The `IngestFileHolds` proof in 4_ingest.ts is why it cannot drift
 *  again, and is the reason this section exists at all. */
export type IngestFile = (rt: IDlRuntime, filePath: string) => Promise<TickReport>;

/** A parsed `sh` decl -> a runnable host. The template's `{col}` splices into the command
 *  line and `$col` goes to the environment; output is parsed as JSON, JSONL, or columns. */
export type ShHost = (decl: HostDecl) => HostDef;

/** Boot the http front on `cfg.port` against `cfg.dbPath`. The server owns one mutable
 *  slot, empty until a program loads via POST /edb/program. */
export type StartServer = (cfg: { dbPath: string; port: number }) => Promise<DlServer>;

// ─────────────────────────────────────────────────────────────────────────────
// Static-side proof helper. `implements` covers a class's instance side and
// cannot see statics, and nothing at all binds a free function to a declared
// type. The `false` branch is what makes it bite: `never extends true` is true,
// so a `never` branch would prove nothing.
// ─────────────────────────────────────────────────────────────────────────────

export type AssertTrue<Holds extends true> = Holds;

/** Decoding driver rows into `Value`. One copy: the two that existed had drifted, and the
 *  one without a `bigint` branch turned every INTEGER column into text. */
export interface IRowCodec {
  normalizeValue(raw: unknown): Value;
  rowFromRaw(rawRow: unknown, columns: readonly string[]): Row;
}
