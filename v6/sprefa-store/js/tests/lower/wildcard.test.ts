/**
 * wildcard.test.ts — positive-ref wildcard `_` and trailing-arg elision (v6/plans/
 * 2026-07-24-v6-dl-mvp-slice.md task 1.3): `Arg` now includes `Wild` for a plain
 * `RelRef`, not just a negated `NegRelRef`. A `Wild` in a positive ref matches any
 * value in that column and binds nothing ("don't project, don't consistency-check");
 * a body ref with fewer args than the rel's declared arity elides the missing
 * trailing columns as implicit wildcards (same join code path, ast.ts's `RelRef` doc).
 *
 * Style matches tests/lower/lower.test.ts: node:test, hand-built Program via ast.ts
 * constructors, ReplaySubject-injected sources, from-scratch reference expectations,
 * sortRows for order-independent comparison. This file owns only the wildcard-shaped
 * cases; the negation/join/recursion/aggregation golden suite stays in lower.test.ts.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { ReplaySubject } from "rxjs";
import type { Observable } from "rxjs";

import { edbRel, derivedRel, relRef, notRel, v, lit, wild, headVar, type Program } from "../../src/lower/ast.ts";
import { lowerProgram, type Row, type Sources } from "../../src/lower/lower.ts";

// =============================================================================
// Helpers (duplicated from lower.test.ts's local, unexported helpers — same shapes).
// =============================================================================

function collectSync<T>(obs: Observable<T>): T[] {
  const out: T[] = [];
  const sub = obs.subscribe((x) => out.push(x));
  sub.unsubscribe();
  return out;
}

function currentRows(obs: Observable<Row[]>): Row[] {
  const emissions = collectSync(obs);
  return emissions[emissions.length - 1] ?? [];
}

function sortRows(rows: readonly Row[]): Row[] {
  return rows
    .map((row) => [...row])
    .sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
}

function makeReplay(rows: Row[]): Observable<Row[]> {
  const subject = new ReplaySubject<Row[]>(1);
  subject.next(rows);
  return subject.asObservable();
}

// =============================================================================
// 1. A positive wildcard skips a column but a Lit in another column still selects.
// =============================================================================

test("wildcard: out(x) <- edge(x, _, 7) — wild skips a column, Lit 7 still selects", () => {
  const prog: Program = {
    rels: [edbRel("edge", ["a", "b", "c"]), derivedRel("out", ["x"])],
    rules: [
      {
        head: "out",
        headTerms: [headVar("x")],
        body: [relRef("edge", v("x"), wild(), lit(7))],
      },
    ],
  };
  const edge: Row[] = [
    [1, 9, 7],
    [2, 9, 8],
    [3, 5, 7],
  ];
  const expected: Row[] = [[1], [3]]; // middle column ignored; last column must equal 7

  const sources: Sources = new Map([["edge", makeReplay(edge)]]);
  const { rels, deferred } = lowerProgram(prog, sources);
  assert.deepStrictEqual(deferred, []);
  assert.deepStrictEqual(currentRows(rels.get("out")!), sortRows(expected));
});

// =============================================================================
// 2. Two wildcards in one ref are independently existential — no accidental equi-join.
// =============================================================================

test("wildcard: out(x) <- edge(_, _, x) — two wilds never bind, so they never compare", () => {
  const prog: Program = {
    rels: [edbRel("edge", ["a", "b", "c"]), derivedRel("out", ["x"])],
    rules: [
      {
        head: "out",
        headTerms: [headVar("x")],
        body: [relRef("edge", wild(), wild(), v("x"))],
      },
    ],
  };
  const edge: Row[] = [
    [1, 2, 3], // a !== b: would fail an (accidental) equi-join between the two wild columns
    [4, 4, 5], // a === b: passes either way — distinguishes "wild binds" from "wild doesn't"
  ];
  const expected: Row[] = [[3], [5]]; // both rows pass: the two `_` never bind or compare

  const sources: Sources = new Map([["edge", makeReplay(edge)]]);
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(currentRows(rels.get("out")!), sortRows(expected));
});

// =============================================================================
// 3. Trailing-arg elision: a relRef with fewer args than the rel's arity treats the
//    missing trailing positions as wildcards.
// =============================================================================

test("wildcard: trailing-arg elision — relRef with fewer args than arity behaves as trailing wildcards", () => {
  const prog: Program = {
    rels: [edbRel("edge", ["a", "b", "c"]), derivedRel("out", ["x"])],
    rules: [
      {
        head: "out",
        headTerms: [headVar("x")],
        // edge has arity 3; this ref supplies only 1 arg — columns b, c are elided.
        body: [relRef("edge", v("x"))],
      },
    ],
  };
  const edge: Row[] = [
    [1, 2, 3],
    [1, 9, 9], // same col0 as the row above, different b/c: elision must not distinguish these
    [2, 5, 5],
  ];
  // rel semantics is a SET: the two rows sharing col0=1 collapse into one output row
  // once b/c are elided — proves elision drops the whole column, not just its value.
  const expected: Row[] = [[1], [2]];

  const sources: Sources = new Map([["edge", makeReplay(edge)]]);
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(currentRows(rels.get("out")!), sortRows(expected));
});

// =============================================================================
// 4. Wild still legal in a negated ref (regression — this arc must not narrow that).
// =============================================================================

test("wildcard: !round(t, _) inside a negated ref still existentially quantifies (regression)", () => {
  // Mirrors lower.test.ts's leaf_round shape: leaf_round(t, r) <- tgt_round(t, r), !round(t, _).
  const prog: Program = {
    rels: [
      edbRel("round", ["f", "r"]),
      edbRel("tgt_round", ["t", "r"]),
      derivedRel("leaf_round", ["t", "r"]),
    ],
    rules: [
      {
        head: "leaf_round",
        headTerms: [headVar("t"), headVar("r")],
        body: [relRef("tgt_round", v("t"), v("r")), notRel("round", v("t"), wild())],
      },
    ],
  };
  const round: Row[] = [
    ["hub1", 0],
    ["hub2", 1],
  ];
  const tgtRound: Row[] = [
    ["hub1", 0],
    ["leafA", 2],
    ["leafB", 2],
    ["hub2", 1],
  ];
  const roundFs = new Set(round.map(([f]) => f));
  const expected: Row[] = tgtRound.filter(([t]) => !roundFs.has(t)); // {leafA, leafB}

  const sources: Sources = new Map([
    ["round", makeReplay(round)],
    ["tgt_round", makeReplay(tgtRound)],
  ]);
  const { rels } = lowerProgram(prog, sources);
  assert.deepStrictEqual(currentRows(rels.get("leaf_round")!), sortRows(expected));
});

// =============================================================================
// 5. A wildcard never reaches the head: projection stays correct when a rule mixes
//    wild positions (never projected) with bound positions (projected, no undefined
//    columns).
// =============================================================================

test("wildcard: never reaches the head — projection is correct alongside wild positions", () => {
  // watch_label(ep, name) <- watch(ep, _), label(ep, name, _).
  // Every head column (ep, name) comes from a NON-wild position; the wild positions
  // (watch's 2nd column, label's 3rd column) must not leak into the projected rows.
  const prog: Program = {
    rels: [
      edbRel("watch", ["ep", "junk"]),
      edbRel("label", ["ep", "name", "junk2"]),
      derivedRel("watch_label", ["ep", "name"]),
    ],
    rules: [
      {
        head: "watch_label",
        headTerms: [headVar("ep"), headVar("name")],
        body: [relRef("watch", v("ep"), wild()), relRef("label", v("ep"), v("name"), wild())],
      },
    ],
  };
  const watch: Row[] = [
    ["a", "ignored1"],
    ["b", "ignored2"],
  ];
  const label: Row[] = [
    ["a", "Alpha", "ignored3"],
    ["b", "Beta", "ignored4"],
    ["c", "Gamma", "ignored5"], // no watch("c", _): must not appear in output
  ];
  const expected: Row[] = [
    ["a", "Alpha"],
    ["b", "Beta"],
  ];

  const sources: Sources = new Map([
    ["watch", makeReplay(watch)],
    ["label", makeReplay(label)],
  ]);
  const { rels } = lowerProgram(prog, sources);
  const got = currentRows(rels.get("watch_label")!);
  // exactly 2 columns per row (no leaked wild/junk column), and matches the expected set.
  for (const row of got) assert.strictEqual(row.length, 2, "wild columns never reach the head");
  assert.deepStrictEqual(got, sortRows(expected));
});
