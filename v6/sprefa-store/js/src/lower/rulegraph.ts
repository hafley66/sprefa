/**
 * rulegraph.ts — the static rule dependency graph + stratification.
 *
 * Pure functions over a named-node adjacency list. NO rxjs, NO SQLite, NO engine
 * types — a generic, reusable graph layer (part of the json-rx spec). The three
 * exports are `buildRuleGraph` → `scc` (Tarjan) → `stratify` (condense + topo-sort).
 *
 * Mirrors v5's approach (src/engine/strata.rs + src/graph/scc.rs): recursive rules
 * are stratified by SCC before any fixpoint. Here the fixpoint is deferred (see
 * lower.ts `RecursiveStratumDeferred`), so stratify only needs to ORDER the acyclic
 * strata and MARK the recursive ones. The Tarjan is the iterative form proven in
 * tests/golden.test.ts `ref_oracle.tarjan` (itself ported from src/graph/scc.rs).
 *
 * Stratified negation adds polarity: a `!rel(args)` body predicate (v5 surface
 * spelling; src/ast.rs:370 `BodyItem::Neg`) is STILL a dependency edge (the negated
 * rel must be evaluated first) but marked negative in `Graph.negAdj` — v5's "forcing
 * edge" (src/typecheck.rs:1185). `stratify` refuses a program where a negative edge's
 * two endpoints share an SCC — the negated rel could never be "already complete"
 * relative to its own cycle — by throwing `NonStratifiableError`, matching v5's
 * diagnostic wording (src/typecheck.rs:1201).
 */

import type { Program } from "./ast.ts";

// ─────────────────────────────────────────────────────────────────────────────
// The graph: dense-indexed named nodes + deduped, sorted out-edges.
// ─────────────────────────────────────────────────────────────────────────────
// Edge direction = DEPENDENCY: an edge head -> body-read means "head depends on
// body" (head's rule reads body in its body). Evaluation needs body first.
// ─────────────────────────────────────────────────────────────────────────────

export interface Graph {
  /** Dense index -> node name. */
  readonly nodes: readonly string[];
  /** adj[i] = out-edges from node i (the rels node i depends on), deduped + sorted ascending. */
  readonly adj: readonly (readonly number[])[];
  /** negAdj[i] = the subset of adj[i] that came from a `!rel(args)` body predicate.
   *  Optional so pre-negation Graph literals stay valid; `buildRuleGraph` always fills
   *  it (with empty arrays when a program has no negation). */
  readonly negAdj?: readonly (readonly number[])[];
}

/**
 * Build the rule dependency graph from a program: edge `head_rel -> each rel read
 * in its body`. Works for higher-order maps — a rel reading a derived rel is just
 * another edge. Node indexing: declared rels in declaration order, then any body-
 * referenced-but-undeclared rel in first-seen order. A `!rel(args)` body predicate
 * still contributes the dependency edge (evaluation order needs it first) AND marks
 * it negative in `negAdj` for the stratifiability check below.
 */
export function buildRuleGraph(prog: Program): Graph {
  const id = new Map<string, number>();
  const names: string[] = [];
  const intern = (n: string): number => {
    const existing = id.get(n);
    if (existing !== undefined) return existing;
    const i = names.length;
    names.push(n);
    id.set(n, i);
    return i;
  };
  for (const d of prog.rels) intern(d.name);
  // adjSets/negAdjSets grow lazily so an undeclared head/body rel still gets a slot.
  const adjSets: Set<number>[] = names.map(() => new Set<number>());
  const negAdjSets: Set<number>[] = names.map(() => new Set<number>());
  const ensure = (n: string): number => {
    const i = intern(n);
    while (adjSets.length <= i) adjSets.push(new Set<number>());
    while (negAdjSets.length <= i) negAdjSets.push(new Set<number>());
    return i;
  };
  for (const rule of prog.rules) {
    const h = ensure(rule.head);
    for (const p of rule.body) {
      if (p.kind === "rel") {
        adjSets[h]!.add(ensure(p.rel));
      } else if (p.kind === "notrel") {
        const w = ensure(p.rel);
        adjSets[h]!.add(w); // the dependency exists regardless of polarity
        negAdjSets[h]!.add(w); // ... but this one is negative
      }
    }
  }
  const adj = adjSets.map((s) => [...s].sort((a, b) => a - b));
  const negAdj = negAdjSets.map((s) => [...s].sort((a, b) => a - b));
  return { nodes: names, adj, negAdj };
}

// ─────────────────────────────────────────────────────────────────────────────
// Tarjan SCC — iterative, byte-for-byte the golden.test.ts `ref_oracle.tarjan`
// reference (ported from src/graph/scc.rs). Returns (comp per node, #components).
// ─────────────────────────────────────────────────────────────────────────────

export function scc(graph: Graph): { comp: number[]; ncomp: number } {
  const adj = graph.adj;
  const n = adj.length;
  const index = new Array<number>(n).fill(-1);
  const low = new Array<number>(n).fill(0);
  const onStack = new Array<boolean>(n).fill(false);
  const comp = new Array<number>(n).fill(-1);
  const stack: number[] = [];
  let idx = 0;
  let ncomp = 0;
  for (let start = 0; start < n; start++) {
    if (index[start] !== -1) continue;
    const work: [number, number][] = [[start, 0]];
    while (work.length > 0) {
      const top = work[work.length - 1]!;
      const v = top[0];
      const ci = top[1];
      if (ci === 0) {
        index[v] = idx;
        low[v] = idx;
        idx++;
        stack.push(v);
        onStack[v] = true;
      }
      if (ci < adj[v]!.length) {
        top[1] += 1;
        const w = adj[v]![ci]!;
        if (index[w] === -1) {
          work.push([w, 0]);
        } else if (onStack[w] && index[w]! < low[v]!) {
          low[v] = index[w]!;
        }
      } else {
        work.pop();
        const parent = work[work.length - 1];
        if (parent !== undefined && low[v]! < low[parent[0]!]!) {
          low[parent[0]!] = low[v]!;
        }
        if (low[v] === index[v]) {
          // eslint-disable-next-line no-constant-condition
          while (true) {
            const w = stack.pop()!;
            onStack[w] = false;
            comp[w] = ncomp;
            if (w === v) break;
          }
          ncomp++;
        }
      }
    }
  }
  return { comp, ncomp };
}

