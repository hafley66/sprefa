/**
 * lower.ts — lower a stratified dl program to rxjs Observables, stratum by stratum,
 * in stratification order. Pure + deterministic; no SQLite, no IO. Fact sources are
 * INJECTED as plain Observables (tests use ReplaySubject/Subject).
 *
 * Pipeline shape (pinned rxjs ops, one honest divergence — see NOTES below):
 *   combineLatest(body rel Observables)            // the join: one Row[] per body rel
 *     |> map(equi-join + selection + projection)   // set-valued: the full Row[] per emission
 *     |> map(group-aggregate, when a head has Agg) // max/min/sum/count, group-by non-agg vars
 *
 * Recursive strata (SCC > 1 or a self-loop) lower when the in-memory backend can run
 * them: every member lazy IDB, no aggregates (non-monotone under recursion). The stratum
 * becomes ONE fixpoint pipe — combineLatest(external body rels) |> map(bottom-up naive
 * fixpoint over the stratum's rules) — and each member rel projects its set out of it.
 * A stratum the backend declines (a materialized member = the heavy/cascade-delegate
 * path, which needs the SQLite engine; or an aggregate head) is collected as a
 * `RecursiveStratumDeferred` marker, so the deferral stays explicit.
 *
 * RelKind: every lowered derived rel is a cold Observable (cold-derived trinity row in
 * tasks.d.ts). EDB rels resolve to whatever the injected source Observable is.
 *
 * Stratified negation: a `!rel(args)` body predicate is an ANTI-join — `equiJoin`
 * drops a binding when the negated rel's CURRENT row set has any row consistent with
 * it, introducing no new bindings. `stratify` (rulegraph.ts) already refused any
 * program where the negated rel shares an SCC with the reader, so both lowering paths
 * below (the acyclic `lowerDerivedRule` map, and the in-stratum `stratumFixpoint`) can
 * resolve a negated ref's rows exactly like a positive one (lowered map, else
 * external/member row set) — the safety check already ran.
 *
 * ── NOTES on the pinned ops ──────────────────────────────────────────────────────
 * combineLatest is used literally for joins. `map` holds equi-join + selection +
 * projection (and aggregation) because a rel emission is the FULL current row set
 * (set semantics): rxjs `filter`/`reduce`/`scan` operate at EMISSION granularity, but
 * selection and group-aggregation are ELEMENT-level transforms over the row set, so
 * they live inside `map`. The recursive fixpoint is a plain `while` INSIDE `map`, per
 * the labs verdict (src/labs/fixpoint.ts): over a sync hop `expand` is a fancy `while`;
 * it earns its keep only across an async hop (the cascade-delegate backend, when the
 * engine wiring lands).
 */

import { combineLatest, of, map, type Observable } from "rxjs";
import type { Program, Rule, RelRef, NegRelRef, Compare, HeadTerm, RelDecl, AggFn, NegArg } from "./ast.ts";
import { buildRuleGraph, scc, stratify } from "./rulegraph.ts";
import type { Stratum } from "./types.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Public types.
// ─────────────────────────────────────────────────────────────────────────────

/** A row is a tuple of column values (JSON-representable primitives). */
export type Row = readonly unknown[];

/** Fact sources: per-rel-name, an Observable emitting the rel's full current row set. */
export type Sources = Map<string, Observable<Row[]>>;

/**
 * Marker that a recursive stratum was encountered and NOT lowered — the in-memory
 * fixpoint backend declined it (a non-lazy/EDB member, or an aggregate head inside the
 * cycle). Collected (not thrown) by `lowerProgram` into `LoweredProgram.deferred`, so a
 * program mixing lowerable and declined strata still lowers everything else. The
 * declined shapes are the cascade-delegate arc (engine wiring).
 */
export class RecursiveStratumDeferred extends Error {
  readonly stratum: Stratum;
  constructor(stratum: Stratum) {
    super(
      `recursive stratum deferred (SCC > 1 / self-loop): rels = [${stratum.rels.join(", ")}]`,
    );
    this.name = "RecursiveStratumDeferred";
    this.stratum = stratum;
  }
}

/** The result of lowering: the rels that lowered cleanly + the strata deferred. */
export interface LoweredProgram {
  /** rel name -> cold Observable of its current row set (acyclic rels only). */
  readonly rels: ReadonlyMap<string, Observable<Row[]>>;
  /** recursive strata not lowered (empty for a fully acyclic program). */
  readonly deferred: readonly RecursiveStratumDeferred[];
}

