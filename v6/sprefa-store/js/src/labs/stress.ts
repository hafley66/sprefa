/**
 * stress.ts — E1, the stress gun. Synthesizes a layered dl program (chains + diamonds +
 * recursive strata), runs it on BOTH backends (the live rx lowering, and a from-scratch
 * SQL reconcile runner built for this lab), churns EDB rows, and reports wake selectivity,
 * RSS slope, tick latency, and stmt cost. The two backends are a cross-oracle: their final
 * sink digests must agree.
 *
 * Backend split:
 *   rx  — `lowerProgram` (src/lower/lower.ts) over injected ReplaySubject sources. Wake is
 *         measured by counting which rel subscriptions receive a NEW emission per tick (a
 *         cold-by-default rx graph only re-fires the branches downstream of what changed).
 *   sql — mints one rel table per synth rel (spine.rels.create_rel_table), stratifies the
 *         SAME program via rulegraph.ts (byte-identical structure to what lower.ts would
 *         build), and drives reconcile.mark_changed/propagate over one id per stratum
 *         (recursive strata collapse to ONE id — propagate's ascending-id sweep assumes an
 *         acyclic rx_dep, so a cyclic rel group cannot own more than one id). The actual row
 *         rebuild per stratum is done by hand here (a small rule->SQL compiler scoped to the
 *         RelRef/HeadVar shapes this synth emits — Lit/Compare/aggregation throw, they are
 *         not needed and the generalized version is ARC4's job, not this lab's), reusing the
 *         labs/fixpoint.ts SQL-fixpoint pattern for the recursive stratum's inner loop.
 *         `reconcile.propagate`'s `recompute` callback is synchronous by contract; the real
 *         (async) SQL rebuild runs BEFORE propagate, in topo order over the touched leaves'
 *         downstream cone, and `recompute` is reduced to a cache lookup of the digest that
 *         rebuild just computed. propagate still owns the real early-cutoff wake decision
 *         (comparing each digest against its PREVIOUS verified value in rx_memo) — the
 *         pre-rebuild is a superset (the whole reachable cone), the wake COUNT it reports is
 *         the authentic selective figure.
 *
 * Retraction (task 1.4, owner amendment 2026-07-23 PM): NOT a ruling in this arc. An
 * OPTIONAL timing note only — retract vs retract_scc vs retract_dred_cte on one cyclic synth
 * graph, printed by the CLI, wrapped so a failure there cannot fail the module or the gate.
 */

import { ReplaySubject, type Observable, type Subscription } from "rxjs";
import { createClient } from "@libsql/client";
import Database from "better-sqlite3";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";
import process from "node:process";

import {
  edbRel,
  derivedRel,
  relRef,
  v,
  headVar,
  type Program,
  type Rule,
  type RelDecl,
  type RelRef,
} from "../lower/ast.ts";
import { lowerProgram, RecursiveStratumDeferred, type Row, type Sources } from "../lower/lower.ts";
import { buildRuleGraph, scc, stratify } from "../lower/rulegraph.ts";
import type { Stratum } from "../lower/types.ts";

import { RelStore, GraphNs } from "../engine/lib.ts";
import type { SqliteDb as Db } from "../engine/types.ts";
import { reconcile, stmt_counter } from "../engine/engine.ts";
import { OPEN_PRAGMAS } from "../engine/spine.ts";
import { memcap, benchgraph } from "../engine/measure.ts";
import { mix } from "../engine/oracle.ts";

// ═════════════════════════════════════════════════════════════════════════════
// Public contract (E1, verbatim from the plan).
// ═════════════════════════════════════════════════════════════════════════════

export interface GunConfig {
  rels: number;
  strataDepth: number;
  diamondWidth: number;
  rowsPerRel: number;
  churnTicks: number;
  churnRowsPerTick: number;
  seed: number;
}

export interface GunReport {
  peakRssMib: number;
  rssSlope: number; // MiB per 100 churn ticks
  wakeMedian: number; // rels recomputed per tick
  wakeP95: number;
  stmtsPerTick: number;
  msPerTickP50: number;
  msPerTickP95: number;
}

/** `runGun` plus the sink digest — the cross-backend oracle key. Not part of the pinned
 *  contract (a strict superset); the golden test and the CLI both want the digest. */
export interface GunRunDetail {
  report: GunReport;
  sinkDigest: string;
}

export async function runGun(cfg: GunConfig, backend: "rx" | "sql"): Promise<GunReport> {
  return (await runGunDetailed(cfg, backend)).report;
}

export async function runGunDetailed(cfg: GunConfig, backend: "rx" | "sql"): Promise<GunRunDetail> {
  return backend === "rx" ? runRxBackend(cfg) : runSqlBackend(cfg);
}

