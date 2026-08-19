/**
 * Resolves relation-shaped values arriving in reference columns.
 *
 * The wire carries a complete target row. Before the parent tick runs, this
 * module validates the row shape, recursively resolves referenced children,
 * checks the target relation for a same-key/full-row conflict, inserts missing
 * targets, looks up their `__id`, and replaces each parent wire value with that
 * integer endpoint.
 *
 * Each target relation costs three set-based SQL statements for a non-empty
 * batch: conflict preflight, INSERT OR IGNORE, and key lookup. Statement count
 * is flat in requested row count. The target table stores typed columns plus
 * `__id`; canonical JSON exists only as transient wire and comparison text.
 *
 * Generated ticks pass an ordinary arrival applicator. Target rows therefore
 * enter the same relation clock as authored arrivals before the rewritten
 * parent rows. Direct insertion remains available to boot and focused storage
 * receipts that do not run a tick.
 */

import { concatMap, map, of, type Observable } from "rxjs";

import { TextPlane } from "./textPlane.ts";
import type {
  IArrivalBatch,
  IArrivalRow,
  IRow,
  IRowValue,
  ISqlSeam,
  IStructPlane,
  IStructTypePlan,
  IStructRefColumns,
  ITextInternPlan,
} from "./types.ts";

/** Sorted object keys, no whitespace: the ruled cross-target encoding
 *  (json_ticklog_encoding), and a clause-for-clause match of
 *  0_type_plane.pl:canonical_json_text/2. */
function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(Object.keys(record).sort().map((key) => [key, canonicalize(record[key])]));
  }
  return value;
}

function canonical_text(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/** Host and HTTP arrivals carry JSON through IRowValue's text side. Schedule
 * fixtures may still inject the already-decoded object. Both spellings enter
 * the same shape check and canonicalization path before interning. */
function decoded_struct_value(value: unknown): unknown {
  if (typeof value !== "string") return value;
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return value;
  }
}

/**
 * The runtime half of SLOT-ARRIVAL-MALFORMED. The oracle door checks the same
 * shapes over a fixture's static Initial + Schedule
 * (0_type_plane.pl:world_row_shape_violation/3); here the data is not static,
 * so the check runs per value at intern time and names the same reasons.
 *
 * The oracle canonicalizes object key order at ingress. This door accepts any
 * key order and canonicalizes before target lookup, producing the same row.
 */
function check_shape(plan: IStructTypePlan, by_name: ReadonlyMap<string, IStructTypePlan>, value: unknown): void {
  if (!isObject(value)) {
    throw new Error(`type_arrival_shape_mismatch: not_an_object(${plan.name}, ${JSON.stringify(value)})`);
  }
  for (const column of plan.columns) {
    if (!(column in value)) {
      throw new Error(`type_arrival_shape_mismatch: missing_key(${plan.name}, ${column})`);
    }
  }
  for (const key of Object.keys(value)) {
    if (!plan.columns.includes(key)) {
      throw new Error(`type_arrival_shape_mismatch: unknown_key(${plan.name}, ${key})`);
    }
  }
  plan.columns.forEach((column, index) => {
    const ref_type = plan.refs[index];
    const field = value[column];
    if (ref_type !== null && ref_type !== undefined) {
      const child_plan = by_name.get(ref_type);
      if (child_plan === undefined) {
        throw new Error(`type_arrival_shape_mismatch: column_type_unknown(${ref_type})`);
      }
      check_shape(child_plan, by_name, field);
    }
  });
}

/** `TypeName` ++ canonical JSON. The type name is an identifier and the
 *  rendering always starts with `{`, so the concatenation is unambiguous, and
 *  it matches lower.pl:struct_intern_statements/5 byte for byte. Full
 *  canonical text rather than a digest because @libsql registers no UDF and
 *  SQLite ships no hash (SLOT-SEMANTIC-DIGEST in 0_type_plane.pl's header):
 *  strictly stronger than a hash, with no collision case to reason about. */
