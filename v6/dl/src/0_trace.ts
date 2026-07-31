/**
 * Performance tracing through node:diagnostics_channel and pino.
 * as the event spine, pino as the JSONL emit layer, performance.now() as the duration
 * primitive. Off by default: DL_PERF_LOG unset means every channel here has zero
 * subscribers, and every publish site below checks `channel.hasSubscribers` before
 * doing any real work — an unsubscribed diagnostics_channel publish is documented as
 * near-free, which is the whole "near-zero when off" contract.
 *
 * The event seams are sql, effect, ingest, and bind.
 * bind-seam arc, same shape as sprefa:effect):
 *
 *   sprefa:sql — one event per SQL statement, fed from SqlRunner's EXISTING
 *   `trace?: TraceStatement` hook (engine/types.ts:50), reached through
 *   evalProgramSql -> DatalogEvaluator.exec -> SqlRunner.execute (3_runtime.ts's
 *   applyDerivedTxn passes `PerfTrace.sqlTraceFor(tick)` as evalProgramSql's 5th
 *   arg). DURATION NOTE, read before touching this: `TraceStatement` is
 *   `(sql: string) => void`, called synchronously right before `db.execute`, and
 *   never again after — there is no post-execution callback to hang a duration off
 *   without widening that hook, which tests/lower/stmtBudget.test.ts pins to exactly
 *   that one-string-argument shape (`(sql) => evaluation.push(sql)`). This module
 *   does NOT widen it — the hook already suffices, given one fact about its only
 *   caller: every statement in a run goes through `concat(...)` (lowerSql.ts's
 *   `runAll`), which never subscribes statement N+1 until statement N's observable
 *   has COMPLETED. So trace()'s call for statement N+1 fires at the exact wall-clock
 *   moment statement N finished, and the gap between two consecutive trace() calls
 *   IS statement N's wall time. `finishSqlTrace` closes out a run's LAST statement,
 *   which has no "next" call to diff against.
 *
 *   SEAM GAP (report honestly, not invented away): 3_runtime.ts's EDB-plane writes
 *   (insertRows/deleteRows/insertDeltaRows inside applyEdbTxn) run through that
 *   file's own hand-rolled `execute$`/`executeAll$` helpers, which call
 *   `db.execute` directly and never touch SqlRunner or its trace hook at all — a
 *   pre-existing gap in 3_runtime.ts, independent of this arc. Seam 1 therefore
 *   only sees the FIXPOINT/derived phase (evalProgramSql), not the raw EDB write
 *   phase. Routing applyEdbTxn's writes through SqlRunner is a real fix but is a
 *   bigger touch than "tick-boundary hook only, minimal" authorizes here.
 *
 *   sprefa:effect — one event per finished host effect (1_hosts.ts,
 *   HostRunner.runEffectOnce): `effectDone` is called explicitly at each of that
 *   method's three exit points (cache hit / success / error), because those three
 *   outcomes are NOT otherwise distinguishable from outside the method (a cache hit
 *   and a real error both currently resolve `HostEffectDone` identically). The
 *   "done" case is tagged with the tick ITS OWN response commit produced
 *   (`TickReport.tick`); "cache_hit" and "error" never reach a commit at all, so
 *   they are tagged with the DEMAND tick (`pending.tick`) instead — a real asymmetry
 *   in the underlying system, not a shortcut taken here.
 *
 *   sprefa:bind -- one event per finished bind commit (1_binds.ts, BindRunner.
 *   commitOnce). Every firing reaches a commit (a bind has no cache/demand row
 *   to short-circuit against, unlike an effect's cache_hit), so `bindDone` is
 *   called exactly once per firing, tagged with that firing's own commit tick.
 *   Generic on purpose: the event carries `rel`/`rows`/`ms` only, never
 *   anything clock-specific (spine_residency ruling -- the tracer, like the
 *   runner, knows the word "bind", never the word "clock").
 *
 *   sprefa:ingest — one event per file, now split into every sub-span `ingestFile`
 *   (4_ingest.ts) measures (read_ms/extract_wall_ms/fact_ms/diff_ms/commit_ms — see
 *   PerfIngestEntry in 0_types.ts for what each covers; this is the attribution work
 *   for the unexplained-3s-per-60-files perf hunt). Tagged with the commit's own
 *   `TickReport.tick`, known only once `rt.commit()` resolves.
 *
 * Aggregation uses exactly one subscriber per channel, installed only when DL_PERF_LOG
 * is set (`installFromEnv`, called once at import time for the real app; tests call
 * it again after mutating env — idempotent either way). It folds events into a
 * per-tick buffer and writes ONE pino JSONL line per tick — never one write per
 * statement/effect/bind/file. The N+1 law applies to this module's own output
 * exactly as much as it applies to the engine's SQL.
 *
 * FLUSH TIMING: a tick's SQL statements complete synchronously as part of that same
 * tick's commit; a tick's effect/ingest spans (measured by the CALLER of
 * DlRuntime.commit, in 1_hosts.ts/4_ingest.ts) can only be known to have "settled"
 * strictly AFTER that commit's promise resolves back to the caller — which is
 * strictly AFTER 3_runtime.ts's tick-settle machinery (`reportsSubject.next`) has
 * already run (that `next()` call is WHY the commit promise resolves at all). So
 * `tickSettled` (the tick-boundary hook 3_runtime.ts calls from `clearScratchRels`)
 * does not flush synchronously: it schedules the flush on `setImmediate`, which Node
 * runs only after the current microtask queue drains — specifically after the
 * awaited `commit()` promise's continuation in the caller (the synchronous
 * `effectDone`/`ingestDone` call that immediately follows `await rt.commit(...)` in
 * 1_hosts.ts/4_ingest.ts, with no further `await` in between) has already run. A
 * caller that awaits something else between `commit()` resolving and publishing its
 * span would miss this tick's line; neither of this module's two real callers does.
 */

