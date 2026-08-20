/**
 * writeVerbs.ts — the six write verbs of a tick (arrive, stage, read_staged,
 * recount, publish, clear) and the two transient-storage strategies behind
 * them.
 *
 * `per_rel` writes each rel's own `__delta_`/`__frontier_`/`__next_frontier_`
 * tables. `shared` writes one `__frontier`, one `__next_frontier` and one
 * `__support_count`, every row carrying its `relation_id`
 * (plans/2026-08-19-shared-sqlite-frontier.md). The rel's own frontier names
 * survive as TEMP views under `shared`, so every compiled read keeps its text.
 *
 * The strategy is resolved ONCE per program by `write_verbs_for`, off the
 * plan metadata. No statement inside a tick loop asks which mode it is in.
 */

import type {
  IDeltaEvent,
  IFrontierCopy,
  IIncrementalLevelStatement,
  IIncrementalRelationPlan,
  IRow,
  IRowValue,
  IWriteVerbs,
  SqlStatement,
  TickBoundary,
} from "./types.ts";
import { list_at_scalar_seam } from "./boundary.ts";

export const SHARED_FRONTIER_TABLE = "__frontier";
export const SHARED_NEXT_FRONTIER_TABLE = "__next_frontier";
export const SHARED_SUPPORT_TABLE = "__support_count";

export function quote_identifier(identifier: string): string {
  return `"${identifier.replaceAll('"', '""')}"`;
}

export function values_sql(row_count: number, column_count: number): string {
  const row = `(${Array.from({ length: column_count }, () => "?").join(", ")})`;
  return Array.from({ length: row_count }, () => row).join(", ");
}

export function bind_args(values: readonly IRowValue[]): (string | number | bigint | Uint8Array)[] {
  return values.map((value) => {
    if (typeof value === "boolean") return BigInt(value ? 1 : 0);
    if (typeof value === "number") return Number.isSafeInteger(value) ? BigInt(value) : value;
    if (typeof value === "string") return value;
    if (value instanceof Uint8Array) return value;
    throw list_at_scalar_seam("sql_parameter");
  });
}

export function has_bytes(relation: IIncrementalRelationPlan): boolean {
  return relation.column_types?.includes("bytes") === true;
}

/** A bytes column cannot cross json1, so its rows bind directly. */
function direct_arrival_statement(
  relation: IIncrementalRelationPlan,
  sign: 1 | -1,
  rows: readonly IRow[],
): SqlStatement {
  const columns = relation.columns.map(quote_identifier);
  if (sign === -1) {
    return {
      sql: `DELETE FROM ${quote_identifier(relation.table_name)} WHERE (${columns.join(", ")}) IN (${values_sql(rows.length, columns.length)}) RETURNING ${columns.join(", ")}`,
      args: rows.flatMap(bind_args),
    };
  }
  const key_indices = relation.key_indices ?? [];
  const conflict = key_indices.length === 0 ? " OR IGNORE" : key_indices.length > 0
    ? ` ON CONFLICT(${key_indices.map((index) => columns[index]!).join(", ")}) DO ${key_indices.length === columns.length ? "NOTHING" : `UPDATE SET ${columns.filter((_column, index) => !key_indices.includes(index)).map((column) => `${column} = excluded.${column}`).join(", ")}`}`
    : "";
  return {
    sql: `INSERT${conflict.startsWith(" OR") ? conflict : ""} INTO ${quote_identifier(relation.table_name)} (${columns.join(", ")}) VALUES ${values_sql(rows.length, columns.length)}${conflict.startsWith(" ON") ? conflict : ""} RETURNING ${columns.join(", ")}`,
    args: rows.flatMap(bind_args),
  };
}

function direct_stage_statement(
  relation: IIncrementalRelationPlan,
  table_name: string,
  mode: "delta" | "frontier",
  phase: number,
  events: readonly IDeltaEvent[],
): SqlStatement {
  const columns = (mode === "delta" ? ["_sign", "_sequence", ...relation.columns] : ["_phase", "_sequence", ...relation.columns]).map(quote_identifier);
  const rows = events.map((event) => [mode === "delta" ? event.sign : phase, event.sequence, ...event.row]);
  return {
    sql: `INSERT INTO ${quote_identifier(table_name)} (${columns.join(", ")}) VALUES ${values_sql(rows.length, columns.length)}`,
    args: rows.flatMap(bind_args),
  };
}