// ═════════════════════════════════════════════════════════════════════════════
// Seeded PRNG — mulberry32. Public-domain, one deterministic function; not a build-vs-buy
// case (no queue/parser/scheduler shape, no dependency to evaluate).
// ═════════════════════════════════════════════════════════════════════════════

function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// ═════════════════════════════════════════════════════════════════════════════
// 1.1 — synthProgram: a layered DAG of derived rels. Every rel has columns [id, val].
// Layer 0 = EDB leaves (width = ceil(rels/strataDepth)); each later layer is the SAME
// width, each derived rel reading ONE parent at the same index (a chain) — except every
// `diamondWidth`-th index in a layer ALSO reads a second, neighboring parent (a diamond
// merge), and every 32nd rel constructed (global counter) becomes a 2-member mutual-
// recursion pair (nameEven <-> nameOdd) seeded from its chain parent(s) instead of a plain
// derived rel. This keeps wake bounded (mostly single-chain propagation, occasional forks
// at diamond points) rather than exponential fan-out.
// ═════════════════════════════════════════════════════════════════════════════

export function synthProgram(cfg: GunConfig): { prog: Program; sources: Sources } {
  const rng = mulberry32(cfg.seed);
  const depth = Math.max(1, Math.floor(cfg.strataDepth));
  const diamondEvery = Math.max(1, Math.floor(cfg.diamondWidth));
  const perLayer = Math.max(1, Math.ceil(cfg.rels / depth));

  const rels: RelDecl[] = [];
  const rules: Rule[] = [];
  const sources: Sources = new Map();

  let total = 0;
  let globalCounter = 0;
  let prevLayer: string[] = [];

  for (let layer = 0; layer < depth && total < cfg.rels; layer++) {
    const width = Math.min(perLayer, cfg.rels - total);
    const thisLayer: string[] = new Array(width);

    if (layer === 0) {
      for (let w = 0; w < width; w++) {
        const name = `leaf_${w}`;
        rels.push(edbRel(name, ["id", "val"]));
        const rows: Row[] = [];
        for (let i = 0; i < cfg.rowsPerRel; i++) rows.push([i, Math.floor(rng() * 1_000_000)]);
        const subject = new ReplaySubject<Row[]>(1);
        subject.next(rows);
        sources.set(name, subject);
        thisLayer[w] = name;
        total++;
      }
    } else {
      let w = 0;
      while (w < width) {
        globalCounter++;
        const parentA = prevLayer[w % prevLayer.length]!;
        const isDiamond = prevLayer.length > 1 && w % diamondEvery === 0;
        const parentB = isDiamond ? prevLayer[(w + 1) % prevLayer.length]! : null;
        const seedBody: RelRef[] = parentB
          ? [relRef(parentA, v("id"), v("val")), relRef(parentB, v("id"), v("val2"))]
          : [relRef(parentA, v("id"), v("val"))];

        if (globalCounter % 32 === 0 && width - w >= 2) {
          const nameEven = `rec_${layer}_${w}_e`;
          const nameOdd = `rec_${layer}_${w}_o`;
          rels.push(derivedRel(nameEven, ["id", "val"]), derivedRel(nameOdd, ["id", "val"]));
          rules.push(
            { head: nameEven, headTerms: [headVar("id"), headVar("val")], body: seedBody },
            { head: nameOdd, headTerms: [headVar("id"), headVar("val")], body: [relRef(nameEven, v("id"), v("val"))] },
            { head: nameEven, headTerms: [headVar("id"), headVar("val")], body: [relRef(nameOdd, v("id"), v("val"))] },
          );
          thisLayer[w] = nameEven;
          if (w + 1 < width) thisLayer[w + 1] = nameOdd;
          total += 2;
          w += 2;
        } else {
          const name = `d_${layer}_${w}`;
          rels.push(derivedRel(name, ["id", "val"]));
          rules.push({ head: name, headTerms: [headVar("id"), headVar("val")], body: seedBody });
          thisLayer[w] = name;
          total++;
          w++;
        }
      }
    }
    prevLayer = thisLayer;
  }

  return { prog: { rels, rules }, sources };
}

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic churn sequence — shared by both backends so a single cfg reproduces the
// exact same mutation stream regardless of which backend runs (or run order).
// ─────────────────────────────────────────────────────────────────────────────

interface ChurnEvent {
  readonly leafIndex: number;
  readonly rowId: number;
  readonly newVal: number;
}