import diagnostics_channel from "node:diagnostics_channel";

import pino, { type Logger } from "pino";

import { memcap } from "sprefa-store-engine/src/engine/measure.ts";

import type { IPerfTrace, PerfBindEntry, PerfEffectEntry, PerfIngestEntry, PerfTickLine } from "./0_types.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Channels. diagnostics_channel.channel(name) is a process-global registry: the
// same name always resolves to the same channel object, in this module or a test
// importing "node:diagnostics_channel" directly.
// ─────────────────────────────────────────────────────────────────────────────

export const PERF_CHANNEL_NAMES = {
  sql: "sprefa:sql",
  effect: "sprefa:effect",
  bind: "sprefa:bind",
  ingest: "sprefa:ingest",
} as const;

const sqlChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql);
const effectChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.effect);
const bindChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.bind);
const ingestChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.ingest);

// ─────────────────────────────────────────────────────────────────────────────
// Event shapes carried on the channels (internal to this module; the pinned public
// shape is PerfTickLine, in 0_types.ts).
// ─────────────────────────────────────────────────────────────────────────────

interface SqlEvent {
  readonly tick: number;
  readonly sql: string;
  readonly ms: number;
}

interface EffectEvent {
  readonly tick: number;
  readonly host: string;
  readonly effectId: number;
  readonly ms: number;
  readonly status: "done" | "cache_hit" | "error";
}

interface BindEvent {
  readonly tick: number;
  readonly rel: string;
  readonly rows: number;
  readonly ms: number;
}

