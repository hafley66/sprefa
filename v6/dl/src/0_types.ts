/**
 * The v6/dl cross-file type system. Every
 * other src file imports its cross-file types from here (numbering law: 0 is
 * the base — nothing this file imports may be package-local). Sections below
 * mirror the pipeline order: values/rows -> decls/retention -> bridge ->
 * schema/tick -> ingest -> hosts -> diag -> http -> runtime interface.
 *
 * tasks.d.ts re-exports most of the types below rather than
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
 * This file contains types/interfaces/type-aliases only: no runtime code,
 * classes with bodies, or functions.
 */

import type { Observable, SchedulerLike } from "rxjs";
import type { Program, RelDecl, RelRef } from "sprefa-store-engine/src/lower/ast.ts";
import type { FactLine } from "sprefa-store-engine/src/engine/ingest.ts";

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

/** One finished trip through HostRunner's effect pipeline: which host it was, and how
 *  many response rows that trip committed. A cache hit (the witness already fired) and
 *  a failed run both report zero. */
export interface HostEffectDone {
  readonly host: string;
  readonly responseRows: number;
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
// Binds (M? · src/1_binds.ts) -- the INPUT-side twin of the Hosts section above.
// A HostDef answers OUTBOUND demand (`host?(args)` rows trigger a subprocess whose
// response lands on `__resp_h`); a BindDef feeds INBOUND arrival rows into an
// ordinary EDB rel with no demand row at all -- the world pushes on its own
// schedule, the program just reads the latest state (clock_residency ruling,
// rulings.pl: "cadence enters as ordinary arrival rows; SWR policy is program
// rules over latest state, NEVER runtime machinery").
//
// LINKING BY REL NAME (spine_residency ruling: the kernel learns the word
// "bind", never the word "clock" -- zero grammar changes this arc): a BindDef
// activates for a loaded program iff the program declares an EDB rel whose name
// matches `bind.rel`. An inactive bind's `source$` is never subscribed, so it
// costs nothing (no timer, no query) for a program that never mentions its rel.
// ─────────────────────────────────────────────────────────────────────────────

/** Everything a bind's source needs to decide what to emit. `runtime` is the
 *  structural IDlRuntime (never the concrete class, same numbering-law reason
 *  as HostDef.run/HostRunner above) -- a bind reads its own config rows off it
 *  (e.g. the clock bind's `clock_period` rows) the same way it will later
 *  write through it. `scheduler` is rxjs's own `SchedulerLike` (nothing
 *  invented, per the vocabulary law): the ONE seam wall-clock cadence enters
 *  this package through. A bind that schedules its own recurring work (the
 *  clock bind's `interval`) MUST take this scheduler rather than reading the
 *  ambient default, so a test can inject rxjs's `TestScheduler` and run the
 *  cadence on virtual time. Production always constructs `BindConfig` with
 *  rxjs's own `asyncScheduler` (1_binds.ts, BindRunner's constructor default),
 *  so real behavior is byte-identical to before this seam existed. `Date.now()`
 *  bucket VALUES (the clock bind's `bucketFor`) are NOT covered by this seam --
 *  they stay real wall-clock on purpose (this file's Binds section header). */
export interface BindConfig {
  readonly runtime: IDlRuntime;
  readonly scheduler: SchedulerLike;
}

/** A registered input source, symmetric to HostDef on the output side: `rel`
 *  names the EDB rel it feeds; `columns` documents its emitted row shape (Row is
 *  a Record, so only the NAMES need to match the rel's declared columns, not
 *  their order); `source$` is a cold Observable of row batches to insert. One
 *  subscription per program load (the same cold-graph region HostRunner's
 *  effects$ lives in), one commit per emitted batch -- a bind never computes its
 *  own retract: a bind whose rel needs "latest wins" relies on the rel's OWN
 *  retention marker (`rel(1)`), the same knob any program author already uses
 *  for EDB writes, rather than the bind tracking prior state itself. */
export interface BindDef {
  readonly rel: string;
  readonly columns: readonly string[];
  source$(config: BindConfig): Observable<readonly Row[]>;
}

/** One finished bind commit: which rel, how many rows that firing inserted. */
export interface BindCommitDone {
  readonly rel: string;
  readonly rows: number;
}

/** The bind driver: reads the currently loaded program's declared EDB rels,
 *  activates every registered BindDef whose `rel` matches one by name, and
 *  merges their `source$` firings into one commit stream. Cold: nothing runs
 *  (no timer starts, no config row is read) until the program's own cold graph
 *  region subscribes, and unsubscribing (program swap) IS every timer's
 *  teardown -- no dispose(), no held Subscription, matching IHostRunner's
 *  contract exactly. */
export interface IBindRunner {
  readonly commits$: Observable<BindCommitDone>;
}

/** The static side of the BindRunner class: its constructor is the whole surface.
 *  `scheduler` is the same rxjs `SchedulerLike` `BindConfig` carries -- BindRunner's
 *  constructor is the one place a `BindConfig` gets built (per bind, on activation),
 *  so it is the caller-facing seam a test or `serveDl` injects a `TestScheduler` or
 *  `asyncScheduler` through. */
export interface IBindRunnerStatics {
  new (runtime: IDlRuntime, binds: readonly BindDef[], program: Program, scheduler: SchedulerLike): IBindRunner;
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

/** DlServer: the handle the app hands out once it is listening (6_http.ts, declared
 *  here — a cross-file contract: tests/6_http.test.ts imports the type). */
export interface DlServer {
  close(): Promise<void>;
  readonly port: number;
  activeSubscribeCount(): number;
}

/** Everything the running app reports to its ONE subscriber (main.ts). Each branch of
 *  the graph emits one of these, so no branch has to throw its values away: `listening`
 *  once, carrying the handle; `served` per finished request; `delta` per tick event,
 *  emitted once by the tick stream itself and once more per SSE client it was written
 *  to (so the fan-out is visible); `effect` per finished host effect; `bind` per
 *  finished bind commit (1_binds.ts, the input-side twin of `effect`). */
export type DlAppEvent =
  | { readonly kind: "listening"; readonly server: DlServer }
  | { readonly kind: "served"; readonly method: string; readonly path: string }
  | { readonly kind: "delta"; readonly delta: DeltaEvent }
  | { readonly kind: "effect"; readonly done: HostEffectDone }
  | { readonly kind: "bind"; readonly done: BindCommitDone };

// ─────────────────────────────────────────────────────────────────────────────
// Perf trace (Phase 0 · src/0_trace.ts, plans/2026-07-27-v5-port-perf-header.md,
// SLOT-LIB filled by plans/2026-07-27-perf-tracing-buy-verdict.md). Four seams
// (SqlRunner's existing trace hook, host effects, bind commits, per-file ingest) fold
// into ONE JSONL line per tick. Field names are pinned by the header verbatim: a later
// SLOT-ENVELOPE wraps this object without renaming anything inside it.
// ─────────────────────────────────────────────────────────────────────────────

/** One finished host effect folded into a tick's perf line. `effect_id` is the
 *  effect_cache full_digest (1_hosts.ts); `cache_hit` never reaches a commit, so it
 *  is tagged with the DEMAND tick (`pending.tick`) rather than a response tick — see
 *  0_trace.ts's file header for the full seam-gap note. */
export interface PerfEffectEntry {
  readonly host: string;
  readonly effect_id: number;
  readonly ms: number;
  readonly status: "done" | "cache_hit" | "error";
}

/** One finished bind commit folded into a tick's perf line (1_binds.ts,
 *  BindRunner) -- the input-side twin of PerfEffectEntry. Generic on purpose
 *  (spine_residency ruling: the tracer knows "rel" and "row count", never
 *  "period"/"bucket" -- those stay inside the clock bind's own closure). */
export interface PerfBindEntry {
  readonly rel: string;
  readonly rows: number;
  readonly ms: number;
}

/** One file's ingest, split into every sub-span `ingestFile` (4_ingest.ts) times
 *  around its existing calls (attribution work for the unexplained-3s perf hunt,
 *  plans/2026-07-27-v5-port-perf-header.md):
 *   - `read_ms`: the file read (`fs.readFileSync`) + content hash, one IO shared by
 *     both (see 4_ingest.ts's `contentHashOf` doc). Zero on the delete-file path.
 *   - `extract_wall_ms`: `extractFile` draining start to finish — spawn through
 *     process close, so subprocess startup cost is visible here, not folded into
 *     `read_ms`/`fact_ms`. Zero on the delete-file path.
 *   - `fact_ms`: the whole `toFactLines` call (F2 mapping + the span_line byte
 *     scan), INCLUDING `read_ms` as a nested subset — `fact_ms >= read_ms` always
 *     holds when a file was read. Zero on the delete-file path.
 *   - `diff_ms`: the SPINE_REL_SCHEMA loop — `rt.rows(relName)` per rel plus the
 *     `differenceWith` insert/retract computation.
 *   - `commit_ms`: the `rt.commit(...)` await.
 *  `diff_size` is toInsert.length + toRetract.length summed across every spine rel. */
export interface PerfIngestEntry {
  readonly path: string;
  readonly read_ms: number;
  readonly extract_wall_ms: number;
  readonly fact_ms: number;
  readonly diff_ms: number;
  readonly commit_ms: number;
  readonly fact_lines: number;
  readonly diff_size: number;
}

/** One emitted JSONL line, one per tick. Shape pinned by the header:
 *  `{tick, wall_ms, stmt_count, stmt_ms_total, stmt_ms_max, effects, ingest, rss_kb}`.
 *  `rss_kb` comes from sprefa-store-engine's existing `memcap` sampler — no second
 *  RSS reader. `ingest` is null on a tick no file was ingested into. `binds` is the
 *  bind-seam addition (1_binds.ts): empty on a tick no bind fired, same convention
 *  as `effects`. */
export interface PerfTickLine {
  readonly tick: number;
  readonly wall_ms: number;
  readonly stmt_count: number;
  readonly stmt_ms_total: number;
  readonly stmt_ms_max: number;
  readonly effects: readonly PerfEffectEntry[];
  readonly binds: readonly PerfBindEntry[];
  readonly ingest: PerfIngestEntry | null;
  readonly rss_kb: number;
}

/** The perf trace module's whole surface (0_trace.ts exports `PerfTrace: IPerfTrace`).
 *  Off by default: every method below is near-free when no DL_PERF_LOG subscriber is
 *  attached (diagnostics_channel's documented unsubscribed-publish cost), and
 *  `installFromEnv`/`uninstall` are the only state transitions. */
export interface IPerfTrace {
  /** True when any of the four channels currently has a subscriber. */
  readonly enabled: boolean;
  /** Re-reads process.env.DL_PERF_LOG and installs or removes the pino subscriber
   *  accordingly. Idempotent (calling it twice with the same env is a no-op). Called
   *  once at module load for the real app; tests call it again after mutating env. */
  installFromEnv(): void;
  /** Removes the subscriber and drops every open per-tick buffer. Test cleanup. */
  uninstall(): void;
  /** A SqlRunner `TraceStatement` closure scoped to one tick. See 0_trace.ts's file
   *  header for why consecutive calls' timestamp gap IS the previous statement's
   *  wall time, with no change to TraceStatement's existing `(sql: string) => void`
   *  shape (engine/types.ts:50, pinned by tests/lower/stmtBudget.test.ts). */
  sqlTraceFor(tick: number): (sql: string) => void;
  /** Closes out `tick`'s last open statement (the one with no "next" trace() call to
   *  diff against). Call once, after the traced evaluation settles. */
  finishSqlTrace(tick: number): void;
  /** Untethered text-only publish on the same `sprefa:sql` channel: no tick, no ms,
   *  ignored by the tick-keyed aggregator (F1 test seam; `edbSql` supersedes it). */
  rawSql(sql: string): void;
  /** The single off-path boolean read `execute$` uses to decide whether to pay for
   *  EDB-plane tracing at all (the near-zero-when-off contract, 0_trace.ts header). */
  sqlActive(): boolean;
  /** One EDB-plane statement from 3_runtime.ts's `execute$` funnel, wall time in
   *  `ms`, tagged `seam: "edb"` to separate it from fixpoint tick events in a grep. */
  edbSql(sql: string, ms: number): void;
  /** One finished host effect (1_hosts.ts, HostRunner.runEffectOnce). */
  effectDone(tick: number, host: string, effectId: number, ms: number, status: "done" | "cache_hit" | "error"): void;
  /** One finished bind commit (1_binds.ts, BindRunner.commitOnce) -- the input-side
   *  twin of effectDone. */
  bindDone(tick: number, rel: string, rows: number, ms: number): void;
  /** One finished file's ingest, every sub-span already measured by the caller
   *  (4_ingest.ts, ingestFile) — see PerfIngestEntry for what each field covers. */
  ingestDone(tick: number, entry: PerfIngestEntry): void;
  /** The tick-boundary hook (3_runtime.ts, clearScratchRels): schedules this tick's
   *  JSONL line for emission once the current tick's caller-side continuation (the
   *  effectDone/ingestDone call right after `await rt.commit(...)`) has had a chance
   *  to run — see 0_trace.ts's FLUSH TIMING note. */
  tickSettled(tick: number): void;
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
  /** Path-scoped read: WHERE on the interned path id against the rel's
   *  UNIQUE(path, ...) index, never fetch-all-then-filter. The per-file
   *  ingest diff depends on this staying O(rows-for-path). */
  rowsForPath(rel: string, path: string): Promise<Row[]>;
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
 * The effect driver. Reads `runtime.deltas$`, sees `__req_<host>` rows appear, runs
 * the host process, and commits `__resp_<host>` rows back. It is a READER, not a
 * queue: the effect_cache row is the dedupe, and concatMap is the serialization lock.
 *
 * It holds the runtime only for deltas$/commit, so the parameter is typed by IDlRuntime
 * rather than the class, and takes its own CacheDb handle for the effect_cache reads and
 * writes it does on its own.
 */
export interface IHostRunner {
  /** Cold. Subscribing starts the boot replay and the live watch; unsubscribing is the
   *  whole of teardown (one-subscribe law: no start/dispose pair, no held Subscription).
   *  One value per finished effect trip. */
  readonly effects$: Observable<HostEffectDone>;
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
// into a `HostDef` that `IHostRunner` can run. `ServeDl` wires all of it to the
// http surface above, as one cold observable with one subscriber.
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

/** The whole app as one cold observable: subscribing boots the http front on
 *  `cfg.port` against `cfg.dbPath` and IS the app's lifetime; `DlServer.close()` (or
 *  unsubscribing) ends it. The server owns one mutable slot, empty until a program
 *  loads via POST /edb/program. `cfg.scheduler` is the SAME rxjs `SchedulerLike` seam
 *  `BindConfig`/`IBindRunnerStatics` take (1_binds.ts) -- optional so every existing
 *  caller (main.ts) keeps compiling unchanged; omitted, `serveDl` defaults to rxjs's
 *  own `asyncScheduler`. A test that needs virtual time for a bind's cadence (e.g. the
 *  clock bind's `interval`) passes a `TestScheduler` here instead of reaching into
 *  module state -- the only parameterization this seam needed. */
export type ServeDl = (cfg: { dbPath: string; port: number; scheduler?: SchedulerLike }) => Observable<DlAppEvent>;

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
