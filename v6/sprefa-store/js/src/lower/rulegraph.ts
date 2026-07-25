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
import type { Stratum, Graph } from "./types.ts";

// ─────────────────────────────────────────────────────────────────────────────
// The graph: dense-indexed named nodes + deduped, sorted out-edges.
// ─────────────────────────────────────────────────────────────────────────────
// Edge direction = DEPENDENCY: an edge head -> body-read means "head depends on
// body" (head's rule reads body in its body). Evaluation needs body first.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Build the rule dependency graph from a program: edge `head_rel -> each rel read
 * in its body`. Works for higher-order maps — a rel reading a derived rel is just
 * another edge. Node indexing: declared rels in declaration order, then any body-
 * referenced-but-undeclared rel in first-seen order. A `!rel(args)` body predicate
 * still contributes the dependency edge (evaluation order needs it first) AND marks
 * it negative in `negAdj` for the stratifiability check below.
 */
export function buildRuleGraph(prog: Program): Graph {
  const nameToIndex = new Map<string, number>();
  const names: string[] = [];
  const intern = (name: string): number => {
    const existing = nameToIndex.get(name);
    if (existing !== undefined) return existing;
    const index = names.length;
    names.push(name);
    nameToIndex.set(name, index);
    return index;
  };
  for (const declaration of prog.rels) intern(declaration.name);
  // adjSets/negAdjSets grow lazily so an undeclared head/body rel still gets a slot.
  const adjSets: Set<number>[] = names.map(() => new Set<number>());
  const negAdjSets: Set<number>[] = names.map(() => new Set<number>());
  const ensure = (name: string): number => {
    const index = intern(name);
    while (adjSets.length <= index) adjSets.push(new Set<number>());
    while (negAdjSets.length <= index) negAdjSets.push(new Set<number>());
    return index;
  };
  for (const rule of prog.rules) {
    const headIndex = ensure(rule.head);
    for (const predicate of rule.body) {
      if (predicate.kind === "rel") {
        adjSets[headIndex]!.add(ensure(predicate.rel));
      } else if (predicate.kind === "notrel") {
        const negRelIndex = ensure(predicate.rel);
        adjSets[headIndex]!.add(negRelIndex); // the dependency exists regardless of polarity
        negAdjSets[headIndex]!.add(negRelIndex); // ... but this one is negative
      }
    }
  }
  const adj = adjSets.map((indexSet) => [...indexSet].sort((a, b) => a - b));
  const negAdj = negAdjSets.map((indexSet) => [...indexSet].sort((a, b) => a - b));
  return { nodes: names, adj, negAdj };
}

// ─────────────────────────────────────────────────────────────────────────────
// Tarjan SCC — iterative, byte-for-byte the golden.test.ts `ref_oracle.tarjan`
// reference (ported from src/graph/scc.rs). Returns (comp per node, #components).
// ─────────────────────────────────────────────────────────────────────────────

export function scc(graph: Graph): { comp: number[]; ncomp: number } {
  const adj = graph.adj;
  const nodeCount = adj.length;
  const index = new Array<number>(nodeCount).fill(-1);
  const low = new Array<number>(nodeCount).fill(0);
  const onStack = new Array<boolean>(nodeCount).fill(false);
  const comp = new Array<number>(nodeCount).fill(-1);
  const stack: number[] = [];
  let indexCounter = 0;
  let ncomp = 0;
  for (let start = 0; start < nodeCount; start++) {
    if (index[start] !== -1) continue;
    const work: [number, number][] = [[start, 0]];
    while (work.length > 0) {
      const top = work[work.length - 1]!;
      const node = top[0];
      const edgeIndex = top[1];
      if (edgeIndex === 0) {
        index[node] = indexCounter;
        low[node] = indexCounter;
        indexCounter++;
        stack.push(node);
        onStack[node] = true;
      }
      if (edgeIndex < adj[node]!.length) {
        top[1] += 1;
        const neighbor = adj[node]![edgeIndex]!;
        if (index[neighbor] === -1) {
          work.push([neighbor, 0]);
        } else if (onStack[neighbor] && index[neighbor]! < low[node]!) {
          low[node] = index[neighbor]!;
        }
      } else {
        work.pop();
        const parent = work[work.length - 1];
        if (parent !== undefined && low[node]! < low[parent[0]!]!) {
          low[parent[0]!] = low[node]!;
        }
        if (low[node] === index[node]) {
          // eslint-disable-next-line no-constant-condition
          while (true) {
            const neighbor = stack.pop()!;
            onStack[neighbor] = false;
            comp[neighbor] = ncomp;
            if (neighbor === node) break;
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
  for (let nodeIndex = 0; nodeIndex < comp.length; nodeIndex++) members[comp[nodeIndex]!]!.push(nodeIndex);

  if (graph.negAdj) {
    for (let headIndex = 0; headIndex < graph.negAdj.length; headIndex++) {
      for (const negatedIndex of graph.negAdj[headIndex]!) {
        if (comp[headIndex] === comp[negatedIndex]) {
          throw new NonStratifiableError(graph.nodes[negatedIndex]!, graph.nodes[headIndex]!);
        }
      }
    }
  }

  // cyclic iff SCC size > 1 OR a self-loop on the singleton (Tarjan alone misses self-loops).
  const cyclic = new Array<boolean>(ncomp).fill(false);
  for (let componentIndex = 0; componentIndex < ncomp; componentIndex++) if (members[componentIndex]!.length > 1) cyclic[componentIndex] = true;
  for (let nodeIndex = 0; nodeIndex < adj.length; nodeIndex++) {
    for (const neighbor of adj[nodeIndex]!) {
      if (neighbor === nodeIndex) cyclic[comp[nodeIndex]!] = true;
    }
  }

  // condensation out-edges (deduped): cadj[c] = components c DEPENDS ON.
  const cadjSets: Set<number>[] = Array.from({ length: ncomp }, () => new Set<number>());
  for (let nodeIndex = 0; nodeIndex < adj.length; nodeIndex++) {
    const nodeComponent = comp[nodeIndex]!;
    for (const neighbor of adj[nodeIndex]!) {
      const neighborComponent = comp[neighbor]!;
      if (nodeComponent !== neighborComponent) cadjSets[nodeComponent]!.add(neighborComponent);
    }
  }
  // reverse adjacency: who depends on c.
  const dependents: number[][] = Array.from({ length: ncomp }, () => []);
  for (let componentIndex = 0; componentIndex < ncomp; componentIndex++) for (const dependent of cadjSets[componentIndex]!) dependents[dependent]!.push(componentIndex);

  // Kahn: a component is ready when every component it depends on is emitted.
  const remaining = cadjSets.map((indexSet) => indexSet.size);
  const ready: number[] = [];
  for (let componentIndex = 0; componentIndex < ncomp; componentIndex++) if (remaining[componentIndex] === 0) ready.push(componentIndex);

  const minMember = (componentIndex: number): number => {
    let minimumMember = members[componentIndex]![0]!;
    for (const memberIndex of members[componentIndex]!) if (memberIndex < minimumMember) minimumMember = memberIndex;
    return minimumMember;
  };

  const order: number[] = [];
  while (ready.length > 0) {
    // deterministic tie-break: smallest min-member among ready components.
    let bestIndex = 0;
    for (let index = 1; index < ready.length; index++) {
      if (minMember(ready[index]!) < minMember(ready[bestIndex]!)) bestIndex = index;
    }
    const componentIndex = ready.splice(bestIndex, 1)[0]!;
    order.push(componentIndex);
    for (const dependent of dependents[componentIndex]!) {
      remaining[dependent]! -= 1;
      if (remaining[dependent] === 0) ready.push(dependent);
    }
  }

  return order.map((componentIndex, stratumIndex) => ({
    rels: members[componentIndex]!.map((nodeIndex) => graph.nodes[nodeIndex]!),
    recursive: cyclic[componentIndex]!,
    order: stratumIndex,
  }));
}
