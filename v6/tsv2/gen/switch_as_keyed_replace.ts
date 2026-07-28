/**
 * gen/switch_as_keyed_replace.ts — HAND-CARVED phase-A exemplar for the
 * fixture at v6/prolog/conformance/fixtures/scopes.pl:31. This is what the
 * phase-B prolog emitter must reproduce byte-for-byte from:
 *
 *   prog([ kind(route_change/2, log), keep(route_change/2, all),
 *          kind(route_row/2, set),
 *          keyed(open_scope/2, [1]) ],
 *        [ (open_scope(SessionId, route_data(RouteId)) <+
 *             only(route_change(SessionId, RouteId))),
 *          (demanded(Target, SessionId) <- open_scope(SessionId, Target)),
 *          (route_view(RouteId, Body) <-
 *             demanded(route_data(RouteId), _), route_row(RouteId, Body)) ]),
 *   [ route_row(settings, body_settings), route_row(profile, body_profile) ]
 *
 * Program shape: one Log EDB rel (`route_change`, append-only arrivals), one
 * static seeded Set rel (`route_row`, never written after boot), one edge
 * rule with a keyed HEAD (`open_scope`, replace-on-key), and two plain level
 * rules (`demanded`, `route_view`) — `route_view` reads `demanded`'s target
 * column through the compound pattern `route_data(RouteId)`.
 *
 * FINDINGS (margin notes for the emitter spec):
 *
 * 1. Compound-term pattern matching lowers to SQL substring slicing. Every
 *    value this program stores collapses to TEXT once it reaches SQLite —
 *    `open_scope.target` holds the literal text `route_data(settings)`,
 *    produced by string-concatenating the edge rule's head. Reading the
 *    pattern `demanded(route_data(RouteId), _)` back out is therefore a
 *    `LIKE 'route_data(%)'` guard plus a `substr` extraction, with the
 *    functor's own length (`route_data(` = 11 characters) baked in as a
 *    compile-time literal offset — see ROUTE_DATA_PREFIX_LEN below. A
 *    functor with nested compound arguments would need a real parser here;
 *    this fixture's arity-1 atomic-argument compounds do not.
 *
 * 2. `keyed(open_scope/2, [1])` DOES drive replace here (contrast
 *    gen/demand_laziness_effect_rows.ts's open_feed, where the same kind of
 *    decl on a rel that only ever receives raw arrivals is inert): because
 *    `open_scope` is the HEAD of an edge rule, every write goes through
 *    engine.pl's `apply_edge_writes`, which looks up the existing row by key
 *    position, no-ops on an identical replacement, and replaces otherwise.
 *
 * 3. `carryPending` (this file's stand-in for engine.pl's CarryCandidates /
 *    q4 next-tick trigger carry) is computed here as "did this tick's edge
 *    rule actually change a row" (`writtenRows.length > 0`), not the full
 *    engine.pl algorithm (which also carries newly-true POST-write level
 *    rows as separate trigger occurrences, computed via a pre-write vs
 *    post-write level snapshot). The simplification is safe for this
 *    program specifically: `demanded`/`route_view` have no source but
 *    `open_scope`, so a level-rel row can only turn newly-true in the same
 *    tick `open_scope` itself changes — the two conditions are equivalent
 *    here, not in general. A program where a level rule also reads an
 *    arrival-driven rel directly would need the real pre/post split.
 *
 * Imports ONLY from ../runtime/ and rxjs (import-gate law, checked by
 * scripts/check-imports.sh).
 */

import { concatMap, forkJoin, map, type Observable } from "rxjs";

import { multisetDiff } from "../runtime/diff.ts";
import { selectRows } from "../runtime/rows.ts";
import type { IArrivalBatch, IGenProgram, IRow, ISqlSeam, ITickDeltas, SqlStatement } from "../runtime/types.ts";

