/**
 * ast.ts — the typed program input. THE SHARED CONTRACT a future parser produces.
 *
 * Scope (this arc): rel decl, EDB/IDB origin, derived rules `head <- body`, body =
 * rel predicates sharing variables (the join), literal selection in a rel arg,
 * comparison selection, head aggregation (`max`/`min`/`sum`/`count`), and stratified
 * negation (`!rel(args)`, v5 surface spelling — src/ast.rs:370's `BodyItem::Neg`, no
 * `not` keyword). No parser here — the AST is hand-constructed typed data. No SQLite,
 * no rxjs, no IO.
 *
 * Deferrals (explicit, next arcs): wildcard `_` in a POSITIVE rel-ref position (a rel
 * position that neither binds nor selects) — v5 allows `_` in any body atom, but here
 * it is legal ONLY inside a negated ref (`NegRelRef.args`, `NegArg`'s `Wild` variant):
 * an unbound Var already gives existential quantification for a positive ref's
 * OWN row set, but a positive-ref wildcard also needs "don't project, don't consistency-
 * check" plumbing through equi-join and projection that hasn't landed. Also deferred:
 * extraction ops (scan/match/ast/sg/json/cmd), and closure/scc/node2vec operators. The
 * Var|Lit-only `Arg` set is the minimal shape for the positive-ref constructs in scope.
 *
 * Origin note: `RelKind.origin` (tasks.d.ts) IS the origin. `RelDecl.origin` mirrors
 * it as a top-level field so the lowerer reads origin without digging into the kind
 * cross-product. The two must agree; `buildRuleGraph`/`lowerProgram` read `decl.origin`.
 */

import type { RelKind, Origin } from "../../tasks.d.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Body + head positions.
// ─────────────────────────────────────────────────────────────────────────────

/** A literal constant. JSON-representable so it crosses the rx boundary by value. */
export type LitValue = string | number | boolean | null;

/** A variable reference: bound by a body rel ref, projected to the head. */
export interface Var {
  readonly kind: "var";
  readonly name: string;
}
/** A literal constant in a body or comparison position. */
export interface Lit {
  readonly kind: "lit";
  readonly value: LitValue;
}

/** One argument to a body rel reference. `Var` binds; `Lit` selects (row[col] === value). */
export type Arg = Var | Lit;

/** Existential wildcard `_`: matches any value in that column, binds nothing. Legal ONLY
 *  inside a negated ref (`NegArg`) for now — see the deferrals note above for why a
 *  positive-ref wildcard is still out of scope. */
export interface Wild {
  readonly kind: "wild";
}

/** An argument to a negated rel reference: `Arg` (Var|Lit, same as a positive ref) or
 *  `Wild` (`_`). A `Var` here reuses whatever binding rules `Arg` already has (checks
 *  equality if bound elsewhere in the body, otherwise is existentially quantified over
 *  the negated rel's rows); `Wild` is ALWAYS existentially quantified and never binds,
 *  even if the same rule uses `_` in more than one negated-arg position. */
export type NegArg = Arg | Wild;

/** Comparison operators for a selection predicate. */
export type CmpOp = "eq" | "ne" | "lt" | "le" | "gt" | "ge";

// ─────────────────────────────────────────────────────────────────────────────
// Body predicates: a rel reference (join source) or a comparison (selection).
// ─────────────────────────────────────────────────────────────────────────────

/** A rel reference as a body predicate. `rel` names an EDB/IDB rel; `args` are
 *  positional, one per declared column. A `Var` binds/joins; a `Lit` selects. */
export interface RelRef {
  readonly kind: "rel";
  readonly rel: string;
  readonly args: readonly Arg[];
}

/** A selection predicate: compare a bound `Var` to a constant. The `Var` must be
 *  bound by a body `RelRef` appearing before (or after) this predicate in `body`. */
export interface Compare {
  readonly kind: "cmp";
  readonly op: CmpOp;
  readonly lhs: Var;
  readonly rhs: Lit;
}

/** A negated rel reference: `!rel(args)` (v5 surface spelling, src/ast.rs:370
 *  `BodyItem::Neg`; stratified negation). Filters bindings whose projection matches ANY
 *  row currently in `rel`'s set — an existence check that introduces no new bindings
 *  (unlike `RelRef`). `rel` must be outside this rule's own SCC: a negation edge whose
 *  endpoints share a cycle is a `NonStratifiableError` (rulegraph.ts `stratify`) — v5's
 *  "forcing edge" check, src/typecheck.rs:1195. `args` are `NegArg`: `Var`|`Lit` as in a
 *  positive ref, or `Wild` (`_`) — legal here even though a positive ref can't take one
 *  yet (see the deferrals note above). */
export interface NegRelRef {
  readonly kind: "notrel";
  readonly rel: string;
  readonly args: readonly NegArg[];
}

/** One body predicate: a rel reference (the join source), a comparison (selection),
 *  or a negated rel reference (an anti-join filter). */
export type BodyPred = RelRef | Compare | NegRelRef;

// ─────────────────────────────────────────────────────────────────────────────
// Head terms: a plain Var (group-by / projection) or an aggregate of a Var.
// ─────────────────────────────────────────────────────────────────────────────

