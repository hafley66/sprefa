/**
 * serveStats.ts -- the served engine's self-diagnosis surface for storage +
 * process memory (NEW DIRECTIVE 2026-07-29 late: memory-soak + sqlite
 * stats). Read src/db.rs and src/cli/health.rs FIRST, per the standing law,
 * before writing a line of this file:
 *
 *   - src/db.rs `Db::rel_stats` (~line 1216): row count + `PRAGMA
 *     table_info` for columns/pk, plus one `SELECT sum(pgsize) FROM dbstat
 *     WHERE name = ?1` per table/index object.
 *   - src/cli/health.rs `report_db` (~line 120-153): `PRAGMA page_count` /
 *     `page_size` / `freelist_count` for the file-level numbers, plus ONE
 *     grouped dbstat pass (`SELECT name, sum(pgsize) FROM dbstat GROUP BY
 *     name`) joined to `sqlite_master`, bucketed by object kind.
 *
 * This module mirrors those CONCEPTS -- PRAGMA file stats + dbstat page
 * bytes -- not new categories: grepped `src/db.rs` for `sqlite3_status`,
 * `memory_used`, `highwater`; none exist. PRAGMA + one dbstat pass is the
 * whole of what v5 exposes, so it is the whole of what this file adds.
 * `@libsql/client` has no memory-highwater API either (the P3 arc's
 * PERF-REPORT.md finding, sqlite_hw_mb = N/A-with-reason) -- `processMemory`
 * below is `process.memoryUsage()`, the one number this driver CAN give.
 *
 * DBSTAT AVAILABILITY, VERIFIED NOT ASSUMED: probed empirically against
 * @libsql/client 0.17.4 in this worktree (`SELECT name, sum(pgsize) FROM
 * dbstat GROUP BY name` against a live `:memory:` connection returned real
 * per-object byte counts; every PRAGMA used here returned real scalars too
 * -- memory-soak arc report carries the receipt). `dbstatAvailable` still
 * guards every call site rather than assuming a future driver build keeps
 * the vtab compiled in.
 *
 * ONE dbstat statement, not one per table (the N+1 law applies to stats
 * reads too): `WHERE name IN (SELECT value FROM json_each(?))` against a
 * bound JSON array, the same dynamic-list idiom `runtime/1_incremental.ts`
 * already uses for arrival batches. The four reads (three PRAGMAs + the
 * dbstat pass) run concurrently over the one connection via `forkJoin`, the
 * same pattern `runtime/1_incremental.ts`'s `readBoundary` already uses for
 * concurrent per-relation reads -- not a new concurrency shape.
 */

import { type Observable, catchError, forkJoin, map, of } from "rxjs";

import type {
  IProcessMemorySnapshot,
  IServeStats,
  ISqlSeam,
  ISqliteObjectBytes,
  ISqliteStatsSnapshot,
} from "./types.ts";

function pragmaScalar(seam: ISqlSeam, name: string): Observable<number> {
  return seam.runner.scalar(seam.db, `PRAGMA ${name}`);
}

interface DbstatRead {
  readonly available: boolean;
  readonly objects: readonly ISqliteObjectBytes[];
}

/** Empty `tableNames` is answered without a round trip: `dbstat` is treated
 *  as available (nothing was asked of it, so nothing failed) with zero
 *  objects, never a guess either way. */
function objectBytes(seam: ISqlSeam, tableNames: readonly string[]): Observable<DbstatRead> {
  if (tableNames.length === 0) return of({ available: true, objects: [] });
  return seam.runner
    .execute(seam.db, {
      sql:
        "SELECT s.name AS name, s.bytes AS bytes FROM " +
        "(SELECT name, sum(pgsize) AS bytes FROM dbstat GROUP BY name) s " +
        "WHERE s.name IN (SELECT value FROM json_each(?))",
      args: [JSON.stringify(tableNames)],
    })
    .pipe(
      map(
        (result): DbstatRead => ({
          available: true,
          objects: result.rows.map(
            (row): ISqliteObjectBytes => ({ name: row["name"] as string, bytes: Number(row["bytes"]) }),
          ),
        }),
      ),
      // `dbstat` absent (or any other reason this one statement fails) is a
      // finding, not a fatal read: the PRAGMA numbers alongside it still
      // answer the caller (the fallback the arc's brief names).
      catchError(() => of<DbstatRead>({ available: false, objects: [] })),
    );
}

export const ServeStats: IServeStats = {
  sqliteSnapshot(seam: ISqlSeam, tableNames: readonly string[]): Observable<ISqliteStatsSnapshot> {
    return forkJoin({
      pageCount: pragmaScalar(seam, "page_count"),
      pageSize: pragmaScalar(seam, "page_size"),
      freelistCount: pragmaScalar(seam, "freelist_count"),
      dbstat: objectBytes(seam, tableNames),
    }).pipe(
      map(
        ({ pageCount, pageSize, freelistCount, dbstat }): ISqliteStatsSnapshot => ({
          pageCount,
          pageSize,
          freelistCount,
          dbBytes: pageCount * pageSize,
          freelistBytes: freelistCount * pageSize,
          dbstatAvailable: dbstat.available,
          objectBytes: dbstat.objects,
        }),
      ),
    );
  },

  processMemory(): IProcessMemorySnapshot {
    const usage = process.memoryUsage();
    return { rssBytes: usage.rss, heapUsedBytes: usage.heapUsed, externalBytes: usage.external };
  },
};