// ── DDL (run once at boot; seed INSERTs for the fixture's Initial rows ride
// along — Initial is fixed program text, same as the schema) ────────────────

const ROUTE_DATA_PREFIX = "route_data(";
const ROUTE_DATA_PREFIX_LEN = ROUTE_DATA_PREFIX.length; // 11, a compile-time constant of the functor name

const DDL: readonly string[] = [
  "CREATE TABLE route_change (session_id TEXT NOT NULL, route_id TEXT NOT NULL)",
  "CREATE TABLE route_row (route_id TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (route_id))",
  "CREATE TABLE open_scope (session_id TEXT NOT NULL, target TEXT NOT NULL, PRIMARY KEY (session_id))",
  "CREATE TABLE demanded (target TEXT NOT NULL, session_id TEXT NOT NULL, PRIMARY KEY (target, session_id))",
  "CREATE TABLE route_view (route_id TEXT NOT NULL, body TEXT NOT NULL, PRIMARY KEY (route_id, body))",
  "INSERT INTO route_row (route_id, body) VALUES ('settings', 'body_settings')",
  "INSERT INTO route_row (route_id, body) VALUES ('profile', 'body_profile')",
];

const REL_COLUMNS: Readonly<Record<string, readonly string[]>> = {
  route_change: ["session_id", "route_id"],
  route_row: ["route_id", "body"],
  open_scope: ["session_id", "target"],
  demanded: ["target", "session_id"],
  route_view: ["route_id", "body"],
};

const ARRIVAL_TARGETS: readonly string[] = ["route_change"];

type Snapshot = {
  readonly route_change: readonly IRow[];
  readonly route_row: readonly IRow[];
  readonly open_scope: readonly IRow[];
  readonly demanded: readonly IRow[];
  readonly route_view: readonly IRow[];
};

function readSnapshot(seam: ISqlSeam): Observable<Snapshot> {
  return forkJoin({
    route_change: selectRows(seam, "SELECT session_id, route_id FROM route_change", REL_COLUMNS.route_change!),
    route_row: selectRows(seam, "SELECT route_id, body FROM route_row", REL_COLUMNS.route_row!),
    open_scope: selectRows(seam, "SELECT session_id, target FROM open_scope", REL_COLUMNS.open_scope!),
    demanded: selectRows(seam, "SELECT target, session_id FROM demanded", REL_COLUMNS.demanded!),
    route_view: selectRows(seam, "SELECT route_id, body FROM route_view", REL_COLUMNS.route_view!),
  });
}

// ── route_change arrivals: Log append only (rulings.pl: -Row into a Log rel
// throws; not exercised by this fixture, so a defensive throw is enough) ────