// ─────────────────────────────────────────────────────────────────────────────
// A binding: variable name -> value, produced by the equi-join. Keeps every bound
// variable so selection (Compare) and aggregation can read vars NOT projected to the head.
// ─────────────────────────────────────────────────────────────────────────────
type Binding = Map<string, unknown>;

// ─────────────────────────────────────────────────────────────────────────────
// lowerProgram — the entry point.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Lower a program stratum by stratum, in stratification (topological) order.
 * EDB rels (origin EDB, or no rule) resolve to their injected source (empty if none).
 * Acyclic IDB rels lower to a cold Observable pipe. Recursive strata are deferred.
 *
 * Throws `NonStratifiableError` (rulegraph.ts) if `prog` has a negated rel inside its
 * own SCC — surfaced before any Observable is constructed.
 */
export function lowerProgram(prog: Program, sources: Sources): LoweredProgram {
  const graph = buildRuleGraph(prog);
  const sccs = scc(graph);
  const strata = stratify(graph, sccs);

  const relDecls = new Map<string, RelDecl>();
  for (const declaration of prog.rels) relDecls.set(declaration.name, declaration);

  const rulesByHead = new Map<string, Rule[]>();
  for (const rule of prog.rules) {
    const list = rulesByHead.get(rule.head);
    if (list) list.push(rule);
    else rulesByHead.set(rule.head, [rule]);
  }

  const lowered = new Map<string, Observable<Row[]>>();
  const deferred: RecursiveStratumDeferred[] = [];

  for (const stratum of strata) {
    if (stratum.recursive) {
      if (recursiveBackend(stratum, rulesByHead, relDecls) === "defer") {
        deferred.push(new RecursiveStratumDeferred(stratum));
        continue;
      }
      for (const [relName, obs] of lowerRecursiveStratum(stratum, rulesByHead, lowered, sources)) {
        lowered.set(relName, obs);
      }
      continue;
    }
    // acyclic stratum: one rel (single-rel SCC). Lower it.
    for (const relName of stratum.rels) {
      const decl = relDecls.get(relName);
      const isEdb = decl?.origin === "EDB";
      const rules = rulesByHead.get(relName);
      if (isEdb || !rules || rules.length === 0) {
        // EDB, or an IDB rel with no rule: the injected source, or empty if none provided.
        lowered.set(relName, sources.get(relName) ?? of([] as Row[]));
        continue;
      }
      lowered.set(relName, lowerRelRules(rules, relDecls, lowered, sources, relName));
    }
  }

  return { rels: lowered, deferred };
}

/** Lower one rel's rule(s). Multiple rules heading the same rel UNION (combineLatest + dedup). */
function lowerRelRules(
  rules: readonly Rule[],
  relDecls: ReadonlyMap<string, RelDecl>,
  lowered: ReadonlyMap<string, Observable<Row[]>>,
  sources: Sources,
  headRel: string,
): Observable<Row[]> {
  const ruleObs = rules.map((rule) => lowerDerivedRule(rule, relDecls, lowered, sources, headRel));
  if (ruleObs.length === 1) return ruleObs[0]!;
  // UNION: re-emit the deduped, sorted concatenation whenever any rule's inputs move.
  return combineLatest(ruleObs).pipe(map((sets) => dedupSorted(sets.flat())));
}

// ─────────────────────────────────────────────────────────────────────────────
// Recursive strata. Backend split (the Epic 2 seam): RelKind.materialization selects.
//   lazy IDB members, no aggregates  → the in-memory fixpoint below (this file).
//   anything else                     → defer (the cascade-delegate path needs the
//                                       SQLite engine; aggregates are non-monotone).
// ─────────────────────────────────────────────────────────────────────────────

function recursiveBackend(
  stratum: Stratum,
  rulesByHead: ReadonlyMap<string, Rule[]>,
  relDecls: ReadonlyMap<string, RelDecl>,
): "fixpoint" | "defer" {
  for (const relName of stratum.rels) {
    const decl = relDecls.get(relName);
    if (!decl || decl.origin === "EDB") return "defer";
    if (decl.kind.materialization !== "lazy") return "defer";
    const rules = rulesByHead.get(relName);
    if (!rules || rules.length === 0) return "defer"; // a cycle member needs a rule; guard anyway
    for (const rule of rules) {
      if (rule.headTerms.some((term) => term.kind === "hagg")) return "defer";
    }
  }
  return "fixpoint";
}

/**
 * Lower one recursive stratum: combineLatest(the external body rels) |> map(bottom-up
 * fixpoint over the stratum's rules). Each member rel projects its converged set out of
 * the one shared pipe. Cold like every other lowered rel: each subscriber re-runs.
 */
