/**
 * fixpoint.ts — the LAB (datalog half). The datalog least-fixpoint written THREE ways,
 * all golden-equal, so the differences are MEASURED, not asserted: LOC, RAM, and where
 * async fits. This is the `oracle.ts` of the recursive arc — the raw mechanism ported
 * straight, not a dl→rxjs compiler (that is lower.ts / Epic 1). Prolog (SLG + SLD) is
 * the study sibling in `prolog.ts`, which imports this file's shared term/unify core.
 *
 * ── THE OPERATOR-LAYER VERDICT (why not "just expand") ──────────────────────────
 * A fixpoint is FEEDBACK + ACCUMULATION + TERMINATION. In rxjs only ONE stdlib operator
 * has self-feedback (`expand`); `scan`/`reduce`/`mergeScan`/`switchScan` fold an EXTERNAL
 * source and cannot drive a loop. But the semi-naive inner loop is synchronous and
 * CPU-bound — it uses none of `expand`'s async/backpressure payoff — so over a sync hop,
 * `expand` is a fancy `while`. The honest split:
 *   inner loop, heavy    → SQL `INSERT…SELECT` loop (sets never enter JS heap)   [way 3]
 *   inner loop, bounded  → plain `while` (the mechanism)                          [way 1]
 *                          OR `expand` — earns its keep ONLY if the hop is async  [way 2 + 2b]
 *   accumulate deltas    → `scan` / `reduce`   (the closure = ∪ of the deltas)
 *   demand / cancel-stale→ `switchMap` / `switchScan`  (the QUERY layer, see prolog.ts)
 *
 * ── THE THREE WAYS (this file) ──────────────────────────────────────────────────
 *   1. datalogWhile     — plain sync loop. The mechanism the Rust cascade runs.
 *   2. datalogExpand     — rxjs `expand` + seen-set. Same `semiNaiveRound` hop; the
 *                          emitted batches are DISJOINT deltas whose union is the closure
 *                          (this is the SET-vs-ELEMENT evidence: the stream grain is the
 *                          DELTA, so recursive rels lower delta/element-semantic).
 *   2b. datalogExpandAsync — identical to 2 with an ASYNC hop (`from(Promise…)`). ZERO
 *                          structural change vs 2 — that is exactly where `expand` beats
 *                          `while`: a sync loop would need manual Promise orchestration.
 *   3. datalogSqlClosure — delegate the fixpoint to SQLite's delta loop. The sets stay on
 *                          disk; JS heap holds only the final read. The RAM thesis, literal.
 *
 * Pure w.r.t. the caller: no shared globals; way 3 opens its own :memory: db and closes it.
 * rxjs (of/from/EMPTY/expand) + better-sqlite3 are the only deps.
 */

import { EMPTY, of, from, expand, map, mergeMap, type Observable } from "rxjs";
import Database from "better-sqlite3";

// ═════════════════════════════════════════════════════════════════════════════
// SHARED CORE — terms, atoms, rules, facts + unification. `prolog.ts` imports these.
// Function-free (datalog-restricted): the Herbrand base is finite over a finite
// constant set, so both bottom-up closure and top-down tabling terminate.
// ═════════════════════════════════════════════════════════════════════════════

export type Value = string | number;

/** A ground fact: predicate + tuple of constant values. */
export interface Fact {
  readonly pred: string;
  readonly args: readonly Value[];
}

/** A rule/query variable. */
export interface Var {
  readonly kind: "var";
  readonly name: string;
}
/** A constant term. */
export interface Const {
  readonly kind: "const";
  readonly value: Value;
}
export type Term = Var | Const;

/** An atom: predicate applied to terms (vars in rules/queries; consts in call patterns). */
export interface Atom {
  readonly pred: string;
  readonly args: readonly Term[];
}

/** A rule `head :- body` (body = conjunction). Range-restricted: every head var is in the body. */
export interface Rule {
  readonly head: Atom;
  readonly body: readonly Atom[];
}

// Constructors (terse, typo-resistant — same posture as ast.ts).
export const vr = (name: string): Var => ({ kind: "var", name });
export const con = (value: Value): Const => ({ kind: "const", value });
export const atom = (pred: string, ...args: readonly Term[]): Atom => ({ pred, args });
export const rule = (head: Atom, ...body: readonly Atom[]): Rule => ({ head, body });
export const fact = (pred: string, ...args: readonly Value[]): Fact => ({ pred, args });

/** Stable identity of a ground fact (the dedup / seen-set key). */
export const factKey = (f: Fact): string => `${f.pred}(${f.args.join("")})`;

/** A substitution: variable name → bound constant. */
export type Subst = ReadonlyMap<string, Value>;

