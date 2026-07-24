/**
 * algo.ts — the parity surface as a literal trait, ported from src/algo.rs.
 *
 * Reachability and structure over a directed graph (node keys are dense i64). The SQLite
 * engine: every method is the on-disk `cx_dep` formulation in `engine.reach`. Borrows the
 * connection and the store's GraphNs; state is on disk, not in self.
 */

import { reach } from "./engine.ts";
import type { GraphNs, SqliteDb } from "./lib.ts";

/**
 * SQLite reach engine over `cx_dep` (the on-disk covering set). The `Reach` trait interface
 * lives in tasks.ts (the parity ledger); SqliteReach satisfies it structurally. Its methods
 * are the on-disk `cx_dep` formulations in `engine.reach`, borrowing the connection + ns.
 */
export class SqliteReach {
  constructor(private readonly db: SqliteDb, private readonly ns: GraphNs) {}

  async reaches_from(start: number): Promise<number[]> {
    return reach.reaches_from(this.db, this.ns, start);
  }
  async reached_by(target: number): Promise<number[]> {
    return reach.reached_by(this.db, this.ns, target);
  }
  async scc_labels(): Promise<[number, number][]> {
    return reach.scc_labels(this.db, this.ns);
  }
  async count_pairs(): Promise<bigint> {
    return reach.count_pairs(this.db, this.ns);
  }
}