interface IngestEvent extends PerfIngestEntry {
  readonly tick: number;
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

// ─────────────────────────────────────────────────────────────────────────────
// Open-statement tracking for the SQL seam's timestamp-diff trick (see file header).
// Kept separate from the per-tick aggregation buffer below: this map exists
// regardless of whether anyone is subscribed to fold its output, because the timer
// itself has to start on the FIRST trace() call, before we know a subscriber cares.
// ─────────────────────────────────────────────────────────────────────────────

interface OpenStatement {
  readonly startedAt: number;
  readonly sql: string;
}

const openStatementByTick = new Map<number, OpenStatement>();

function closeOpenStatement(tick: number, now: number): void {
  const open = openStatementByTick.get(tick);
  if (!open) return;
  openStatementByTick.delete(tick);
  sqlChannel.publish({ tick, sql: open.sql, ms: now - open.startedAt } satisfies SqlEvent);
}

function sqlTraceFor(tick: number): (sql: string) => void {
  return (sql: string) => {
    if (!sqlChannel.hasSubscribers) return;
    const now = performance.now();
    noteTickTouched(tick, now);
    closeOpenStatement(tick, now);
    openStatementByTick.set(tick, { startedAt: now, sql });
  };
}

function finishSqlTrace(tick: number): void {
  if (!sqlChannel.hasSubscribers) {
    openStatementByTick.delete(tick);
    return;
  }
  closeOpenStatement(tick, performance.now());
}

/** Untethered from any tick: no `ms`, no open/close bookkeeping, and (see
 *  onSqlMessage above) invisible to the tick-keyed aggregator. See IPerfTrace's
 *  doc comment (0_types.ts) for why this exists and why it is safe to call from
 *  every statement 3_runtime.ts runs, unconditionally. */
function rawSql(sql: string): void {
  if (!sqlChannel.hasSubscribers) return;
  sqlChannel.publish({ sql } satisfies Pick<SqlEvent, "sql">);
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-tick aggregation buffer. Populated ONLY by the subscriber functions below,
// which are attached to the channels only while DL_PERF_LOG is set — so this map
// stays empty (and every fold below is dead code) whenever tracing is off.
// ─────────────────────────────────────────────────────────────────────────────

interface TickBuffer {
  stmtCount: number;
  stmtMsTotal: number;
  stmtMsMax: number;
  effects: PerfEffectEntry[];
  binds: PerfBindEntry[];
  ingest: PerfIngestEntry | null;
}

const buffers = new Map<number, TickBuffer>();
/** First wall-clock touch of each tick, across all four seams, independent of
 *  whether that seam's event ever gets folded (so wall_ms reflects real elapsed
 *  time even for a tick a subscriber only starts seeing partway through). */
const tickFirstSeenAt = new Map<number, number>();

function noteTickTouched(tick: number, now: number): void {
  if (!tickFirstSeenAt.has(tick)) tickFirstSeenAt.set(tick, now);
}

function bufferFor(tick: number): TickBuffer {
  const existing = buffers.get(tick);
  if (existing) return existing;
  const created: TickBuffer = { stmtCount: 0, stmtMsTotal: 0, stmtMsMax: 0, effects: [], binds: [], ingest: null };
  buffers.set(tick, created);
  return created;
}

function onSqlMessage(message: unknown): void {
  const event = message as SqlEvent;
  // rawSql (F1's test-observability seam, below) publishes on this SAME channel with
  // no `tick` at all -- ignore anything that isn't a real tick-keyed SqlEvent so a raw
  // publish never mints a phantom buffer entry that no real tick ever flushes.
  if (typeof event.tick !== "number") return;
  const buffer = bufferFor(event.tick);
  buffer.stmtCount += 1;
  buffer.stmtMsTotal += event.ms;
  buffer.stmtMsMax = Math.max(buffer.stmtMsMax, event.ms);
}

function onEffectMessage(message: unknown): void {
  const event = message as EffectEvent;
  bufferFor(event.tick).effects.push({
    host: event.host,
    effect_id: event.effectId,
    ms: round2(event.ms),
    status: event.status,
  });
}

function onBindMessage(message: unknown): void {
  const event = message as BindEvent;
  bufferFor(event.tick).binds.push({
    rel: event.rel,
    rows: event.rows,
    ms: round2(event.ms),
  });
}

function onIngestMessage(message: unknown): void {
  const event = message as IngestEvent;
  // One file = one commit = one tick in every call site this arc ships (4_ingest.ts's
  // ingestFile), so "last write wins" never actually overwrites a sibling's data.
  // Documented, not enforced: a future caller batching multiple files into one tick
  // would need this to become an array, matching `effects`.
  bufferFor(event.tick).ingest = {
    path: event.path,
    read_ms: round2(event.read_ms),
    extract_wall_ms: round2(event.extract_wall_ms),
    fact_ms: round2(event.fact_ms),
    diff_ms: round2(event.diff_ms),
    commit_ms: round2(event.commit_ms),
    fact_lines: event.fact_lines,
    diff_size: event.diff_size,
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// effectDone / ingestDone — the two remaining publish sites (sqlTraceFor/
// finishSqlTrace above are the third).
// ─────────────────────────────────────────────────────────────────────────────

function effectDone(
  tick: number,
  host: string,
  effectId: number,
  ms: number,
  status: "done" | "cache_hit" | "error",
): void {
  if (!effectChannel.hasSubscribers) return;
  noteTickTouched(tick, performance.now());
  effectChannel.publish({ tick, host, effectId, ms, status } satisfies EffectEvent);
}

function bindDone(tick: number, rel: string, rows: number, ms: number): void {
  if (!bindChannel.hasSubscribers) return;
  noteTickTouched(tick, performance.now());
  bindChannel.publish({ tick, rel, rows, ms } satisfies BindEvent);
}

function ingestDone(tick: number, entry: PerfIngestEntry): void {
  if (!ingestChannel.hasSubscribers) return;
  noteTickTouched(tick, performance.now());
  ingestChannel.publish({ tick, ...entry } satisfies IngestEvent);
}

// ─────────────────────────────────────────────────────────────────────────────
// Flush: one pino .info() call per tick, never per event (N+1 law). rss_kb reuses
// sprefa-store-engine's existing memcap sampler — no second RSS reader.
// ─────────────────────────────────────────────────────────────────────────────

let logger: Logger | null = null;

function flushTick(tick: number): void {
  const buffer = buffers.get(tick);
  const firstSeenAt = tickFirstSeenAt.get(tick);
  buffers.delete(tick);
  tickFirstSeenAt.delete(tick);
  if (!buffer || firstSeenAt === undefined || !logger) return;

  const line: PerfTickLine = {
    tick,
    wall_ms: round2(performance.now() - firstSeenAt),
    stmt_count: buffer.stmtCount,
    stmt_ms_total: round2(buffer.stmtMsTotal),
    stmt_ms_max: round2(buffer.stmtMsMax),
    effects: buffer.effects,
    binds: buffer.binds,
    ingest: buffer.ingest,
    rss_kb: Math.floor(memcap.sample() / 1024),
  };
  logger.info(line);
}

/** The tick-boundary hook (3_runtime.ts's clearScratchRels): closes out this tick's
 *  last open SQL statement, then schedules the flush — see file header FLUSH TIMING
 *  for why this is not synchronous. */
function tickSettled(tick: number): void {
  if (
    !sqlChannel.hasSubscribers &&
    !effectChannel.hasSubscribers &&
    !bindChannel.hasSubscribers &&
    !ingestChannel.hasSubscribers
  ) {
    openStatementByTick.delete(tick);
    return;
  }
  finishSqlTrace(tick);
  setImmediate(() => flushTick(tick));
}

// ─────────────────────────────────────────────────────────────────────────────
// install / uninstall — the only state transitions. Off by default: nothing here
// runs until DL_PERF_LOG is set.
// ─────────────────────────────────────────────────────────────────────────────

let installedPath: string | null = null;

function uninstall(): void {
  if (installedPath === null) return;
  sqlChannel.unsubscribe(onSqlMessage);
  effectChannel.unsubscribe(onEffectMessage);
  bindChannel.unsubscribe(onBindMessage);
  ingestChannel.unsubscribe(onIngestMessage);
  logger = null;
  installedPath = null;
  buffers.clear();
  tickFirstSeenAt.clear();
  openStatementByTick.clear();
}

function install(destinationPath: string): void {
  if (installedPath === destinationPath) return; // already installed for this path
  if (installedPath !== null) uninstall();
  logger = pino({ base: null }, pino.destination({ dest: destinationPath, sync: true, mkdir: true }));
  sqlChannel.subscribe(onSqlMessage);
  effectChannel.subscribe(onEffectMessage);
  bindChannel.subscribe(onBindMessage);
  ingestChannel.subscribe(onIngestMessage);
  installedPath = destinationPath;
}

function installFromEnv(): void {
  const destinationPath = process.env.DL_PERF_LOG;
  if (!destinationPath) {
    uninstall();
    return;
  }
  install(destinationPath);
}

export const PerfTrace: IPerfTrace = {
  get enabled(): boolean {
    return (
      sqlChannel.hasSubscribers ||
      effectChannel.hasSubscribers ||
      bindChannel.hasSubscribers ||
      ingestChannel.hasSubscribers
    );
  },
  installFromEnv,
  uninstall,
  sqlTraceFor,
  finishSqlTrace,
  rawSql,
  effectDone,
  bindDone,
  ingestDone,
  tickSettled,
};

// Real-app boot: off unless DL_PERF_LOG was set before this module first loaded.
// Every import of this module resolves to the same module instance (Node's ESM
// cache), so this runs exactly once per process.
installFromEnv();