/** Match an atom (vars/consts) against a GROUND fact under `s` → extended subst, or null. */
export function matchFact(a: Atom, f: Fact, s: Subst): Subst | null {
  if (a.pred !== f.pred || a.args.length !== f.args.length) return null;
  const next = new Map(s);
  for (let i = 0; i < a.args.length; i++) {
    const t = a.args[i]!;
    const val = f.args[i]!;
    if (t.kind === "const") {
      if (t.value !== val) return null;
    } else {
      const bound = next.get(t.name);
      if (bound !== undefined) {
        if (bound !== val) return null; // shared-var consistency (the equi-join key)
      } else {
        next.set(t.name, val);
      }
    }
  }
  return next;
}

/** Instantiate a head atom to a ground fact under a subst that binds all its vars. */
export function instantiate(a: Atom, s: Subst): Fact {
  return { pred: a.pred, args: a.args.map((t) => (t.kind === "const" ? t.value : s.get(t.name)!)) };
}

/** Apply a subst to an atom: bound vars become consts; unbound vars stay. */
export function applySubst(a: Atom, s: Subst): Atom {
  return {
    pred: a.pred,
    args: a.args.map((t) => (t.kind === "var" && s.has(t.name) ? con(s.get(t.name)!) : t)),
  };
}

// ═════════════════════════════════════════════════════════════════════════════
// The ONE hop — semi-naive round. Shared by way 1, 2, 2b (way 3 is SQL's own loop).
// ═════════════════════════════════════════════════════════════════════════════

function indexByPred(facts: Iterable<Fact>): Map<string, Fact[]> {
  const m = new Map<string, Fact[]>();
  for (const f of facts) {
    const l = m.get(f.pred);
    if (l) l.push(f);
    else m.set(f.pred, [f]);
  }
  return m;
}

/**
 * One semi-naive round: every rule joined with AT LEAST ONE body atom drawn from `delta`
 * (the previous round's new facts); the rest from `allByPred` (which the caller has already
 * grown to include `delta`). So a fact is produced exactly in the round it first becomes
 * derivable, never re-derived. Pure: reads both maps, mutates neither.
 */
export function semiNaiveRound(
  rules: readonly Rule[],
  allByPred: ReadonlyMap<string, Fact[]>,
  deltaByPred: ReadonlyMap<string, Fact[]>,
): Fact[] {
  const factsOf = (p: string): readonly Fact[] => allByPred.get(p) ?? [];
  const out: Fact[] = [];
  for (const r of rules) {
    for (let dpos = 0; dpos < r.body.length; dpos++) {
      if (!deltaByPred.get(r.body[dpos]!.pred)?.length) continue; // atom dpos has no new fact
      let substs: Subst[] = [new Map<string, Value>()];
      for (let i = 0; i < r.body.length && substs.length; i++) {
        const b = r.body[i]!;
        const src = i === dpos ? (deltaByPred.get(b.pred) ?? []) : factsOf(b.pred);
        const nextS: Subst[] = [];
        for (const s of substs) {
          for (const f of src) {
            const m = matchFact(b, f, s);
            if (m) nextS.push(m);
          }
        }
        substs = nextS;
      }
      for (const s of substs) out.push(instantiate(r.head, s));
    }
  }
  return out;
}

// ═════════════════════════════════════════════════════════════════════════════
// WAY 1 — plain `while`. The mechanism. What the Rust cascade / SQL loop actually is.
// ═════════════════════════════════════════════════════════════════════════════

export function datalogWhile(edb: readonly Fact[], rules: readonly Rule[]): Fact[] {
  const all = new Map<string, Fact>(); // factKey → Fact (the closure)
  const allByPred = new Map<string, Fact[]>();
  const admit = (f: Fact): boolean => {
    const k = factKey(f);
    if (all.has(k)) return false;
    all.set(k, f);
    const l = allByPred.get(f.pred);
    if (l) l.push(f);
    else allByPred.set(f.pred, [f]);
    return true;
  };

  let delta: Fact[] = [];
  for (const f of edb) if (admit(f)) delta.push(f);
  while (delta.length) {
    const produced = semiNaiveRound(rules, allByPred, indexByPred(delta));
    const nextDelta: Fact[] = [];
    for (const f of produced) if (admit(f)) nextDelta.push(f);
    delta = nextDelta;
  }
  return [...all.values()];
}

// ═════════════════════════════════════════════════════════════════════════════
// WAY 2 — rxjs `expand` + seen-set. The self-driving Observable form. Emits DISJOINT
// delta batches (the set-vs-element evidence). Same `semiNaiveRound` hop as way 1.
// ═════════════════════════════════════════════════════════════════════════════

/**
 * Drive a synchronous hop to quiescence via `expand`. Each emission is a DELTA batch of
 * NOVEL facts (deduped by `factKey`); an empty novel batch → EMPTY, which STOPS `expand`
 * = the least fixpoint. Synchronous: `of`/`expand`/`EMPTY` exhaust in one stack frame.
 */
