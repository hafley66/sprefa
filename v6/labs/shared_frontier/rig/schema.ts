/**
 * The two arms' DDL.
 *
 * Arm A statement text is copied from `v6/prolog/7_lower/lower.pl`:
 *   durable set rel   lower.pl:996-998   set_rel_table_ddl/5
 *   __frontier_<rel>  lower.pl:6347-6349 delta_ddl/3
 *   its _phase index  lower.pl:6352-6354 delta_ddl/3
 *   __support_next_   lower.pl:6428-6430 ref_count_head_ddl/4
 * Column types are `int`, so every column def is `"<name>" INTEGER NOT NULL`
 * (lower.pl:2860 column_def/4).
 *
 * Arm B statement text is copied from the Storage section of
 * `plans/2026-08-19-shared-sqlite-frontier.md`.
 *
 * Both arms carry the SAME durable tables and the SAME payload: the frontier
 * row references its durable row by integer id, so the read in each arm is the
 * same join answering the same question.
 */

/** Durable typed table for relation `index`, surrogate `__id` plus 3 int columns. */
export function durableDdl(index: number): string {
  return (
    `CREATE TABLE "rel_${index}" ("__id" INTEGER PRIMARY KEY,` +
    ` "row_key" INTEGER NOT NULL, "value_a" INTEGER NOT NULL, "value_b" INTEGER NOT NULL,` +
    ` UNIQUE ("row_key"))`
  );
}

/** Arm A: one frontier table, one _phase index, one support table, per relation. */
export function armATransientDdl(index: number): readonly string[] {
  return [
    `CREATE TEMP TABLE "__frontier_rel_${index}" ("_phase" INTEGER NOT NULL, "_sequence" INTEGER NOT NULL, "row_id" INTEGER NOT NULL)`,
    `CREATE INDEX "__frontier_rel_${index}_phase" ON "__frontier_rel_${index}" ("_phase")`,
    `CREATE TEMP TABLE "__support_next_rel_${index}" ("row_id" INTEGER NOT NULL, "rule_id" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL, PRIMARY KEY ("row_id", "rule_id")) WITHOUT ROWID`,
  ];
}

export type Arm = "A" | "B" | "B'";

/** Arm B and B' differ only in the frontier PRIMARY KEY column order; two tables total, whatever N is. */
export function armBTransientDdl(arm: Arm): readonly string[] {
  const key = arm === "B" ? `"relation_id", "row_id", "tick", "sign"` : `"relation_id", "tick", "row_id", "sign"`;
  return [
    `CREATE TEMP TABLE "frontier" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "tick" INTEGER NOT NULL, "sign" INTEGER NOT NULL CHECK ("sign" IN (-1, 1)), PRIMARY KEY (${key}))`,
    `CREATE TEMP TABLE "support_count" ("relation_id" INTEGER NOT NULL, "row_id" INTEGER NOT NULL, "rule_id" INTEGER NOT NULL, "count" INTEGER NOT NULL, PRIMARY KEY ("relation_id", "row_id", "rule_id"))`,
  ];
}

export const ARM_B_TRANSIENT_DDL: readonly string[] = armBTransientDdl("B");

/** Every CREATE the arm issues for N relations, durable tables first. */
export function bootDdl(arm: Arm, relations: number): readonly string[] {
  const statements: string[] = [];
  for (let index = 0; index < relations; index += 1) statements.push(durableDdl(index));
  if (arm === "A") {
    for (let index = 0; index < relations; index += 1) statements.push(...armATransientDdl(index));
  } else {
    statements.push(...armBTransientDdl(arm));
  }
  return statements;
}

export function durableInsertSql(index: number): string {
  return `INSERT INTO "rel_${index}" ("row_key", "value_a", "value_b") VALUES (?, ?, ?)`;
}

export function frontierInsertSql(arm: Arm, index: number): string {
  return arm === "A"
    ? `INSERT INTO "__frontier_rel_${index}" ("_phase", "_sequence", "row_id") VALUES (?, ?, ?)`
    : `INSERT INTO "frontier" ("relation_id", "row_id", "tick", "sign") VALUES (?, ?, ?, 1)`;
}

/** The read the brief prescribes, one per touched relation. */
export function frontierReadSql(arm: Arm, index: number): string {
  const projection = `typed."__id", typed."row_key", typed."value_a", typed."value_b"`;
  return arm === "A"
    ? `SELECT ${projection} FROM "__frontier_rel_${index}" f JOIN "rel_${index}" typed ON typed."__id" = f."row_id" WHERE f."_phase" = ?`
    : `SELECT ${projection} FROM "frontier" f JOIN "rel_${index}" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?`;
}

/** Arm A must clear one table per touched relation; arm B clears the tick once. */
export function frontierDeleteSql(arm: Arm, index: number): string {
  return arm === "A"
    ? `DELETE FROM "__frontier_rel_${index}" WHERE "_phase" = ?`
    : `DELETE FROM "frontier" WHERE "tick" = ?`;
}

/** Which relations tick `tick` touches: a rotating contiguous window of width k. */
export function touched(relations: number, k: number, tick: number): number[] {
  const offset = (tick * k) % relations;
  const indices: number[] = [];
  for (let step = 0; step < k; step += 1) indices.push((offset + step) % relations);
  return indices;
}