function synthChurnEvents(cfg: GunConfig, leafCount: number): ChurnEvent[] {
  const rng = mulberry32((cfg.seed + 0x9e3779b9) >>> 0);
  const total = cfg.churnTicks * cfg.churnRowsPerTick;
  const events: ChurnEvent[] = [];
  for (let i = 0; i < total; i++) {
    events.push({
      leafIndex: Math.floor(rng() * leafCount),
      rowId: Math.floor(rng() * cfg.rowsPerRel),
      newVal: Math.floor(rng() * 1_000_000),
    });
  }
  return events;
}

// ═════════════════════════════════════════════════════════════════════════════
// Report aggregation — shared by both backends.
// ═════════════════════════════════════════════════════════════════════════════

function percentile(values: readonly number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor(p * sorted.length));
  return sorted[idx]!;
}

/** OLS slope of RSS(MiB) vs tick index, scaled to MiB per 100 ticks. */
function rssSlopePer100Ticks(rssMib: readonly number[]): number {
  const n = rssMib.length;
  if (n < 2) return 0;
  const xMean = (n - 1) / 2;
  const yMean = rssMib.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  for (let i = 0; i < n; i++) {
    const dx = i - xMean;
    num += dx * (rssMib[i]! - yMean);
    den += dx * dx;
  }
  const slopePerTick = den === 0 ? 0 : num / den;
  return slopePerTick * 100;
}

function aggregateReport(
  tickMsList: readonly number[],
  wakeList: readonly number[],
  rssMibList: readonly number[],
  stmtsPerTick: number,
): GunReport {
  return {
    peakRssMib: rssMibList.length ? Math.max(...rssMibList) : 0,
    rssSlope: rssSlopePer100Ticks(rssMibList),
    wakeMedian: percentile(wakeList, 0.5),
    wakeP95: percentile(wakeList, 0.95),
    stmtsPerTick,
    msPerTickP50: percentile(tickMsList, 0.5),
    msPerTickP95: percentile(tickMsList, 0.95),
  };
}

/** Canonical cross-backend content digest: sorted rel names, sorted+JSON-stringified rows
 *  per rel, sha256 of the concatenation. "byte-equal" (the golden test's wording) means this
 *  hex string matches, independent of row-insertion order (set semantics). */
function sinkDigestOf(finalRows: ReadonlyMap<string, readonly Row[]>): string {
  const names = [...finalRows.keys()].sort();
  const parts: string[] = [];
  for (const name of names) {
    const rows = finalRows.get(name)!.map((r) => JSON.stringify(r)).sort();
    parts.push(`${name}:${rows.join("|")}`);
  }
  return createHash("sha256").update(parts.join("\n")).digest("hex");
}

// ═════════════════════════════════════════════════════════════════════════════
// 1.2 — rx backend: lowerProgram over injected ReplaySubject sources.
// ═════════════════════════════════════════════════════════════════════════════

function currentRowsOf(obs: Observable<Row[]>): Row[] {
  let latest: Row[] = [];
  const sub = obs.subscribe((rows) => {
    latest = rows;
  });
  sub.unsubscribe();
  return latest;
}

async function runRxBackend(cfg: GunConfig): Promise<GunRunDetail> {
  const { prog, sources } = synthProgram(cfg);
  const lowered = lowerProgram(prog, sources);
  if (lowered.deferred.length > 0) {
    const names = lowered.deferred.map((d: RecursiveStratumDeferred) => d.message).join("; ");
    throw new Error(`stress rx backend: unexpected deferred recursive strata: ${names}`);
  }

  const leafNames = [...sources.keys()];
  // Authoritative per-leaf row mirror (mutated in place; the source of every subject.next()).
  const leafRowsLive = new Map<string, Row[]>();
  for (const name of leafNames) {
    leafRowsLive.set(name, currentRowsOf(sources.get(name)!).map((r) => [...r] as Row));
  }

  // Hold ONE persistent subscription per rel for the whole run: cold pipes re-run on every
  // upstream emission they are subscribed to, so this is the "subscribe once, let updates
  // flow" shape (the same shape a served program would use).
  const emitCounts = new Map<string, number>();
  const subs: Subscription[] = [];
  for (const [name, obs] of lowered.rels) {
    emitCounts.set(name, 0);
    subs.push(obs.subscribe(() => emitCounts.set(name, (emitCounts.get(name) ?? 0) + 1)));
  }

  const churnEvents = synthChurnEvents(cfg, leafNames.length);
  const tickMsList: number[] = [];
  const wakeList: number[] = [];
  const rssMibList: number[] = [];
  memcap.reset_peak();

  for (let tick = 0; tick < cfg.churnTicks; tick++) {
    const t0 = process.hrtime.bigint();
    const before = new Map(emitCounts);
    const touchedLeaves = new Set<string>();
    for (let i = 0; i < cfg.churnRowsPerTick; i++) {
      const ev = churnEvents[tick * cfg.churnRowsPerTick + i]!;
      const leafName = leafNames[ev.leafIndex]!;
      leafRowsLive.get(leafName)![ev.rowId] = [ev.rowId, ev.newVal];
      touchedLeaves.add(leafName);
    }
    for (const leafName of touchedLeaves) {
      const subject = sources.get(leafName) as ReplaySubject<Row[]>;
      subject.next(leafRowsLive.get(leafName)!.map((r) => [...r] as Row));
    }
    const wakeCount = [...emitCounts.entries()].filter(([name, cnt]) => cnt > (before.get(name) ?? 0)).length;
    wakeList.push(wakeCount);
    tickMsList.push(Number(process.hrtime.bigint() - t0) / 1e6);
    rssMibList.push(memcap.sample() / 1_048_576);
  }

  for (const sub of subs) sub.unsubscribe();

  const finalRows = new Map<string, Row[]>();
  for (const [name, obs] of lowered.rels) finalRows.set(name, currentRowsOf(obs));

  return {
    report: aggregateReport(tickMsList, wakeList, rssMibList, 0),
    sinkDigest: sinkDigestOf(finalRows),
  };
}

