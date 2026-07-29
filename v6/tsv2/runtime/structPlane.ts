/**
 * structPlane.ts — the declared value plane at runtime (ruling
 * compound_storage = struct_as_rows; arc header
 * plans/2026-07-29-struct-as-rows-header.md).
 *
 * A declared struct value is a dictionary row referenced by content id. Every
 * arriving struct value is interned before the tick's own arrival statements
 * run, and the arrival row's ref column is rewritten from the VALUE to the
 * dense id. Everything downstream — arrival SQL, level joins, edge arms,
 * frontier staging, retraction — then handles a plain INTEGER column and
 * needs to know nothing about the value plane at all.
 *
 * ── the two things this file exists to guarantee ────────────────────────────
 *
 * EDGE 1, values not ids. Each dictionary row carries `__rendered`, the
 * value's canonical JSON, written ONCE here at intern time. Values are
 * immutable and children intern before parents, so a parent's rendering is one
 * concat over finished child renderings. The boundary read (lower.pl
 * dictionary_render_expr/3) is then a single rowid probe per row with no
 * recursion, whatever the nesting depth.
 *
 * EDGE 2, dictionaries are invisible. Nothing in this file writes a delta
 * table, a frontier, or a boundary row. `__dict_*` never appears in
 * `relColumns`, so the tick log cannot mention it. That is not tidiness: the
 * oracle holds real terms and has no dictionary, so a dictionary row in the
 * log would be a row the oracle can never produce and the byte grade would be
 * unwinnable.
 *
 * ── statement count ─────────────────────────────────────────────────────────
 *
 * Exactly TWO statements per declared type per tick that carries any struct
 * value — one set-based INSERT OR IGNORE over `json_each(?)`, one lookup of
 * the semantic keys — and ZERO statements on a tick that carries none. Flat in
 * the number of arriving values; there is no per-row shape to fall into.
 * `tests/structPlane.test.ts` is the count receipt.
 *
 * ── SLOT-GC-TIMING, decided: monotone dictionaries, named debt ──────────────
 *
 * Dictionary rows are never deleted. The decisive argument is Edge 2 itself:
 * because the dictionary is boundary-invisible, GC timing is UNOBSERVABLE in
 * the tick log — a collected and an uncollected implementation print the same
 * bytes for every program. What it costs is storage, and only for values that
 * are never seen again: interning is content-addressed, so a value that
 * re-arrives reuses its row and a churning program (keyed replace over a fixed
 * value set, the memory-soak shape) grows by exactly zero. The debt is real
 * for a program streaming ever-new distinct struct values, where the
 * dictionary grows with the number of DISTINCT values ever seen rather than
 * the number live; one row costs its canonical JSON twice (`__semantic` and
 * `__rendered`) plus the flattened columns. Collecting it means refcounting
 * every ref column across arrivals, level heads, edge heads, frontier staging
 * and retention — the derived planes copy ids, so an arrival-plane-only count
 * would delete rows a derived row still renders — and that is its own arc.
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
 * ONE deliberate asymmetry: the oracle ALSO refuses a struct value whose keys
 * are not already sorted (SLOT-ARRIVAL-CANONICAL-ORDER), and this door does
 * not. That refusal exists to protect the oracle COMPARISON — the oracle
 * stores world values as terms and its Set membership is term identity, so two
 * spellings of one value would be two rows there and one dictionary row here.
 * At runtime there is no oracle and no term store: interning is content
 * addressed, so any key order lands on the same row and no divergence is
 * reachable. Holding live HTTP payloads to a sorted-key spelling would buy
 * nothing and reject correct JSON.
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
      throw new Error(`struct intern lost the id for ${refType} value ${rendered}`);
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
      (chain, plan) => chain.pipe(concatMap(() => internOneType(seam, plan, perType.get(plan.name)!, ids))),
      of(undefined),
    ).pipe(
      map((): IArrivalBatch => arrivals.map((arrival): IArrivalRow => {
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
): Observable<unknown> {
  const lookupToSemantic = new Map<string, string>();
  const tuples = [...bucket.entries()].map(([semantic, collected]) => {
    const fields = collected.fields.map((field) =>
      typeof field === "object" && field !== null && "childSemantic" in field
        ? idFor(ids, field.childSemantic)
        : field
    );
    lookupToSemantic.set(JSON.stringify(fields), semantic);
    return fields;
  });
  return seam.runner.execute(seam.db, { sql: plan.internSql, args: [JSON.stringify(tuples)] }).pipe(
    concatMap(() => seam.runner.execute(seam.db, { sql: plan.lookupSql, args: [JSON.stringify(tuples)] })),
    map((result) => {
      for (const row of result.rows) {
        const semantic = lookupToSemantic.get(row["__lookup"] as string);
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
    throw new Error(`struct intern read a child id before its type was interned: ${semantic}`);
  }
  return id;
}