function boundary_stage_statement(
  relation: IIncrementalRelationPlan,
  events: readonly IDeltaEvent[],
): SqlStatement {
  if (has_bytes(relation)) {
    return direct_stage_statement(relation, relation.delta_table_name, "delta", 0, events);
  }
  const columns = ["_sign", "_sequence", ...relation.columns].map(quote_identifier);
  const value_expressions = columns.map(
    (_column, index) => `json_extract(value, '$[${index}]')`,
  );
  const encoded_events = events.map((event) => [
    event.sign,
    event.sequence,
    ...event.row,
  ]);
  return {
    sql: `INSERT INTO ${quote_identifier(relation.delta_table_name)} (${columns.join(", ")}) SELECT ${value_expressions.join(", ")} FROM json_each(?)`,
    args: [JSON.stringify(encoded_events)],
  };
}

function per_rel_frontier_statement(
  relation: IIncrementalRelationPlan,
  table_name: string,
  phase: number,
  events: readonly IDeltaEvent[],
): SqlStatement {
  if (has_bytes(relation)) {
    return direct_stage_statement(relation, table_name, "frontier", phase, events);
  }
  const columns = ["_phase", "_sequence", ...relation.columns].map(quote_identifier);
  const value_expressions = columns.map(
    (_column, index) => `json_extract(value, '$[${index}]')`,
  );
  const encoded_events = events.map((event) => [phase, event.sequence, ...event.row]);
  return {
    sql: `INSERT INTO ${quote_identifier(table_name)} (${columns.join(", ")}) SELECT ${value_expressions.join(", ")} FROM json_each(?)`,
    args: [JSON.stringify(encoded_events)],
  };
}

/** Resolve each event row to its durable `__id` and write
 *  (relation_id, phase, sequence, row_id), one batched statement per rel. */
function shared_frontier_statement(
  relation: IIncrementalRelationPlan,
  table_name: string,
  phase: number,
  events: readonly IDeltaEvent[],
): SqlStatement {
  const shared_table = table_name === relation.next_frontier_table_name
    ? SHARED_NEXT_FRONTIER_TABLE
    : SHARED_FRONTIER_TABLE;
  const join_terms = relation.columns.map(
    (column, index) =>
      `t.${quote_identifier(column)} IS json_extract(je.value, '$[${index + 1}]')`,
  );
  const on_sql = join_terms.length === 0 ? "1" : join_terms.join(" AND ");
  const encoded_events = events.map((event) => [event.sequence, ...event.row]);
  return {
    sql: `INSERT INTO ${quote_identifier(shared_table)} ("relation_id", "_phase", "_sequence", "row_id") SELECT ?, ?, json_extract(je.value, '$[0]'), t."__id" FROM json_each(?) je JOIN ${quote_identifier(relation.table_name)} t ON ${on_sql}`,
    args: [
      relation.shared_frontier!.relation_id,
      phase,
      JSON.stringify(encoded_events),
    ],
  };
}

/** arrive and publish read the durable and boundary planes, which no
 *  strategy changes; both objects below take them from here. */
const durable_verbs = {
  arrive(
    relation: IIncrementalRelationPlan,
    sign: 1 | -1,
    rows: readonly IRow[],
  ): SqlStatement {
    if (has_bytes(relation)) return direct_arrival_statement(relation, sign, rows);
    // apply_arrivals rejects a missing statement while it groups, naming the
    // rel and the sign; by here the text is known to exist.
    const sql = (sign === 1 ? relation.arrival_add_sql : relation.arrival_del_sql)!;
    return { sql, args: [JSON.stringify(rows)] };
  },
  publish(relation: IIncrementalRelationPlan): SqlStatement {
    return relation.boundary_sql;
  },
};

function staged_statements(
  relation: IIncrementalRelationPlan,
  events: readonly IDeltaEvent[],
  copies: readonly IFrontierCopy[],
  frontier: (
    relation: IIncrementalRelationPlan,
    table_name: string,
    phase: number,
    events: readonly IDeltaEvent[],
  ) => SqlStatement,
): readonly SqlStatement[] {
  const boundary = boundary_stage_statement(relation, events);
  const additions = events.filter((event) => event.sign === 1);
  if (additions.length === 0) return [boundary];
  return [
    boundary,
    ...copies.map((copy) => frontier(relation, copy.table_name(relation), copy.phase, additions)),
  ];
}

/** A staged departure is carry the way a staged addition is: engine.pl
 *  appends DepartureCarry to ArrivalCarry in one CarryOut list, and a
 *  non-empty CarryOut is what mints the drain tick. */
function departure_terms(relation: IIncrementalRelationPlan): readonly string[] {
  if (relation.departure_frontier_table_name === undefined) return [];
  return [
    `EXISTS (SELECT 1 FROM ${quote_identifier(relation.departure_frontier_table_name)} LIMIT 1)`,
  ];
}

/** Empty text when there is nothing staged to ask about; the caller then
 *  spends no statement. */