// ═════════════════════════════════════════════════════════════════════════════
// 1.3 — sql backend: minted rel tables + a rule->SQL compiler scoped to this synth's rule
// shapes + reconcile mark_changed/propagate for the wake meter.
// ═════════════════════════════════════════════════════════════════════════════

/** Stratify the SAME program rulegraph.ts would give lower.ts; one reconcile id per stratum
 *  (a recursive stratum's members share ONE id — propagate's ascending-id sweep needs an
 *  acyclic rx_dep, so a cyclic rel group cannot own more than one node in it). */
function buildStratumIndex(prog: Program): {
  strata: Stratum[];
  relToId: Map<string, number>;
  rulesByHead: Map<string, Rule[]>;
  relDecls: Map<string, RelDecl>;
  depsOf: number[][];
} {
  const relDecls = new Map<string, RelDecl>();
  for (const decl of prog.rels) relDecls.set(decl.name, decl);
  const rulesByHead = new Map<string, Rule[]>();
  for (const rule of prog.rules) {
    const list = rulesByHead.get(rule.head);
    if (list) list.push(rule);
    else rulesByHead.set(rule.head, [rule]);
  }

  const graph = buildRuleGraph(prog);
  const sccResult = scc(graph);
  const strata = stratify(graph, sccResult);
  const relToId = new Map<string, number>();
  strata.forEach((stratum, id) => {
    for (const relName of stratum.rels) relToId.set(relName, id);
  });

  const depsOf: number[][] = strata.map((stratum) => {
    const members = new Set(stratum.rels);
    const depIds = new Set<number>();
    for (const relName of stratum.rels) {
      for (const rule of rulesByHead.get(relName) ?? []) {
        for (const pred of rule.body) {
          if (pred.kind === "rel" && !members.has(pred.rel)) depIds.add(relToId.get(pred.rel)!);
        }
      }
    }
    return [...depIds].sort((a, b) => a - b);
  });

  return { strata, relToId, rulesByHead, relDecls, depsOf };
}

/** Forward-affected stratum ids (transitively read `id`), per id, ascending (topo order). */
function buildDownstream(strata: readonly Stratum[], depsOf: readonly (readonly number[])[]): number[][] {
  const readers: number[][] = strata.map(() => []);
  depsOf.forEach((deps, id) => {
    for (const dep of deps) readers[dep]!.push(id);
  });
  return strata.map((_, id) => {
    const seen = new Set<number>([id]);
    const queue = [id];
    while (queue.length > 0) {
      const cur = queue.shift()!;
      for (const reader of readers[cur]!) {
        if (!seen.has(reader)) {
          seen.add(reader);
          queue.push(reader);
        }
      }
    }
    seen.delete(id);
    return [...seen].sort((a, b) => a - b);
  });
}

/** Compile one rule to `INSERT OR IGNORE INTO head(...) SELECT ... FROM t0, t1, ... WHERE
 *  <shared-var equalities>`. Scoped to this lab's synth: RelRef body preds (Var args only)
 *  and plain HeadVar terms. Throws on Lit/Compare/aggregation — synth never emits them; the
 *  general rule->SQL compiler is ARC4's job, not this lab's. */
