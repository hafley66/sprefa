/**
 * gen/demand_laziness_effect_rows.ts — HAND-CARVED phase-A exemplar for the
 * fixture at v6/prolog/conformance/fixtures/scopes.pl:344. This is what the
 * phase-B prolog emitter must reproduce byte-for-byte from:
 *
 *   prog([ keyed(open_feed/2, [1]) ],
 *        [ (demanded(Target, SessionId) <- open_feed(SessionId, Target)),
 *          (effect_call(Target) <- demanded(Target, _)) ])
 *
 * Program shape: one keyed EDB Set rel (`open_feed`, receives raw
 * arrivals only — no edge rule ever writes it) and two plain level rules,
 * `demanded` then `effect_call`, in that dependency order (no recursion, no
 * aggregation — a single linear pass per tick reaches fixpoint).
 *
 * FINDING (margin note for the emitter spec): `keyed(open_feed/2, [1])` does
 * NOT drive keyed-replace behaviour here. engine.pl's `apply_edge_writes`
 * only consults `keyed(...)` for rows an EDGE RULE writes; raw outside
 * arrivals (`absorb_arrivals`) treat every Set rel — keyed or not — as
 * exact-row membership (`+Row` = add if absent, `-Row` = remove exact
 * match). `open_feed` is never edge-written in this program, so its `keyed`
 * decl is inert for every tick below; the DDL still gives it a
 * (session_id, target) primary key (exact-row dedup), never a
 * session_id-only key. Contrast gen/switch_as_keyed_replace.ts, where the
 * SAME decl on an edge-written rel does drive replace.
 *
 * Imports ONLY from ../runtime/ and rxjs (import-gate law, checked by
 * scripts/check-imports.sh).
 */

import { concatMap, forkJoin, map, type Observable } from "rxjs";

import { multiset_diff } from "../runtime/diff.ts";
import { select_rows } from "../runtime/rows.ts";
import type { IArrivalBatch, IGenProgram, IRow, ISqlSeam, ITickDeltas, SqlStatement } from "../runtime/types.ts";

// ── DDL (run once at boot) ───────────────────────────────────────────────────
// open_feed's PRIMARY KEY covers the whole row, not just the keyed column —
// see the FINDING above.

const DDL: readonly string[] = [
  "CREATE TABLE open_feed (session_id TEXT NOT NULL, target TEXT NOT NULL, PRIMARY KEY (session_id, target))",
  "CREATE TABLE demanded (target TEXT NOT NULL, session_id TEXT NOT NULL, PRIMARY KEY (target, session_id))",
  "CREATE TABLE effect_call (target TEXT NOT NULL, PRIMARY KEY (target))",
];

const REL_COLUMNS: Readonly<Record<string, readonly string[]>> = {
  open_feed: ["session_id", "target"],
  demanded: ["target", "session_id"],
  effect_call: ["target"],
};

const ARRIVAL_TARGETS: readonly string[] = ["open_feed"];

type Snapshot = {
  readonly open_feed: readonly IRow[];
  readonly demanded: readonly IRow[];
  readonly effect_call: readonly IRow[];
};

function read_snapshot(seam: ISqlSeam): Observable<Snapshot> {
  return forkJoin({
    open_feed: select_rows(seam, "SELECT session_id, target FROM open_feed", REL_COLUMNS.open_feed!),
    demanded: select_rows(seam, "SELECT target, session_id FROM demanded", REL_COLUMNS.demanded!),
    effect_call: select_rows(seam, "SELECT target FROM effect_call", REL_COLUMNS.effect_call!),
  });
}

// ── arrivals: plain Set add/remove (rulings.pl q1, engine.pl absorb_arrivals) ─

function apply_arrivals(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<unknown> {
  const statements: SqlStatement[] = arrivals
    .filter((arrival) => arrival.rel === "open_feed")
    .map((arrival): SqlStatement =>
      arrival.sign === "add"
        ? { sql: "INSERT OR IGNORE INTO open_feed (session_id, target) VALUES (?, ?)", args: [...arrival.row] }
        : { sql: "DELETE FROM open_feed WHERE session_id = ? AND target = ?", args: [...arrival.row] },
    );
  return seam.runner.batch(seam.db, statements);
}

// ── level rules, dependency order: demanded then effect_call ─────────────────
// demanded(Target, SessionId) <- open_feed(SessionId, Target)
// effect_call(Target)         <- demanded(Target, _)

function recompute_levels(seam: ISqlSeam): Observable<void> {
  const sql = [
    "DELETE FROM demanded",
    "INSERT INTO demanded (target, session_id) SELECT target, session_id FROM open_feed",
    "DELETE FROM effect_call",
    "INSERT INTO effect_call (target) SELECT DISTINCT target FROM demanded",
  ].join(";\n");
  return seam.runner.executeMultiple(seam.db, sql);
}

function build_deltas(before: Snapshot, after: Snapshot): ITickDeltas {
  const open_feed = multiset_diff(before.open_feed, after.open_feed);
  const demanded = multiset_diff(before.demanded, after.demanded);
  const effect_call = multiset_diff(before.effect_call, after.effect_call);
  return {
    rels: [
      { rel: "open_feed", add: open_feed.add, del: open_feed.del },
      { rel: "demanded", add: demanded.add, del: demanded.del },
      { rel: "effect_call", add: effect_call.add, del: effect_call.del },
    ],
    // No edge rule (`<+`) exists in this program, so nothing ever writes a
    // row mid-tick and level closure runs exactly once (arrivals in, then
    // recompute) — engine.pl's post-write-vs-pre-write level split, the
    // thing that can produce carry, never applies here. carryPending is a
    // real structural invariant of this program, not a per-tick guess.
    carry_pending: false,
  };
}

export const DemandLazinessEffectRows: IGenProgram = {
  name: "demand_laziness_effect_rows",
  internMode: "direct",
  ddl: DDL,
  rel_columns: REL_COLUMNS,
  arrival_targets: ARRIVAL_TARGETS,

  tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {
    return read_snapshot(seam).pipe(
      concatMap((before) => apply_arrivals(seam, arrivals).pipe(map(() => before))),
      concatMap((before) => recompute_levels(seam).pipe(map(() => before))),
      concatMap((before) => read_snapshot(seam).pipe(map((after) => build_deltas(before, after)))),
    );
  },
};