// ─────────────────────────────────────────────────────────────────────────────
// Stratify: condense SCCs, topo-sort the condensation (dependencies first), mark
// recursive strata. A stratum is one SCC: either a single acyclic rel (size 1, no
// self-loop) or a recursive group (size > 1, or a self-loop).
// ─────────────────────────────────────────────────────────────────────────────

export interface Stratum {
  /** Member rel names. */
  readonly rels: readonly string[];
  /** True iff the SCC is recursive (size > 1, or a self-loop on a singleton). */
  readonly recursive: boolean;
  /** Topo position: 0 = first (no dependencies). Dependencies always precede dependents. */
  readonly order: number;
}

/**
 * A program is not stratifiable: a `!rel(args)` body predicate's target rel shares an
 * SCC with the rule reading it. Message wording matches v5's diagnostic exactly
 * (src/typecheck.rs:1201, `not-stratified`): "relation `{rel}` is aggregated or negated
 * inside a recursive cycle with `{cycleWith}`" — `rel` is the negated (body-side) rel,
 * `cycleWith` is the head rel whose rule reads it.
 */
export class NonStratifiableError extends Error {
  /** The negated rel (v5's `b` / body side of the forcing edge). */
  readonly rel: string;
  /** The head rel sharing the cycle with `rel` (v5's `h`). */
  readonly cycleWith: string;
  constructor(rel: string, cycleWith: string) {
    super(`relation \`${rel}\` is aggregated or negated inside a recursive cycle with \`${cycleWith}\``);
    this.name = "NonStratifiableError";
    this.rel = rel;
    this.cycleWith = cycleWith;
  }
}

/**
 * Condense the SCCs and topo-sort the condensation so dependencies come first.
 * Deterministic: among ready components (all deps emitted), picks the one whose
 * member set has the smallest min index — stable under reordering of equal-rank
 * components. Returns one `Stratum` per SCC, in evaluation order.
 *
 * Throws `NonStratifiableError` if any `negAdj` edge's two endpoints share an SCC —
 * checked before the condensation/topo-sort work below, since an illegal program has
 * no valid evaluation order to compute.
 */
export function stratify(
  graph: Graph,
  sccs: { comp: number[]; ncomp: number },
): Stratum[] {
  const { comp, ncomp } = sccs;
  const adj = graph.adj;

  const members: number[][] = Array.from({ length: ncomp }, () => []);
  for (let node = 0; node < comp.length; node++) members[comp[node]!]!.push(node);

  if (graph.negAdj) {
    for (let head = 0; head < graph.negAdj.length; head++) {
      for (const negated of graph.negAdj[head]!) {
        if (comp[head] === comp[negated]) {
          throw new NonStratifiableError(graph.nodes[negated]!, graph.nodes[head]!);
        }
      }
    }
  }

  // cyclic iff SCC size > 1 OR a self-loop on the singleton (Tarjan alone misses self-loops).
  const cyclic = new Array<boolean>(ncomp).fill(false);
  for (let c = 0; c < ncomp; c++) if (members[c]!.length > 1) cyclic[c] = true;
  for (let u = 0; u < adj.length; u++) {
    for (const w of adj[u]!) {
      if (w === u) cyclic[comp[u]!] = true;
    }
  }

  // condensation out-edges (deduped): cadj[c] = components c DEPENDS ON.
  const cadjSets: Set<number>[] = Array.from({ length: ncomp }, () => new Set<number>());
  for (let u = 0; u < adj.length; u++) {
    const cu = comp[u]!;
    for (const w of adj[u]!) {
      const cw = comp[w]!;
      if (cu !== cw) cadjSets[cu]!.add(cw);
    }
  }
  // reverse adjacency: who depends on c.
  const dependents: number[][] = Array.from({ length: ncomp }, () => []);
  for (let c = 0; c < ncomp; c++) for (const d of cadjSets[c]!) dependents[d]!.push(c);

  // Kahn: a component is ready when every component it depends on is emitted.
  const remaining = cadjSets.map((s) => s.size);
  const ready: number[] = [];
  for (let c = 0; c < ncomp; c++) if (remaining[c] === 0) ready.push(c);

  const minMember = (c: number): number => {
    let m = members[c]![0]!;
    for (const x of members[c]!) if (x < m) m = x;
    return m;
  };

  const order: number[] = [];
  while (ready.length > 0) {
    // deterministic tie-break: smallest min-member among ready components.
    let bestIdx = 0;
    for (let i = 1; i < ready.length; i++) {
      if (minMember(ready[i]!) < minMember(ready[bestIdx]!)) bestIdx = i;
    }
    const c = ready.splice(bestIdx, 1)[0]!;
    order.push(c);
    for (const dep of dependents[c]!) {
      remaining[dep]! -= 1;
      if (remaining[dep] === 0) ready.push(dep);
    }
  }

  return order.map((c, i) => ({
    rels: members[c]!.map((m) => graph.nodes[m]!),
    recursive: cyclic[c]!,
    order: i,
  }));
}