/** Aggregate functions supported in a rule head. */
export type AggFn = "max" | "min" | "sum" | "count";

/** A plain head column: the named bound variable (group-by key / projection). */
export interface HeadVar {
  readonly kind: "hvar";
  readonly name: string;
}
/** An aggregated head column: `fn(arg)` over the rows of each group. The `arg`
 *  variable must be bound by a body `RelRef`. Non-aggregated head vars are the group key. */
export interface HeadAgg {
  readonly kind: "hagg";
  readonly fn: AggFn;
  readonly arg: Var;
}

export type HeadTerm = HeadVar | HeadAgg;

// ─────────────────────────────────────────────────────────────────────────────
// Declared rels, rules, programs.
// ─────────────────────────────────────────────────────────────────────────────

/** A declared rel. Its `kind` fixes its rx primitive via ResolveRel (tasks.d.ts);
 *  `origin` mirrors `kind.origin` and is what the lowerer reads. */
export interface RelDecl {
  readonly name: string;
  readonly columns: readonly string[];
  readonly kind: RelKind;
  /** EDB = facts (source). IDB = derived. Mirrors `kind.origin`. */
  readonly origin: Origin;
}

/** A derived rule: `head(headTerms) <- body`. The head rel must be cold-derived (IDB). */
export interface Rule {
  /** Head rel name. */
  readonly head: string;
  /** One head term per head column, positional. */
  readonly headTerms: readonly HeadTerm[];
  /** Ordered body: rel references (the join) + comparisons (selection). */
  readonly body: readonly BodyPred[];
}

/** A program: declared rels + derived rules. Facts arrive as injected sources,
 *  not as AST nodes (no SQLite / RelStore / IO in this arc). */
export interface Program {
  readonly rels: readonly RelDecl[];
  readonly rules: readonly Rule[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Constructor helpers — keep hand-built programs in tests (and a future parser)
// terse and typo-resistant. Discriminants are filled in for exhaustiveness.
// ─────────────────────────────────────────────────────────────────────────────

/** A body/head variable reference. (`variable` — `var` is reserved.) */
export function variable(name: string): Var {
  return { kind: "var", name };
}
/** A literal constant. */
export function literal(value: LitValue): Lit {
  return { kind: "lit", value };
}
/** A body rel reference: `rel(name, ...args)`. Args are Var|Lit (descriptive helpers below). */
export function relRef(rel: string, ...args: readonly Arg[]): RelRef {
  return { kind: "rel", rel, args };
}
/** A negated body rel reference: `!rel(name, ...args)`. Args are Var|Lit|Wild. */
export function notRel(rel: string, ...args: readonly NegArg[]): NegRelRef {
  return { kind: "notrel", rel, args };
}
/** A Var arg (binding/join position) — shorthand so relRef reads positionally. */
export function v(name: string): Arg {
  return variable(name);
}
/** A Lit arg (selection position) — shorthand so relRef reads positionally. */
export function lit(value: LitValue): Arg {
  return literal(value);
}
/** A wildcard `_` arg — legal only inside `notRel`'s args. */
export function wild(): Wild {
  return { kind: "wild" };
}
/** A selection predicate: compare a bound variable to a constant. */
export function compare(op: CmpOp, varName: string, value: LitValue): Compare {
  return { kind: "cmp", op, lhs: variable(varName), rhs: literal(value) };
}
/** A plain head column. */
export function headVar(name: string): HeadVar {
  return { kind: "hvar", name };
}
/** An aggregated head column: `fn(argName)`. */
export function headAgg(fn: AggFn, argName: string): HeadAgg {
  return { kind: "hagg", fn, arg: variable(argName) };
}

// ─────────────────────────────────────────────────────────────────────────────
// RelKind presets. The kind cross-product is large; these are the shapes this arc
// lowers. A cold-derived (IDB) rel resolves to a cold Observable; an EDB source
// resolves to whatever the injected source Observable is (the trinity is advisory
// here — the lowerer treats the source as opaque).
// ─────────────────────────────────────────────────────────────────────────────

/** A cold/lazy derived rel (IDB). Lowers to a cold Observable (re-subscribe re-runs). */
export function coldDerived(columns: readonly string[]): RelKind {
  return { shape: "pipe", temperature: "cold", buffer: { replay: 0, onFull: "block" }, origin: "IDB", materialization: "lazy" };
}
/** An EDB fact rel (source). Fed by an injected Observable<Row[]> in `lowerProgram`. */
export function edbKind(): RelKind {
  return { shape: "event", temperature: "hot", buffer: { replay: 0, onFull: "block" }, origin: "EDB", materialization: "materialized" };
}

/** Declare an EDB (source) rel. */
export function edbRel(name: string, columns: readonly string[]): RelDecl {
  return { name, columns, kind: edbKind(), origin: "EDB" };
}
/** Declare an IDB (cold-derived) rel. */
export function derivedRel(name: string, columns: readonly string[]): RelDecl {
  return { name, columns, kind: coldDerived(columns), origin: "IDB" };
}
