import { concatMap, from, map, of, toArray, type Observable } from "rxjs";
import type { IArrivalBatch, IArrivalRow, IEnumPlane, IEnumRefColumns, IEnumTypePlan, IIncrementalRelationPlan, IRelDelta, IRow, IRowValue, ISqlSeam } from "./types.ts";
import { row_value_from_sql } from "./rows.ts";

function object(value: IRowValue, name: string): Record<string, unknown> {
  if (value !== null && typeof value === "object" && !Array.isArray(value) && !(value instanceof Uint8Array)) {
    return value as Record<string, unknown>;
  }
  if (typeof value !== "string") throw new Error(`enum_arrival_shape_mismatch: not_an_object(${name})`);
  try {
    const parsed: unknown = JSON.parse(value);
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error();
    return parsed as Record<string, unknown>;
  } catch {
    throw new Error(`enum_arrival_shape_mismatch: not_an_object(${name})`);
  }
}

function encode(
  plans: ReadonlyMap<string, IEnumTypePlan>, name: string, endpoint: number,
  sign: "add" | "del", value: IRowValue, variants: IArrivalRow[],
): number {
  const plan = plans.get(name);
  if (plan === undefined) throw new Error(`enum plan missing: ${name}`);
  const tagged = object(value, name);
  const tag = tagged.tag;
  if (typeof tag !== "string") throw new Error(`enum_arrival_shape_mismatch: missing_tag(${name})`);
  const variant = plan.variants.find((candidate) => candidate.tag === tag);
  if (variant === undefined) throw new Error(`enum_arrival_shape_mismatch: unknown_tag(${name}, ${tag})`);
  for (const field of variant.fields) if (!(field in tagged)) throw new Error(`enum_arrival_shape_mismatch: missing_key(${name}, ${field})`);
  for (const field of Object.keys(tagged)) if (field !== "tag" && !variant.fields.includes(field)) throw new Error(`enum_arrival_shape_mismatch: unknown_key(${name}, ${field})`);
  const row: IRowValue[] = [endpoint];
  variant.fields.forEach((field, index) => {
    let payload = tagged[field] as IRowValue;
    const nested = variant.field_enums?.[index];
    if (nested !== null && nested !== undefined) payload = encode(plans, nested, endpoint, sign, payload, variants);
    else if (Array.isArray(payload)) payload = JSON.stringify(payload);
    else if (payload !== null && typeof payload === "object" && !(payload instanceof Uint8Array)) payload = JSON.stringify(payload);
    row.push(payload);
  });
  variants.push({ rel: variant.rel, sign, row });
  return endpoint;
}

export const EnumPlane: IEnumPlane = {
  intern(types, ref_columns, arrivals) {
    if (types.length === 0 || arrivals.length === 0) return arrivals;
    const plans = new Map(types.map((plan) => [plan.name, plan]));
    const variants: IArrivalRow[] = [];
    const parents = arrivals.map((arrival): IArrivalRow => {
      const refs = ref_columns[arrival.rel];
      if (refs === undefined) return arrival;
      const row = [...arrival.row];
      refs.forEach((reference, index) => {
        if (reference === null || reference === undefined) return;
        const endpoint = reference.endpoint_index === null ? undefined : row[reference.endpoint_index];
        if (typeof endpoint !== "number" || !Number.isInteger(endpoint)) throw new Error(`enum_arrival_shape_mismatch: ambiguous_owner_context(${arrival.rel}, ${reference.name})`);
        row[index] = encode(plans, reference.name, endpoint, arrival.sign, arrival.row[index]!, variants);
      });
      return { ...arrival, row };
    });
    return [...variants, ...parents];
  },

  decode_deltas(seam, types, ref_columns, deltas) {
    return from(deltas).pipe(
      concatMap((delta) =>
        this.decode_rows(seam, types, ref_columns, delta.rel, delta.add).pipe(
          concatMap((add) =>
            this.decode_rows(seam, types, ref_columns, delta.rel, delta.del).pipe(
              map((del) => ({ ...delta, add, del })),
            ),
          ),
        ),
      ),
      toArray(),
    );
  },

  decode_rows(seam, types, ref_columns, rel, rows) {
    const refs = ref_columns[rel];
    if (refs === undefined || types.length === 0) return of(rows);
    const plans = new Map(types.map((plan) => [plan.name, plan]));
    const decode = (name: string, endpoint: IRowValue): Observable<IRowValue> => {
      if (typeof endpoint !== "number" || !Number.isInteger(endpoint)) throw new Error(`enum_boundary_shape_mismatch: endpoint(${name})`);
      const plan = plans.get(name);
      if (plan === undefined) throw new Error(`enum plan missing: ${name}`);
      return from(plan.variants).pipe(
        concatMap((variant) => {
          if (variant.select_sql === undefined) throw new Error(`enum variant read missing: ${variant.rel}`);
          const sql = `SELECT * FROM (${variant.select_sql}) AS "__enum_payload" WHERE "__enum_payload"."id" = ?`;
          return seam.runner.execute(seam.db, { sql, args: [endpoint] }).pipe(map((result) => result.rows.map((row) => ({ variant, row }))));
        }),
        concatMap((matches) => from(matches)), toArray(),
        concatMap((matches) => {
          if (matches.length !== 1) throw new Error(`enum_boundary_shape_mismatch: ambiguous_endpoint(${name}, ${endpoint})`);
          const { variant, row } = matches[0]!;
          return from(variant.fields).pipe(concatMap((field, index) => {
            const nested = variant.field_enums?.[index];
            const raw = row[field];
            const type = variant.field_types?.[index];
            const value = row_value_from_sql(type, raw);
            return nested === null || nested === undefined
              ? of([field, public_value(value, type)] as const)
              : decode(nested, value).pipe(map((nested_value) => [field, nested_value] as const));
          }), toArray(), map((fields) => ({ tag: variant.tag, ...Object.fromEntries(fields) }) as IRowValue));
        }),
      );
    };
    return from(rows).pipe(concatMap((row) => from(row).pipe(concatMap((value, index) => {
      const reference = refs[index];
      return reference === null || reference === undefined ? of(value) : decode(reference.name, value);
    }), toArray())), toArray());
  },
};

/** The emitted select_sql has already resolved storage ids through dictionary
 * and list views. JSON and ref columns retain their textual carrier until this
 * final boundary conversion. */
function public_value(value: IRowValue, type: string | undefined): IRowValue {
  if ((type === "json" || type === "ref") && typeof value === "string") {
    try { return JSON.parse(value) as IRowValue; } catch { return value; }
  }
  return value;
}