function semantic_key(type_name: string, rendered: string): string {
  return `${type_name}${rendered}`;
}

interface ICollected {
  readonly rendered: string;
  /** column values; a ref column holds the CHILD'S semantic key until the
   *  child's own type has been interned and its dense id is known. */
  readonly fields: readonly (IRowValue | { readonly child_semantic: string })[];
}

function collect(
  plan: IStructTypePlan,
  by_name: ReadonlyMap<string, IStructTypePlan>,
  value: unknown,
  per_type: Map<string, Map<string, ICollected>>,
): string {
  const decoded = decoded_struct_value(value);
  check_shape(plan, by_name, decoded);
  const object = decoded as Record<string, unknown>;
  const fields = plan.columns.map((column, index) => {
    const ref_type = plan.refs[index];
    if (ref_type === null || ref_type === undefined) return object[column] as IRowValue;
    // Post-order: the child is collected (and therefore interned) first.
    const child_semantic = collect(by_name.get(ref_type)!, by_name, object[column], per_type);
    return { child_semantic };
  });
  const rendered = canonical_text(object);
  const semantic = semantic_key(plan.name, rendered);
  let bucket = per_type.get(plan.name);
  if (bucket === undefined) {
    bucket = new Map<string, ICollected>();
    per_type.set(plan.name, bucket);
  }
  bucket.set(semantic, { rendered, fields });
  return semantic;
}

function rewrite_row(
  row: IRow,
  refs: readonly (string | null)[],
  by_name: ReadonlyMap<string, IStructTypePlan>,
  ids: ReadonlyMap<string, number>,
): IRow {
  return row.map((value, index) => {
    const ref_type = refs[index];
    if (ref_type === null || ref_type === undefined) return value;
    const rendered = canonical_text(decoded_struct_value(value));
    const id = ids.get(semantic_key(ref_type, rendered));
    if (id === undefined) {
      throw new Error(`relation reference normalization lost the id for ${ref_type} value ${rendered}`);
    }
    return id;
  });
}

export const StructPlane: IStructPlane = {
  canonical_text,

  intern(
    seam: ISqlSeam,
    types: readonly IStructTypePlan[],
    ref_columns: IStructRefColumns,
    arrivals: IArrivalBatch,
    apply_targets?: (arrivals: IArrivalBatch) => Observable<unknown>,
    text_plan?: ITextInternPlan,
  ): Observable<IArrivalBatch> {
    if (types.length === 0 || arrivals.length === 0) return of(arrivals);
    const by_name = new Map(types.map((plan) => [plan.name, plan]));
    const per_type = new Map<string, Map<string, ICollected>>();
    for (const arrival of arrivals) {
      const refs = ref_columns[arrival.rel];
      if (refs === undefined) continue;
      arrival.row.forEach((value, index) => {
        const ref_type = refs[index];
        if (ref_type === null || ref_type === undefined) return;
        collect(by_name.get(ref_type)!, by_name, value, per_type);
      });
    }
    if (per_type.size === 0) return of(arrivals);

    // `types` arrives in topological order (lower.pl:struct_type_plans/2), so
    // one left fold down the list resolves every child before its parent.
    const ids = new Map<string, number>();
    const pending = types.filter((plan) => per_type.has(plan.name));
    return pending.reduce<Observable<unknown>>(
      (chain, plan) => chain.pipe(concatMap(() =>
        intern_one_type(seam, plan, per_type.get(plan.name)!, ids, apply_targets, text_plan)
      )),
      of(undefined),
    ).pipe(
      map(() => arrivals.map((arrival): IArrivalRow => {
        const refs = ref_columns[arrival.rel];
        if (refs === undefined) return arrival;
        return { rel: arrival.rel, sign: arrival.sign, row: rewrite_row(arrival.row, refs, by_name, ids) };
      })),
    );
  },
};

