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

import type {
  IArrivalBatch,
  IArrivalRow,
  IRow,
  IRowValue,
  ISqlSeam,
  IStructPlane,
  IStructTypePlan,
  IStructRefColumns,
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

function canonicalText(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/** Host and HTTP arrivals carry JSON through IRowValue's text side. Schedule
 * fixtures may still inject the already-decoded object. Both spellings enter
 * the same shape check and canonicalization path before interning. */
function decodedStructValue(value: unknown): unknown {
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
function checkShape(plan: IStructTypePlan, byName: ReadonlyMap<string, IStructTypePlan>, value: unknown): void {
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
    const refType = plan.refs[index];
    const field = value[column];
    if (refType !== null && refType !== undefined) {
      const childPlan = byName.get(refType);
      if (childPlan === undefined) {
        throw new Error(`type_arrival_shape_mismatch: column_type_unknown(${refType})`);
      }
      checkShape(childPlan, byName, field);
    }
  });
}

/** `TypeName` ++ canonical JSON. The type name is an identifier and the
 *  rendering always starts with `{`, so the concatenation is unambiguous, and
 *  it matches lower.pl:struct_intern_statements/5 byte for byte. Full
 *  canonical text rather than a digest because @libsql registers no UDF and
 *  SQLite ships no hash (SLOT-SEMANTIC-DIGEST in 0_type_plane.pl's header):
 *  strictly stronger than a hash, with no collision case to reason about. */
function semanticKey(typeName: string, rendered: string): string {
  return `${typeName}${rendered}`;
}

interface ICollected {
  readonly rendered: string;
  /** column values; a ref column holds the CHILD'S semantic key until the
   *  child's own type has been interned and its dense id is known. */
  readonly fields: readonly (IRowValue | { readonly childSemantic: string })[];
}

function collect(
  plan: IStructTypePlan,
  byName: ReadonlyMap<string, IStructTypePlan>,
  value: unknown,
  perType: Map<string, Map<string, ICollected>>,
): string {
  const decoded = decodedStructValue(value);
  checkShape(plan, byName, decoded);
  const object = decoded as Record<string, unknown>;
  const fields = plan.columns.map((column, index) => {
    const refType = plan.refs[index];
    if (refType === null || refType === undefined) return object[column] as IRowValue;
    // Post-order: the child is collected (and therefore interned) first.
    const childSemantic = collect(byName.get(refType)!, byName, object[column], perType);
    return { childSemantic };
  });
  const rendered = canonicalText(object);
  const semantic = semanticKey(plan.name, rendered);
  let bucket = perType.get(plan.name);
  if (bucket === undefined) {
    bucket = new Map<string, ICollected>();
    perType.set(plan.name, bucket);
  }
  bucket.set(semantic, { rendered, fields });
  return semantic;
}

function rewriteRow(
  row: IRow,
  refs: readonly (string | null)[],
  byName: ReadonlyMap<string, IStructTypePlan>,
  ids: ReadonlyMap<string, number>,
): IRow {
  return row.map((value, index) => {
    const refType = refs[index];
    if (refType === null || refType === undefined) return value;
    const rendered = canonicalText(decodedStructValue(value));
    const id = ids.get(semanticKey(refType, rendered));
    if (id === undefined) {
      throw new Error(`relation reference normalization lost the id for ${refType} value ${rendered}`);
    }
    return id;
  });
}

export const StructPlane: IStructPlane = {
  canonicalText,

  intern(
    seam: ISqlSeam,
    types: readonly IStructTypePlan[],
    refColumns: IStructRefColumns,
    arrivals: IArrivalBatch,
    applyTargets?: (arrivals: IArrivalBatch) => Observable<unknown>,
  ): Observable<IArrivalBatch> {
    if (types.length === 0 || arrivals.length === 0) return of(arrivals);
    const byName = new Map(types.map((plan) => [plan.name, plan]));
    const perType = new Map<string, Map<string, ICollected>>();
    for (const arrival of arrivals) {
      const refs = refColumns[arrival.rel];
      if (refs === undefined) continue;
      arrival.row.forEach((value, index) => {
        const refType = refs[index];
        if (refType === null || refType === undefined) return;
        collect(byName.get(refType)!, byName, value, perType);
      });
    }
    if (perType.size === 0) return of(arrivals);

    // `types` arrives in topological order (lower.pl:struct_type_plans/2), so
    // one left fold down the list resolves every child before its parent.
    const ids = new Map<string, number>();
    const pending = types.filter((plan) => perType.has(plan.name));
    return pending.reduce<Observable<unknown>>(
      (chain, plan) => chain.pipe(concatMap(() =>
        internOneType(seam, plan, perType.get(plan.name)!, ids, applyTargets)
      )),
      of(undefined),
    ).pipe(
      map(() => arrivals.map((arrival): IArrivalRow => {
        const refs = refColumns[arrival.rel];
        if (refs === undefined) return arrival;
        return { rel: arrival.rel, sign: arrival.sign, row: rewriteRow(arrival.row, refs, byName, ids) };
      })),
    );
  },
};

function internOneType(
  seam: ISqlSeam,
  plan: IStructTypePlan,
  bucket: ReadonlyMap<string, ICollected>,
  ids: Map<string, number>,
  applyTargets: ((arrivals: IArrivalBatch) => Observable<unknown>) | undefined,
): Observable<unknown> {
  const lookupToSemantic = new Map<string, string>();
  const tupleByKey = new Map<string, string>();
  const tuples = [...bucket.entries()].map(([semantic, collected]) => {
    const fields = collected.fields.map((field) =>
      typeof field === "object" && field !== null && "childSemantic" in field
        ? idFor(ids, field.childSemantic)
        : field
    );
    const tuple = JSON.stringify(fields);
    const key = JSON.stringify(plan.keyIndices.map((index) => fields[index]));
    const prior = tupleByKey.get(key);
    if (prior !== undefined && prior !== tuple) {
      throw new Error(`relation_reference_conflict(${plan.name}, ${key}, ${prior}, ${tuple})`);
    }
    tupleByKey.set(key, tuple);
    lookupToSemantic.set(tuple, semantic);
    return fields;
  });
  const encoded = JSON.stringify(tuples);
  const arrivals: IArrivalBatch = tuples.map((row) => ({
    rel: plan.name,
    sign: "add",
    row,
  }));
  return seam.runner.execute(seam.db, { sql: plan.conflictSql, args: [encoded] }).pipe(
    map((result) => {
      if (result.rows.length === 0) return undefined;
      const row = result.rows[0]!;
      throw new Error(
        `relation_reference_conflict(${plan.name}, ${String(row["__requested"])}, ${String(row["__stored"])})`,
      );
    }),
    concatMap(() => {
      return applyTargets === undefined
        ? seam.runner.execute(seam.db, { sql: plan.internSql, args: [encoded] })
        : applyTargets(arrivals);
    }),
    concatMap(() => seam.runner.execute(seam.db, { sql: plan.lookupSql, args: [encoded] })),
    map((result) => {
      for (const row of result.rows) {
        const lookup = row["__lookup"] as string;
        const stored = row["__stored"] as string;
        if (stored !== lookup) {
          throw new Error(`relation_reference_conflict(${plan.name}, ${lookup}, ${stored})`);
        }
        const semantic = lookupToSemantic.get(lookup);
        if (semantic === undefined) {
          throw new Error(`relation reference lookup returned an unknown row ${String(row["__lookup"])}`);
        }
        ids.set(semantic, Number(row["__id"]));
      }
      return undefined;
    }),
  );
}

function idFor(ids: ReadonlyMap<string, number>, semantic: string): number {
  const id = ids.get(semantic);
  if (id === undefined) {
    throw new Error(`relation reference normalization read a child id before its target row: ${semantic}`);
  }
  return id;
}