function carry_probe(terms: readonly string[]): string {
  if (terms.length === 0) return "";
  return `SELECT CASE WHEN ${terms.join(" OR ")} THEN 1 ELSE 0 END AS carry_pending`;
}

export const PerRelWriteVerbs: IWriteVerbs = {
  strategy: "per_rel",
  arrive: durable_verbs.arrive,
  publish: durable_verbs.publish,
  stage(relation, events, copies) {
    return staged_statements(relation, events, copies, per_rel_frontier_statement);
  },
  read_staged(relations) {
    return carry_probe(
      relations.flatMap((relation) => [
        `EXISTS (SELECT 1 FROM ${quote_identifier(relation.next_frontier_table_name)} LIMIT 1)`,
        ...departure_terms(relation),
      ]),
    );
  },
  recount(_statement: IIncrementalLevelStatement): readonly string[] {
    return [];
  },
  clear(relations, boundary: TickBoundary) {
    if (boundary === "prepare") {
      return relations.flatMap((relation) => [
        `DELETE FROM ${quote_identifier(relation.delta_table_name)}`,
        `DELETE FROM ${quote_identifier(relation.next_frontier_table_name)}`,
      ]);
    }
    if (boundary === "merge") {
      return relations.map((relation) => {
        const columns = ["_phase", "_sequence", ...relation.columns]
          .map(quote_identifier)
          .join(", ");
        return `INSERT INTO ${quote_identifier(relation.frontier_table_name)} (${columns}) SELECT ${columns} FROM ${quote_identifier(relation.next_frontier_table_name)}`;
      });
    }
    return relations.flatMap((relation) => {
      const columns = ["_phase", "_sequence", ...relation.columns]
        .map(quote_identifier)
        .join(", ");
      return [
        `DELETE FROM ${quote_identifier(relation.frontier_table_name)}`,
        `INSERT INTO ${quote_identifier(relation.frontier_table_name)} (${columns}) SELECT ${columns} FROM ${quote_identifier(relation.next_frontier_table_name)}`,
        `DELETE FROM ${quote_identifier(relation.next_frontier_table_name)}`,
      ];
    });
  },
};

const SHARED_FRONTIER_COLUMNS = '"relation_id", "_phase", "_sequence", "row_id"';

export const SharedWriteVerbs: IWriteVerbs = {
  strategy: "shared",
  arrive: durable_verbs.arrive,
  publish: durable_verbs.publish,
  stage(relation, events, copies) {
    return staged_statements(relation, events, copies, shared_frontier_statement);
  },
  read_staged(relations) {
    return carry_probe([
      `EXISTS (SELECT 1 FROM ${quote_identifier(SHARED_NEXT_FRONTIER_TABLE)} LIMIT 1)`,
      ...relations.flatMap(departure_terms),
    ]);
  },
  recount(statement: IIncrementalLevelStatement): readonly string[] {
    const plan = statement.support_count_sql;
    if (plan === undefined || plan === null) return [];
    return [plan.clear_sql, ...plan.write_sqls];
  },
  clear(relations, boundary: TickBoundary) {
    if (boundary === "prepare") {
      return [
        ...relations.map((relation) => `DELETE FROM ${quote_identifier(relation.delta_table_name)}`),
        `DELETE FROM ${quote_identifier(SHARED_NEXT_FRONTIER_TABLE)}`,
      ];
    }
    const copy = `INSERT INTO ${quote_identifier(SHARED_FRONTIER_TABLE)} (${SHARED_FRONTIER_COLUMNS}) SELECT ${SHARED_FRONTIER_COLUMNS} FROM ${quote_identifier(SHARED_NEXT_FRONTIER_TABLE)}`;
    if (boundary === "merge") return [copy];
    return [
      `DELETE FROM ${quote_identifier(SHARED_FRONTIER_TABLE)}`,
      copy,
      `DELETE FROM ${quote_identifier(SHARED_NEXT_FRONTIER_TABLE)}`,
    ];
  },
};

/** One resolution per program: the relations array a program hands the
 *  runtime is the same object every tick, so the strategy is decided at load
 *  and read back from here. */
const resolved_verbs = new WeakMap<readonly IIncrementalRelationPlan[], IWriteVerbs>();

export function write_verbs_for(
  relations: readonly IIncrementalRelationPlan[],
): IWriteVerbs {
  const known = resolved_verbs.get(relations);
  if (known !== undefined) return known;
  const verbs = relations.some((relation) => relation.shared_frontier !== undefined)
    ? SharedWriteVerbs
    : PerRelWriteVerbs;
  resolved_verbs.set(relations, verbs);
  return verbs;
}