function intern_one_type(
  seam: ISqlSeam,
  plan: IStructTypePlan,
  bucket: ReadonlyMap<string, ICollected>,
  ids: Map<string, number>,
  apply_targets: ((arrivals: IArrivalBatch) => Observable<unknown>) | undefined,
  text_plan: ITextInternPlan | undefined,
): Observable<unknown> {
  const semantics = [...bucket.keys()];
  const rows = [...bucket.values()].map((collected) =>
    collected.fields.map((field) => (is_child_reference(field) ? id_for(ids, field.child_semantic) : field)) as IRow
  );
  const arrivals: IArrivalBatch = rows.map((row): IArrivalRow => ({
    rel: plan.name,
    sign: "add",
    row,
  }));
  // The arrival door never sees a target row, and the preflight, insert and
  // key lookup all read one tuple, so its text columns intern before encoding.
  const staged = text_plan === undefined
    ? of(arrivals)
    : TextPlane.intern(seam, text_plan, arrivals);
  return staged.pipe(
    concatMap((interned) => intern_target_rows(seam, plan, semantics, interned, ids, apply_targets)),
  );
}

function intern_target_rows(
  seam: ISqlSeam,
  plan: IStructTypePlan,
  semantics: readonly string[],
  arrivals: IArrivalBatch,
  ids: Map<string, number>,
  apply_targets: ((arrivals: IArrivalBatch) => Observable<unknown>) | undefined,
): Observable<unknown> {
  const lookup_to_semantic = new Map<string, string>();
  const tuple_by_key = new Map<string, string>();
  const tuples = arrivals.map((arrival, index) => {
    const fields = arrival.row;
    const tuple = JSON.stringify(fields);
    const key = JSON.stringify(plan.key_indices.map((position) => fields[position]));
    const prior = tuple_by_key.get(key);
    if (prior !== undefined && prior !== tuple) {
      throw new Error(`relation_reference_conflict(${plan.name}, ${key}, ${prior}, ${tuple})`);
    }
    tuple_by_key.set(key, tuple);
    lookup_to_semantic.set(tuple, semantics[index]!);
    return fields;
  });
  const encoded = JSON.stringify(tuples);
  return seam.runner.execute(seam.db, { sql: plan.conflict_sql, args: [encoded] }).pipe(
    map((result) => {
      if (result.rows.length === 0) return undefined;
      const row = result.rows[0]!;
      throw new Error(
        `relation_reference_conflict(${plan.name}, ${String(row["__requested"])}, ${String(row["__stored"])})`,
      );
    }),
    concatMap(() => {
      return apply_targets === undefined
        ? seam.runner.execute(seam.db, { sql: plan.intern_sql, args: [encoded] })
        : apply_targets(arrivals);
    }),
    concatMap(() => seam.runner.execute(seam.db, { sql: plan.lookup_sql, args: [encoded] })),
    map((result) => {
      for (const row of result.rows) {
        const lookup = row["__lookup"] as string;
        const stored = row["__stored"] as string;
        if (stored !== lookup) {
          throw new Error(`relation_reference_conflict(${plan.name}, ${lookup}, ${stored})`);
        }
        const semantic = lookup_to_semantic.get(lookup);
        if (semantic === undefined) {
          throw new Error(`relation reference lookup returned an unknown row ${String(row["__lookup"])}`);
        }
        ids.set(semantic, Number(row["__id"]));
      }
      return undefined;
    }),
  );
}

/** IRowValue already admits a plain object, so `"child_semantic" in field`
 *  alone leaves the field's own type in play; the predicate is what narrows. */
function is_child_reference(
  field: IRowValue | { readonly child_semantic: string },
): field is { readonly child_semantic: string } {
  return (
    typeof field === "object" && field !== null && !Array.isArray(field) &&
    "child_semantic" in field && typeof field.child_semantic === "string"
  );
}

function id_for(ids: ReadonlyMap<string, number>, semantic: string): number {
  const id = ids.get(semantic);
  if (id === undefined) {
    throw new Error(`relation reference normalization read a child id before its target row: ${semantic}`);
  }
  return id;
}