function applyArrivals(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<unknown> {
  const statements: SqlStatement[] = arrivals
    .filter((arrival) => arrival.rel === "route_change")
    .map((arrival): SqlStatement => {
      if (arrival.sign === "del") throw new Error("retract_from_log(route_change/2)");
      return { sql: "INSERT INTO route_change (session_id, route_id) VALUES (?, ?)", args: [...arrival.row] };
    });
  return seam.runner.batch(seam.db, statements);
}

// ── the edge rule: open_scope(SessionId, route_data(RouteId)) <+
//    only(route_change(SessionId, RouteId)). Trigger set = this tick's fresh
//    route_change arrivals, in arrival order; keyed replace on session_id,
//    later write for the same key wins (engine.pl apply_edge_writes). ───────

function scopeWrites(
  arrivals: IArrivalBatch,
  beforeOpenScope: readonly IRow[],
): { statements: readonly SqlStatement[]; writtenRows: readonly IRow[] } {
  const beforeBySession = new Map(beforeOpenScope.map((row): [string, IRow] => [row[0] as string, row]));
  const candidateBySession = new Map<string, IRow>();
  for (const arrival of arrivals) {
    if (arrival.rel !== "route_change" || arrival.sign !== "add") continue;
    const sessionId = arrival.row[0] as string;
    const routeId = arrival.row[1] as string;
    const target = `${ROUTE_DATA_PREFIX}${routeId})`;
    candidateBySession.set(sessionId, [sessionId, target]);
  }

  const statements: SqlStatement[] = [];
  const writtenRows: IRow[] = [];
  for (const [sessionId, candidateRow] of candidateBySession) {
    const existing = beforeBySession.get(sessionId);
    if (existing !== undefined && existing[1] === candidateRow[1]) continue; // equal-row write = no-op
    statements.push({
      sql: "INSERT INTO open_scope (session_id, target) VALUES (?, ?) ON CONFLICT(session_id) DO UPDATE SET target = excluded.target",
      args: [...candidateRow],
    });
    writtenRows.push(candidateRow);
  }
  return { statements, writtenRows };
}

// ── level rules, dependency order: demanded then route_view ──────────────────
// demanded(Target, SessionId)  <- open_scope(SessionId, Target)
// route_view(RouteId, Body)    <- demanded(route_data(RouteId), _), route_row(RouteId, Body)

function recomputeLevels(seam: ISqlSeam): Observable<void> {
  const sql = [
    "DELETE FROM demanded",
    "INSERT INTO demanded (target, session_id) SELECT target, session_id FROM open_scope",
    "DELETE FROM route_view",
    `INSERT INTO route_view (route_id, body)
       SELECT substr(demanded.target, ${ROUTE_DATA_PREFIX_LEN + 1}, length(demanded.target) - ${ROUTE_DATA_PREFIX_LEN + 1}), route_row.body
       FROM demanded
       JOIN route_row
         ON route_row.route_id = substr(demanded.target, ${ROUTE_DATA_PREFIX_LEN + 1}, length(demanded.target) - ${ROUTE_DATA_PREFIX_LEN + 1})
       WHERE demanded.target LIKE '${ROUTE_DATA_PREFIX}%)' AND substr(demanded.target, -1) = ')'`,
  ].join(";\n");
  return seam.runner.executeMultiple(seam.db, sql);
}

function buildDeltas(before: Snapshot, after: Snapshot, writtenRows: readonly IRow[]): ITickDeltas {
  const routeChange = multisetDiff(before.route_change, after.route_change);
  const routeRow = multisetDiff(before.route_row, after.route_row);
  const openScope = multisetDiff(before.open_scope, after.open_scope);
  const demanded = multisetDiff(before.demanded, after.demanded);
  const routeView = multisetDiff(before.route_view, after.route_view);
  return {
    rels: [
      { rel: "route_change", add: routeChange.add, del: routeChange.del },
      { rel: "route_row", add: routeRow.add, del: routeRow.del },
      { rel: "open_scope", add: openScope.add, del: openScope.del },
      { rel: "demanded", add: demanded.add, del: demanded.del },
      { rel: "route_view", add: routeView.add, del: routeView.del },
    ],
    // See FINDING 3 above.
    carryPending: writtenRows.length > 0,
  };
}

export const SwitchAsKeyedReplace: IGenProgram = {
  name: "switch_as_keyed_replace",
  ddl: DDL,
  relColumns: REL_COLUMNS,
  arrivalTargets: ARRIVAL_TARGETS,

  tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {
    return readSnapshot(seam).pipe(
      concatMap((before) => applyArrivals(seam, arrivals).pipe(map(() => before))),
      concatMap((before) => {
        const { statements, writtenRows } = scopeWrites(arrivals, before.open_scope);
        return seam.runner.batch(seam.db, statements).pipe(map(() => ({ before, writtenRows })));
      }),
      concatMap(({ before, writtenRows }) => recomputeLevels(seam).pipe(map(() => ({ before, writtenRows })))),
      concatMap(({ before, writtenRows }) =>
        readSnapshot(seam).pipe(map((after) => buildDeltas(before, after, writtenRows))),
      ),
    );
  },
};