export function datalogExpand(edb: readonly Fact[], rules: readonly Rule[]): Observable<Fact[]> {
  const seen = new Set<string>();
  const allByPred = new Map<string, Fact[]>();
  const admit = (items: readonly Fact[]): Fact[] => {
    const out: Fact[] = [];
    for (const f of items) {
      const k = factKey(f);
      if (!seen.has(k)) {
        seen.add(k);
        out.push(f);
        const l = allByPred.get(f.pred); // grow the full set so the round's "rest" reads it
        if (l) l.push(f);
        else allByPred.set(f.pred, [f]);
      }
    }
    return out;
  };
  const hop = (delta: readonly Fact[]): Fact[] =>
    semiNaiveRound(rules, allByPred, indexByPred(delta));

  const seed0 = admit(edb);
  return of(seed0).pipe(
    expand((delta) => {
      const novel = admit(hop(delta));
      return novel.length ? of(novel) : EMPTY; // EMPTY → expand stops = quiescence
    }),
  );
}

/** WAY 2b — the SAME as way 2 with an ASYNC hop. The only change is `of` → `from(Promise…)`;
 *  the structure is untouched. This is precisely where `expand` beats `while`: a sync loop
 *  would need manual Promise orchestration + a re-feed queue; `expand` already is one. */
export function datalogExpandAsync(edb: readonly Fact[], rules: readonly Rule[]): Observable<Fact[]> {
  const seen = new Set<string>();
  const allByPred = new Map<string, Fact[]>();
  const admit = (items: readonly Fact[]): Fact[] => {
    const out: Fact[] = [];
    for (const f of items) {
      const k = factKey(f);
      if (!seen.has(k)) {
        seen.add(k);
        out.push(f);
        const l = allByPred.get(f.pred);
        if (l) l.push(f);
        else allByPred.set(f.pred, [f]);
      }
    }
    return out;
  };
  const hop = (delta: readonly Fact[]): Fact[] =>
    semiNaiveRound(rules, allByPred, indexByPred(delta));

  const seed0 = admit(edb);
  return of(seed0).pipe(
    expand((delta) =>
      from(Promise.resolve(delta)).pipe( // ← async boundary; the only difference vs way 2
        map((d) => admit(hop(d))),
        mergeMap((novel) => (novel.length ? of(novel) : EMPTY)),
      ),
    ),
  );
}

/** Collect way 2's emitted delta batches (sync exhaust). */
export function datalogExpandDeltas(edb: readonly Fact[], rules: readonly Rule[]): Fact[][] {
  const batches: Fact[][] = [];
  datalogExpand(edb, rules).subscribe((b) => batches.push(b));
  return batches;
}

/** Way 2's full closure (all batches flattened). */
export function datalogExpandClosure(edb: readonly Fact[], rules: readonly Rule[]): Fact[] {
  return datalogExpandDeltas(edb, rules).flat();
}

// ═════════════════════════════════════════════════════════════════════════════
// WAY 3 — delegate to SQLite. The delta loop runs IN the db; JS heap holds only the
// final read. Specialized to transitive closure (edge → path): the GENERIC rule→SQL
// is the lowering (Epic 1/4). Here it is hand-lowered, to measure the delegate path.
// ═════════════════════════════════════════════════════════════════════════════

export function datalogSqlClosure(edges: readonly (readonly [Value, Value])[]): Fact[] {
  const db = new Database(":memory:");
  try {
    db.exec(
      "CREATE TABLE edge(x, y);" +
        "CREATE TABLE path(x, y, PRIMARY KEY(x, y)) WITHOUT ROWID;",
    );
    const ins = db.prepare("INSERT INTO edge(x, y) VALUES (?, ?)");
    const load = db.transaction((es: readonly (readonly [Value, Value])[]) => {
      for (const [x, y] of es) ins.run(x, y);
    });
    load(edges);

    db.exec("INSERT OR IGNORE INTO path SELECT x, y FROM edge"); // path(X,Y) :- edge(X,Y)
    // path(X,Z) :- path(X,Y), edge(Y,Z). The cascade's delta loop: repeat the JOIN until a
    // pass inserts 0 rows = least fixpoint. The row sets live in SQLite, never in JS heap.
    const step = db.prepare("INSERT OR IGNORE INTO path SELECT p.x, e.y FROM path p JOIN edge e ON p.y = e.x");
    while (step.run().changes > 0) {
      /* keep going: each pass adds one more hop of reach */
    }

    const rows = db.prepare("SELECT x, y FROM path").all() as { x: Value; y: Value }[];
    return rows.map((r) => fact("path", r.x, r.y));
  } finally {
    db.close();
  }
}