function compileRuleSql(rule: Rule, relDecls: ReadonlyMap<string, RelDecl>): string {
  const headDecl = relDecls.get(rule.head);
  if (!headDecl) throw new Error(`stress sql backend: unknown head rel '${rule.head}'`);

  const bodyRefs: RelRef[] = [];
  for (const pred of rule.body) {
    if (pred.kind !== "rel") {
      throw new Error("stress sql backend: only RelRef body predicates are supported (synth never emits Compare)");
    }
    bodyRefs.push(pred);
  }

  const firstOccurrence = new Map<string, { alias: string; col: string }>();
  const fromParts: string[] = [];
  const whereConds: string[] = [];
  bodyRefs.forEach((ref, i) => {
    const decl = relDecls.get(ref.rel);
    if (!decl) throw new Error(`stress sql backend: unknown body rel '${ref.rel}'`);
    const alias = `t${i}`;
    fromParts.push(`${ref.rel} ${alias}`);
    ref.args.forEach((arg, col) => {
      const colName = decl.columns[col]!;
      if (arg.kind === "lit") {
        const literalSql = typeof arg.value === "string" ? `'${arg.value.replace(/'/g, "''")}'` : String(arg.value);
        whereConds.push(`${alias}.${colName} = ${literalSql}`);
        return;
      }
      if (arg.kind === "wild") {
        throw new Error("stress sql backend: wildcard args not supported (synth never emits them)");
      }
      const seen = firstOccurrence.get(arg.name);
      if (seen) whereConds.push(`${alias}.${colName} = ${seen.alias}.${seen.col}`);
      else firstOccurrence.set(arg.name, { alias, col: colName });
    });
  });

  const headCols = rule.headTerms.map((term) => {
    if (term.kind !== "hvar") {
      throw new Error("stress sql backend: aggregation heads not supported (synth never emits HeadAgg)");
    }
    const binding = firstOccurrence.get(term.name);
    if (!binding) throw new Error(`stress sql backend: unbound head var '${term.name}'`);
    return `${binding.alias}.${binding.col}`;
  });

  const fromSql = fromParts.join(", ");
  const whereSql = whereConds.length ? ` WHERE ${whereConds.join(" AND ")}` : "";
  return `INSERT OR IGNORE INTO ${rule.head}(${headDecl.columns.join(",")}) SELECT ${headCols.join(",")} FROM ${fromSql}${whereSql}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// The DATA plane connection. MEASURED (probes retained in chat_log, not in-tree): driving
// the per-tick rebuild/digest volume (tens of DML statements/tick x churnTicks) through
// `@libsql/client`'s `db.execute(sql)` — parameterized or not, batched via `db.batch()` or
// not, with or without `PRAGMA shrink_memory` — costs ~20 MiB/100 ticks of RSS that never
// comes back; a bare loop of nothing but repeated single-table UPDATEs through that same
// client shows the identical slope, so it is the client's per-call statement handling, not
// this lab's logic. The IDENTICAL workload through better-sqlite3 with a PREPARED,
// CACHED-BY-TEXT statement (labs/fixpoint.ts's own dependency) is flat (~0.07 MiB/100
// calls). The data plane (rel tables: mint/load/rebuild/digest) runs on better-sqlite3;
// the CONTROL plane (rx_memo/rx_dep bookkeeping — mark_changed/propagate/seed, engine.ts)
// stays on the libsql `Db` those functions require. Call volume on the control connection
// is small (bounded by wakeMedian, not by rel count), so it is not the bottleneck this
// backend needs to avoid.
// ─────────────────────────────────────────────────────────────────────────────

type DataDb = InstanceType<typeof Database>;

/** The two Statement operations this lab needs, untied from better-sqlite3's generic
 *  BindParameters (extracting ReturnType off a generic method loses the variadic default
 *  and forces a fixed-arity tuple — a known TS narrowing quirk, not a real API constraint). */
interface PreparedStmt {
  run(...args: readonly unknown[]): { changes: number };
  all(...args: readonly unknown[]): unknown[];
}

/** Prepare-once, cache-by-text: the fix for the RSS growth documented above. */
function prep(db: DataDb, cache: Map<string, PreparedStmt>, sql: string): PreparedStmt {
  let stmt = cache.get(sql);
  if (!stmt) {
    stmt = db.prepare(sql) as unknown as PreparedStmt;
    cache.set(sql, stmt);
  }
  return stmt;
}

function createRelTable(db: DataDb, name: string): void {
  db.exec(`CREATE TABLE IF NOT EXISTS ${name} (id INTEGER NOT NULL, val INTEGER NOT NULL, PRIMARY KEY(id, val)) WITHOUT ROWID`);
}

/** Rebuild one stratum's member table(s) from CURRENT dependency content. EDB leaves are
 *  mutated directly (UPDATE) by the churn loop, never here — an EDB singleton stratum is a
 *  no-op. A non-recursive IDB stratum is DELETE + one INSERT-SELECT per rule (union of
 *  rules, if any — synth never gives a derived rel more than one EXCEPT inside a recursive
 *  pair). A recursive stratum reruns EVERY member rule to a fixpoint (the labs/fixpoint.ts
 *  SQL-fixpoint pattern: loop `INSERT OR IGNORE ... SELECT` until a full pass changes 0
 *  rows), the same bottom-up naive fixpoint lower.ts's `stratumFixpoint` computes in JS. */
function rebuildStratum(
  db: DataDb,
  stmtCache: Map<string, PreparedStmt>,
  stratum: Stratum,
  rulesByHead: ReadonlyMap<string, Rule[]>,
  relDecls: ReadonlyMap<string, RelDecl>,
): void {
  if (!stratum.recursive) {
    const relName = stratum.rels[0]!;
    const decl = relDecls.get(relName)!;
    if (decl.origin === "EDB") return; // mutated directly by the churn loop
    stmt_counter.incr();
    prep(db, stmtCache, `DELETE FROM ${relName}`).run();
    for (const rule of rulesByHead.get(relName) ?? []) {
      stmt_counter.incr();
      prep(db, stmtCache, compileRuleSql(rule, relDecls)).run();
    }
    return;
  }

  for (const relName of stratum.rels) {
    stmt_counter.incr();
    prep(db, stmtCache, `DELETE FROM ${relName}`).run();
  }
  const memberRules = stratum.rels.flatMap((relName) => rulesByHead.get(relName) ?? []);
  let changed = true;
  while (changed) {
    changed = false;
    for (const rule of memberRules) {
      stmt_counter.incr();
      const res = prep(db, stmtCache, compileRuleSql(rule, relDecls)).run();
      if (res.changes > 0) changed = true;
    }
  }
}

/** Fold a rel table's current row set into one order-independent bigint (XOR of `mix` over
 *  each row's polynomial encoding — set semantics, matching `dedupSorted`'s content identity). */
function digestTable(db: DataDb, stmtCache: Map<string, PreparedStmt>, relName: string): bigint {
  stmt_counter.incr();
  const rows = prep(db, stmtCache, `SELECT id,val FROM ${relName}`).all() as { id: number; val: number }[];
  let acc = 0n;
  for (const row of rows) {
    const rowInt = BigInt(row.id) * 1_000_003n + BigInt(row.val);
    acc ^= mix(rowInt);
  }
  return acc;
}

function digestStratum(db: DataDb, stmtCache: Map<string, PreparedStmt>, stratum: Stratum): bigint {
  let acc = 0n;
  for (const relName of stratum.rels) acc ^= digestTable(db, stmtCache, relName);
  return mix(acc);
}

function loadRows(db: DataDb, stmtCache: Map<string, PreparedStmt>, tableName: string, rows: readonly Row[]): void {
  const insertStmt = prep(db, stmtCache, `INSERT INTO ${tableName}(id,val) VALUES (?,?)`);
  const insertAll = db.transaction((rs: readonly Row[]) => {
    for (const r of rs) insertStmt.run(Number(r[0]), Number(r[1]));
  });
  stmt_counter.incr(); // one atomic batched write for the load (N+1 law: one op, not one per row)
  insertAll(rows);
}

async function runSqlBackend(cfg: GunConfig): Promise<GunRunDetail> {
  const { prog, sources } = synthProgram(cfg);
  const { strata, relToId, rulesByHead, relDecls, depsOf } = buildStratumIndex(prog);
  const downstreamOf = buildDownstream(strata, depsOf);

  // Control plane: reconcile-only (NOT `RelStore.attach` — it also stamps the cascade
  // `cx_*` half, unused by this backend).
  const db: Db = createClient({ url: ":memory:", intMode: "bigint" });
  const ns = GraphNs.default();
  await db.executeMultiple(OPEN_PRAGMAS);
  await reconcile.create_schema(db, ns);

  // Data plane: better-sqlite3, prepared+cached statements (see the file-header note above).
  const dataDb: DataDb = new Database(":memory:");
  const stmtCache = new Map<string, PreparedStmt>();
  for (const decl of prog.rels) createRelTable(dataDb, decl.name);

  const leafNames = [...sources.keys()];
  for (const leafName of leafNames) {
    loadRows(dataDb, stmtCache, leafName, currentRowsOf(sources.get(leafName)!));
  }

  // Initial full build (topo order), then seed reconcile memo for every stratum at rev 0.
  for (const stratum of strata) rebuildStratum(dataDb, stmtCache, stratum, rulesByHead, relDecls);
  for (let id = 0; id < strata.length; id++) {
    const digest = digestStratum(dataDb, stmtCache, strata[id]!);
    await reconcile.seed(db, ns, id, digest, depsOf[id]!, 0);
  }

  const churnEvents = synthChurnEvents(cfg, leafNames.length);
  const tickMsList: number[] = [];
  const wakeList: number[] = [];
  const rssMibList: number[] = [];
  const stmtDeltas: number[] = [];
  memcap.reset_peak();
  let lastStmtCount = stmt_counter.get();

  for (let tick = 1; tick <= cfg.churnTicks; tick++) {
    const t0 = process.hrtime.bigint();
    const touchedLeafIds = new Set<number>();
    for (let i = 0; i < cfg.churnRowsPerTick; i++) {
      const ev = churnEvents[(tick - 1) * cfg.churnRowsPerTick + i]!;
      const leafName = leafNames[ev.leafIndex]!;
      stmt_counter.incr();
      prep(dataDb, stmtCache, `UPDATE ${leafName} SET val=? WHERE id=?`).run(ev.newVal, ev.rowId);
      touchedLeafIds.add(relToId.get(leafName)!);
    }

    const affected = new Set<number>(touchedLeafIds);
    for (const leafId of touchedLeafIds) for (const d of downstreamOf[leafId]!) affected.add(d);
    const orderedAffected = [...affected].sort((a, b) => a - b); // ascending id = topo order

    const latestDigest = new Map<number, bigint>();
    for (const id of orderedAffected) {
      const stratum = strata[id]!;
      rebuildStratum(dataDb, stmtCache, stratum, rulesByHead, relDecls);
      latestDigest.set(id, digestStratum(dataDb, stmtCache, stratum));
    }

    const seeds = [...touchedLeafIds];
    await reconcile.mark_changed(db, ns, seeds, tick);
    const recomputeCount = await reconcile.propagate(db, ns, seeds, tick, (id) => latestDigest.get(id)!);

    tickMsList.push(Number(process.hrtime.bigint() - t0) / 1e6);
    wakeList.push(recomputeCount);
    rssMibList.push(memcap.sample() / 1_048_576);
    const stmtsNow = stmt_counter.get();
    stmtDeltas.push(stmtsNow - lastStmtCount);
    lastStmtCount = stmtsNow;
  }

  const finalRows = new Map<string, Row[]>();
  for (const decl of prog.rels) {
    stmt_counter.incr();
    const rows = prep(dataDb, stmtCache, `SELECT id,val FROM ${decl.name} ORDER BY id,val`).all() as {
      id: number;
      val: number;
    }[];
    finalRows.set(decl.name, rows.map((r) => [r.id, r.val] as Row));
  }
  db.close();
  dataDb.close();

  const stmtsPerTick = stmtDeltas.length > 0 ? stmtDeltas.reduce((a, b) => a + b, 0) / stmtDeltas.length : 0;
  return {
    report: aggregateReport(tickMsList, wakeList, rssMibList, stmtsPerTick),
    sinkDigest: sinkDigestOf(finalRows),
  };
}

// ═════════════════════════════════════════════════════════════════════════════
// Task 1.4 (owner amendment 2026-07-23 PM): retraction is CLOSED, not a ruling in this arc.
// An OPTIONAL timing record only — retract vs retract_scc vs retract_dred_cte on one cyclic
// synth graph — printed by the CLI, and swallowed (non-fatal) on any failure.
// ═════════════════════════════════════════════════════════════════════════════

interface RetractTiming {
  readonly variant: string;
  readonly ms: number;
  readonly stmts: number;
  readonly correct: boolean;
}

export async function runRetractShootout(): Promise<RetractTiming[]> {
  const graph = benchgraph.gen_multi_cyclic(40, 6, 5); // layered DAG + back-edges = real cycles
  const expected = benchgraph.oracle_survivors(graph, graph.seed);
  const results: RetractTiming[] = [];

  const variants: readonly ["retract" | "retract_scc" | "retract_dred_cte"][] = [
    ["retract"],
    ["retract_scc"],
    ["retract_dred_cte"],
  ];
  for (const [variant] of variants) {
    const store = await RelStore.attach(createClient({ url: ":memory:", intMode: "bigint" }));
    await store.add_rows(graph.rows);
    await store.add_deps(graph.edges);
    const before = stmt_counter.get();
    const t0 = process.hrtime.bigint();
    await store[variant]([graph.seed]);
    const ms = Number(process.hrtime.bigint() - t0) / 1e6;
    const stmts = stmt_counter.get() - before;
    const survivors = await store.alive_keys();
    const correct = survivors.length === expected.length && survivors.every((k, i) => k === expected[i]);
    results.push({ variant, ms, stmts, correct });
    store.conn().close();
  }
  return results;
}

// ═════════════════════════════════════════════════════════════════════════════
// Task 1.5 — `pnpm stress`: run both backends at 3 config sizes, print one table each,
// assert digest agreement, then the optional retract timing note.
// ═════════════════════════════════════════════════════════════════════════════

/** The epic golden test's fixed config (plan: seed 0xC0FFEE, {rels:255, strataDepth:8,
 *  diamondWidth:4, churnTicks:100}). rowsPerRel/churnRowsPerTick are not pinned by the plan;
 *  chosen here as the one source of truth shared by the CLI's "large" row and the golden test. */
export const goldenGunConfig: GunConfig = {
  rels: 255,
  strataDepth: 8,
  diamondWidth: 4,
  rowsPerRel: 50,
  churnTicks: 100,
  churnRowsPerTick: 1,
  seed: 0xc0ffee,
};

// churnRowsPerTick=1 across every size: the wake meter measures selectivity from ONE
// changed row at a time (the v5 comparison point is "one change wakes ~3 of 255 rels");
// touching multiple unrelated leaves in the same tick sums independent cones and stops
// being a selectivity measurement.
const CONFIGS: readonly [string, GunConfig][] = [
  ["small", { rels: 32, strataDepth: 4, diamondWidth: 2, rowsPerRel: 20, churnTicks: 30, churnRowsPerTick: 1, seed: 0xc0ffee }],
  ["medium", { rels: 96, strataDepth: 6, diamondWidth: 3, rowsPerRel: 30, churnTicks: 50, churnRowsPerTick: 1, seed: 0xc0ffee }],
  ["large (golden)", goldenGunConfig],
];

function fmtRow(label: string, r: GunReport): string {
  const cols = [
    label.padEnd(8),
    r.peakRssMib.toFixed(2).padStart(10),
    r.rssSlope.toFixed(3).padStart(10),
    r.wakeMedian.toFixed(0).padStart(10),
    r.wakeP95.toFixed(0).padStart(8),
    r.stmtsPerTick.toFixed(1).padStart(12),
    r.msPerTickP50.toFixed(3).padStart(9),
    r.msPerTickP95.toFixed(3).padStart(9),
  ];
  return cols.join(" ");
}

async function printReportTable(label: string, cfg: GunConfig): Promise<void> {
  console.log(
    `\n=== ${label} (rels=${cfg.rels} depth=${cfg.strataDepth} diamond=${cfg.diamondWidth} rowsPerRel=${cfg.rowsPerRel} churnTicks=${cfg.churnTicks} churnRowsPerTick=${cfg.churnRowsPerTick}) ===`,
  );
  const header = [
    "backend".padEnd(8),
    "peakRssMB".padStart(10),
    "rssSlope".padStart(10),
    "wakeMed".padStart(10),
    "wakeP95".padStart(8),
    "stmts/tick".padStart(12),
    "msP50".padStart(9),
    "msP95".padStart(9),
  ].join(" ");
  console.log(`  ${header}`);

  const rx = await runGunDetailed(cfg, "rx");
  console.log(`  ${fmtRow("rx", rx.report)}`);
  const sqlRes = await runGunDetailed(cfg, "sql");
  console.log(`  ${fmtRow("sql", sqlRes.report)}`);

  const agree = rx.sinkDigest === sqlRes.sinkDigest;
  console.log(
    `  digestsAgree=${agree}  rx=${rx.sinkDigest.slice(0, 12)}  sql=${sqlRes.sinkDigest.slice(0, 12)}`,
  );
}

async function printRetractShootout(): Promise<void> {
  try {
    console.log("\n=== retract shootout (optional timing note — task 1.4 amended, not a ruling) ===");
    const results = await runRetractShootout();
    for (const r of results) {
      console.log(
        `  ${r.variant.padEnd(18)} ms=${r.ms.toFixed(2).padStart(8)}  stmts=${String(r.stmts).padStart(6)}  correct=${r.correct}`,
      );
    }
  } catch (err) {
    console.log(`  skipped (non-fatal): ${(err as Error).message}`);
  }
}

async function main(): Promise<void> {
  for (const [label, cfg] of CONFIGS) await printReportTable(label, cfg);
  await printRetractShootout();
}

const isMainModule = process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMainModule) {
  main().catch((err) => {
    console.error(err);
    process.exitCode = 1;
  });
}