function lowerRecursiveStratum(
  stratum: Stratum,
  rulesByHead: ReadonlyMap<string, Rule[]>,
  lowered: ReadonlyMap<string, Observable<Row[]>>,
  sources: Sources,
): Map<string, Observable<Row[]>> {
  const members = new Set(stratum.rels);
  const stratumRules: Rule[] = stratum.rels.flatMap((r) => rulesByHead.get(r) ?? []);

  // External inputs: body rels outside the stratum, deduped, first-seen order. A
  // negated ref counts too — `stratify` already refused a negated ref whose
  // target shares this stratum's SCC, so any `notrel` seen here is guaranteed external.
  const externalNames: string[] = [];
  for (const rule of stratumRules) {
    for (const pred of rule.body) {
      if (
        (pred.kind === "rel" || pred.kind === "notrel") &&
        !members.has(pred.rel) &&
        !externalNames.includes(pred.rel)
      ) {
        externalNames.push(pred.rel);
      }
    }
  }
  const externalObs = externalNames.map((relName) => {
    const obs = lowered.get(relName) ?? sources.get(relName);
    if (!obs) {
      throw new Error(
        `lower: body rel '${relName}' of recursive stratum [${stratum.rels.join(", ")}] has no ` +
          `source and is not lowered (undeclared, or part of a deferred recursive stratum)`,
      );
    }
    return obs;
  });
  const inputs$: Observable<Row[][]> =
    externalObs.length > 0 ? combineLatest(externalObs) : of([] as Row[][]);

  const fixpoint$ = inputs$.pipe(
    map((externalSets) => {
      const externalRows = new Map<string, Row[]>();
      externalNames.forEach((relName, index) => externalRows.set(relName, externalSets[index]!));
      return stratumFixpoint(stratumRules, members, externalRows);
    }),
  );

  const out = new Map<string, Observable<Row[]>>();
  for (const relName of stratum.rels) {
    out.set(relName, fixpoint$.pipe(map((sets) => sets.get(relName) ?? [])));
  }
  return out;
}

/**
 * Bottom-up naive fixpoint with dedup: re-evaluate every stratum rule against the
 * current sets until no rule admits a new row. Terminates — the constants all come from
 * the (finite) input rows, so the derivable row space is finite. Plain `while`, per the
 * labs verdict: the hop is sync, `expand` would be a fancy `while` here.
 */
function stratumFixpoint(
  rules: readonly Rule[],
  members: ReadonlySet<string>,
  externalRows: ReadonlyMap<string, Row[]>,
): Map<string, Row[]> {
  const current = new Map<string, Row[]>();
  const seen = new Map<string, Set<string>>();
  for (const memberRel of members) {
    current.set(memberRel, []);
    seen.set(memberRel, new Set());
  }

  let grew = true;
  while (grew) {
    grew = false;
    for (const rule of rules) {
      const relRows = new Map<string, Row[]>();
      for (const predicate of rule.body) {
        if (predicate.kind === "cmp" || relRows.has(predicate.rel)) continue;
        relRows.set(predicate.rel, members.has(predicate.rel) ? current.get(predicate.rel)! : (externalRows.get(predicate.rel) ?? []));
      }
      const bindings = equiJoin(rule, relRows);
      const selected = applySelection(rule, bindings);
      const produced = projectAndAggregate(rule.headTerms, selected, false);
      const headSeen = seen.get(rule.head)!;
      const headRows = current.get(rule.head)!;
      for (const row of produced) {
        const rowKey = JSON.stringify(row);
        if (!headSeen.has(rowKey)) {
          headSeen.add(rowKey);
          headRows.push(row);
          grew = true;
        }
      }
    }
  }

  const out = new Map<string, Row[]>();
  for (const memberRel of members) out.set(memberRel, dedupSorted(current.get(memberRel)!));
  return out;
}

/**
 * Lower one derived rule: combineLatest(body rel Observables) |> map(join+select+project+agg).
 * A negated (`notrel`) body predicate still needs its target rel's current row
 * set to filter against, so it counts as a body source here exactly like a positive ref
 * — the anti-join happens inside `equiJoin`, not at this resolution layer.
 */
function lowerDerivedRule(
  rule: Rule,
  relDecls: ReadonlyMap<string, RelDecl>,
  lowered: ReadonlyMap<string, Observable<Row[]>>,
  sources: Sources,
  headRel: string,
): Observable<Row[]> {
  const bodyRelNames = distinctBodyRelNames(rule.body);
  const bodyObs$ = bodyObsFor(bodyRelNames, lowered, sources, headRel);
  const hasAgg = rule.headTerms.some((term) => term.kind === "hagg");
  return bodyObs$.pipe(
    map((rowSets) => {
      const relRows = new Map<string, Row[]>();
      bodyRelNames.forEach((name, index) => relRows.set(name, rowSets[index]!));
      const bindings = equiJoin(rule, relRows); // 1. join: cartesian product + shared-var equality + literal selection + anti-join
      const selected = applySelection(rule, bindings); // 2. selection: Compare predicates
      return projectAndAggregate(rule.headTerms, selected, hasAgg); // 3. project head cols (+ 4. group-aggregate)
    }),
  );
}

