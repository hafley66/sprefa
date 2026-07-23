/**
 * tasks.d.ts — the TS type ledger. The counterpart of src/tasks.rs: that file keeps
 * the Rust mind (the SQLite cascade, Traits A-D, parity-proven); this one keeps the
 * TS mind (the rel-kind algebra that IS the type system). Ambient: no runtime.
 *
 * Through-line (session 20260723.2): the rel-kind algebra — the RxJS trinity
 * (Observable / Subject / BehaviorSubject) + BufferPolicy + materialized/lazy/port
 * + EDB/IDB — IS the type system. Its job: make illegal rx combinations into type
 * errors (seed a cold derived rel; assert into a sink) and encode the subscribe
 * lifecycle rx cannot express in its own types. SEED = v5 src/typecheck.rs +
 * src/engine/typed_plan.rs; lifted, not kept.
 *
 * This file is the bootstrap: the algebra declared, the trinity mapping fixed, and
 * kind→primitive resolution expressed as a type-level function. The real engine
 * (parser + SQLite boundary) lands in src/ and is checked against this surface.
 *
 * Compiler: tsgo / tsc7 (TypeScript native preview). Foundation: actual rxjs.
 */

import type { Observable, Subject, BehaviorSubject } from "rxjs";

// ─────────────────────────────────────────────────────────────────────────────
// BufferPolicy — the one knob that bridges rx and CSP / Go-channels.
// ─────────────────────────────────────────────────────────────────────────────
//   replay 0, no state           = Subject (event bus).
//   replay 0, rendezvous         = unbuffered `chan` (send blocks for a receiver).
//   replay N                     = ReplaySubject(N) = buffered `chan` (blocks full).
//   replay 1, onFull "overwrite" = BehaviorSubject (non-blocking, latest wins).
// Subject/channel SEND is the blocking pole; BehaviorSubject READ is non-blocking
// (state). One knob bridges the two models.
export type OnFull = "block" | "overwrite" | "drop";

export type BufferPolicy = {
  readonly replay: number; // 0 = event; 1 = latest-wins state; N = replay window
  readonly onFull: OnFull;
};

// ─────────────────────────────────────────────────────────────────────────────
// The rel-kind algebra. A dl rel's kind is the cross-product of these axes; the
// kind resolves to exactly one rx primitive (the trinity). That resolution IS the
// type system.
// ─────────────────────────────────────────────────────────────────────────────

/** Temperature. cold = unicast, re-subscribe re-runs (a pipe). hot = multicast. */
export type Temperature = "cold" | "hot";

/** Shape. pipe = a derived rel (creation op / .pipe). event = a feed. state = latest. */
export type RelShape = "pipe" | "event" | "state";

/** Origin. EDB = facts (source). IDB = derived. */
export type Origin = "EDB" | "IDB";

/** Materialization. materialized = a table. lazy = on demand. port = a WASM/CLI boundary. */
export type Materialization = "materialized" | "lazy" | "port";

/** The full rel kind. Immutable once declared; fixes the rel's rx primitive. */
export interface RelKind {
  readonly shape: RelShape;
  readonly temperature: Temperature;
  readonly buffer: BufferPolicy;
  readonly origin: Origin;
  readonly materialization: Materialization;
}

// ─────────────────────────────────────────────────────────────────────────────
// The trinity, named literally. Each row of the kind→primitive table, as a type,
// parameterized over the row tuple T (the rel's column value tuple).
// ─────────────────────────────────────────────────────────────────────────────
//   dl home                              | rx primitive          | temperature
//   cold/lazy derived rel                | Observable<T>         | cold
//   event feed (@in / @out)              | Subject<T>            | hot
//   @next / latest-by-gen                | BehaviorSubject<T>    | hot
//
// A long-running effect is NOT a 4th primitive: it is a pipe (`concatMap` into
// effects), not a type. The trinity stays three.

/** A cold/lazy derived rel. Re-subscribe re-runs the pipe. */
export type ColdDerivedRel<T> = Observable<T>;

/** An event feed: @in / @out. Late subscribers miss the past. */
export type EventRel<T> = Subject<T>;

/** @next / latest-by-gen. Sync-eager non-blocking read of the current value. */
export type StateRel<T> = BehaviorSubject<T>;

// ─────────────────────────────────────────────────────────────────────────────
// Kind → primitive resolution. The type-level function that turns a RelKind into
// its rx primitive, or `never` when no legal primitive exists. This is the
// through-line: the kind IS the type.
//
// Discriminates on shape + temperature (both literal unions → reliable narrowing).
// The buffer invariant below is a constructor contract the engine enforces when it
// mints a kind (number-literal matching is fragile in conditionals, so the algebra
// carries it as a documented invariant, not a condition):
//   pipe  ⇒ cold, buffer.replay 0.
//   event ⇒ hot,  buffer.replay 0.
//   state ⇒ hot,  buffer.replay 1, onFull "overwrite".
// ─────────────────────────────────────────────────────────────────────────────
export type ResolveRel<K extends RelKind, T> =
  K extends { shape: "pipe"; temperature: "cold" } ? ColdDerivedRel<T>
  : K extends { shape: "event"; temperature: "hot" } ? EventRel<T>
  : K extends { shape: "state"; temperature: "hot" } ? StateRel<T>
  : never; // no legal primitive — the rel kind is itself a type error

// ─────────────────────────────────────────────────────────────────────────────
// dl surface (forward — the parser fills these). The contract a parsed program
// must satisfy so each rel resolves under ResolveRel. Subset for the first arc:
// rel decl, facts, rules, @next, @async, ?.
// ─────────────────────────────────────────────────────────────────────────────

/** A declared rel. Its kind fixes its rx primitive via ResolveRel. */
export interface RelDecl<T = unknown> {
  readonly name: string;
  readonly kind: RelKind;
  readonly columns: readonly string[]; // column names; T is their value tuple
  readonly rowType: T;
}

/** A source fact (EDB). Legal only on a rel whose kind admits a source feed. */
export interface Fact<T = unknown> {
  readonly rel: string;
  readonly row: T;
}

/** A derived rule (IDB). Its body is the pipe; its head rel must be cold-derived. */
export interface Rule {
  readonly head: string;
  readonly body: readonly unknown[]; // body predicates (filled by the parser)
}

// ─────────────────────────────────────────────────────────────────────────────
// The SQLite boundary (forward). The TS engine reads facts the extraction (Rust,
// WASM/CLI) wrote; the two speak through SQLite. NOT an FFI call — a connection.
// Default lib: better-sqlite3 (native, read-only opens a large db without loading
// it into heap — the RAM bound stays SQLite's, the project thesis). node:sqlite is
// the zero-dep alternative; sql.js is rejected (loads the whole db into WASM heap).
// ─────────────────────────────────────────────────────────────────────────────

/** A read-only SQLite connection string the extraction and the engine share. */
export type SqliteConnStr = string; // e.g. "file:./db.sqlite?mode=ro"