/** Distinct rel names read by a rule's body — `RelRef` and `NegRelRef` both count
 *  (a negated ref still needs its target's current rows); `Compare` does not. First-
 *  seen order, deduped, so repeated refs to the same rel share one subscription. */
function distinctBodyRelNames(body: readonly (RelRef | Compare | NegRelRef)[]): string[] {
  const names: string[] = [];
  for (const pred of body) {
    if (pred.kind !== "cmp" && !names.includes(pred.rel)) names.push(pred.rel);
  }
  return names;
}

/** Resolve each body rel name to its Observable (lowered derived, or injected source). */
function bodyObsFor(
  bodyRelNames: readonly string[],
  lowered: ReadonlyMap<string, Observable<Row[]>>,
  sources: Sources,
  headRel: string,
): Observable<Row[][]> {
  const obs = bodyRelNames.map((relName) => {
    const fromLowered = lowered.get(relName);
    if (fromLowered) return fromLowered;
    const fromSource = sources.get(relName);
    if (fromSource) return fromSource;
    throw new Error(
      `lower: body rel '${relName}' of rule head '${headRel}' has no source and is not lowered ` +
        `(undeclared, or part of a deferred recursive stratum)`,
    );
  });
  if (obs.length === 0) {
    // no body rel refs: a head with no join source is degenerate (EDB-fact territory); emit empty.
    return of([] as Row[][]);
  }
  return combineLatest(obs);
}

// ─────────────────────────────────────────────────────────────────────────────
// Equi-join: nested-loop over body predicates in body order. A Lit arg selects
// (row[col] === value); a Var arg binds (first occurrence) or joins (shared-var
// consistency); a Wild arg (positive or negated ref) always matches, binds nothing —
// two wildcards never compare to each other, since neither ever binds. A
// `notrel` predicate is an ANTI-join: it drops any binding for which the negated
// rel's CURRENT row set has a compatible row, and otherwise leaves the binding
// untouched — no new columns, no new bindings (negation introduces no information,
// only rules it out).
// ─────────────────────────────────────────────────────────────────────────────

function equiJoin(rule: Rule, relRows: ReadonlyMap<string, Row[]>): Binding[] {
  let accumulator: Binding[] = [new Map()];
  for (const predicate of rule.body) {
    if (predicate.kind === "cmp") continue; // Compare handled in applySelection
    const rows = relRows.get(predicate.rel) ?? [];
    if (predicate.kind === "notrel") {
      accumulator = accumulator.filter((binding) => !rows.some((row) => tryBind(predicate, row, binding) !== null));
    } else {
      const next: Binding[] = [];
      for (const binding of accumulator) {
        for (const row of rows) {
          const extended = tryBind(predicate, row, binding);
          if (extended !== null) next.push(extended);
        }
      }
      accumulator = next;
    }
    if (accumulator.length === 0) break; // nothing survived this predicate; stop early
  }
  return accumulator;
}

/** Extend `prev` with row's columns per the ref's args. Null if a Lit or shared-var
 *  check fails. Structural over `{args}` so both `RelRef` and `NegRelRef` reuse it
 *  (`Arg`/`NegArg` are the same Var|Lit|Wild set as of ast.ts's positive-wildcard
 *  landing). A `Wild` never binds and never selects — "don't project, don't
 *  consistency-check" — so two wildcards in the same rule are independently
 *  existential and never compare to each other. In a negated ref specifically, an
 *  unbound Var (or a Wild) is existentially quantified over the negated rel's rows
 *  (negation-as-failure); the anti-join filter above only checks the return value
 *  (compatible or not), discarding the extended map either way. `ref.args.length` may
 *  be shorter than the row's width (trailing-arg elision, ast.ts's `RelRef` doc): the
 *  loop below only reads `args[col]` for `col < ref.args.length`, so missing trailing
 *  columns are never consulted — same effect as an explicit trailing `Wild`. */
function tryBind(ref: { readonly args: readonly NegArg[] }, row: Row, prev: Binding): Binding | null {
  const next = new Map(prev);
  for (let columnIndex = 0; columnIndex < ref.args.length; columnIndex++) {
    const argument = ref.args[columnIndex]!;
    if (argument.kind === "wild") continue; // matches any value, binds nothing
    const columnValue = row[columnIndex];
    if (argument.kind === "lit") {
      if (columnValue !== argument.value) return null; // literal selection
    } else {
      const boundValue = next.get(argument.name);
      if (boundValue !== undefined) {
        if (boundValue !== columnValue) return null; // shared-var consistency (equi-join key)
      } else {
        next.set(argument.name, columnValue);
      }
    }
  }
  return next;
}

// ─────────────────────────────────────────────────────────────────────────────
// Selection: drop bindings failing any Compare predicate.
// ─────────────────────────────────────────────────────────────────────────────

function applySelection(rule: Rule, bindings: readonly Binding[]): Binding[] {
  const comparisons = rule.body.filter((predicate): predicate is Compare => predicate.kind === "cmp");
  if (comparisons.length === 0) return bindings.slice();
  return bindings.filter((binding) => comparisons.every((comparison) => evalCmp(comparison, binding)));
}

function evalCmp(comparison: Compare, binding: Binding): boolean {
  const leftValue = binding.get(comparison.lhs.name);
  const rightValue = comparison.rhs.value;
  switch (comparison.op) {
    case "eq":
      return leftValue === rightValue;
    case "ne":
      return leftValue !== rightValue;
    case "lt":
      return (leftValue as number) < (rightValue as number);
    case "le":
      return (leftValue as number) <= (rightValue as number);
    case "gt":
      return (leftValue as number) > (rightValue as number);
    case "ge":
      return (leftValue as number) >= (rightValue as number);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Projection (+ group-aggregation). Head terms map 1:1 to output columns. A plain
// HeadVar is a group-by key / projection; a HeadAgg reduces its arg over each group.
// Output rows are deduped (set semantics) and sorted by JSON.stringify (deterministic).
// ─────────────────────────────────────────────────────────────────────────────

function projectAndAggregate(
  headTerms: readonly HeadTerm[],
  bindings: readonly Binding[],
  hasAgg: boolean,
): Row[] {
  if (!hasAgg) {
    const projected = bindings.map((binding) =>
      headTerms.map((term) => (term.kind === "hvar" ? binding.get(term.name) : undefined)),
    );
    return dedupSorted(projected);
  }

  // group by the non-aggregated head vars (the group key).
  const groupKeyNames = headTerms
    .filter((term): term is Extract<HeadTerm, { kind: "hvar" }> => term.kind === "hvar")
    .map((term) => term.name);

  const groups = new Map<string, { keyMap: Map<string, unknown>; rows: Binding[] }>();
  for (const binding of bindings) {
    const keyMap = new Map<string, unknown>();
    for (const groupKeyName of groupKeyNames) keyMap.set(groupKeyName, binding.get(groupKeyName));
    const groupKey = JSON.stringify(groupKeyNames.map((groupKeyName) => keyMap.get(groupKeyName)));
    let group = groups.get(groupKey);
    if (!group) {
      group = { keyMap, rows: [] };
      groups.set(groupKey, group);
    }
    group.rows.push(binding);
  }

  const out: Row[] = [];
  for (const group of groups.values()) {
    const row: unknown[] = headTerms.map((term) => {
      if (term.kind === "hvar") return group.keyMap.get(term.name);
      const values = group.rows.map((binding) => binding.get(term.arg.name));
      return aggregate(term.fn, values);
    });
    out.push(row);
  }
  return dedupSorted(out);
}

function aggregate(fn: AggFn, values: readonly unknown[]): unknown {
  switch (fn) {
    case "max":
      return values.reduce<unknown>(
        (maxValue, value) => (maxValue === undefined || (value as number) > (maxValue as number) ? value : maxValue),
        undefined,
      );
    case "min":
      return values.reduce<unknown>(
        (minValue, value) => (minValue === undefined || (value as number) < (minValue as number) ? value : minValue),
        undefined,
      );
    case "sum":
      return values.reduce<number>((sumAccumulator, value) => sumAccumulator + (value as number), 0);
    case "count":
      return values.length;
  }
}

/** Dedupe by JSON identity, then sort by JSON for deterministic emission order (set semantics). */
function dedupSorted(rows: readonly Row[]): Row[] {
  const seen = new Set<string>();
  const out: Row[] = [];
  for (const row of rows) {
    const rowKey = JSON.stringify(row);
    if (!seen.has(rowKey)) {
      seen.add(rowKey);
      out.push(row);
    }
  }
  return out.sort((rowA, rowB) => (JSON.stringify(rowA) < JSON.stringify(rowB) ? -1 : 1));
}
