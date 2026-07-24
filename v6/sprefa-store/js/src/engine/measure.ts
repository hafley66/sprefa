/**
 * measure.ts — the measurement harness helpers, ported from src/measure.rs.
 *
 * Ports: `benchgraph` (the deterministic layered DAG generator + the independent
 * `oracle_survivors` from-scratch BFS referee) and `memcap` (a soft RSS guard).
 *
 * DIVERGENCE (documented): Rust reads peak RSS via `getrusage` and enforces a hard address-
 * space cap via a counting `#[global_allocator]` (CappedAlloc). Node can do neither:
 * `process.memoryUsage().rss` is the analog peak sampler, and Node cannot `setrlimit` its
 * own address space. So memcap here is a SOFT guard: it records the peak sampled RSS and
 * optionally throws when a sample exceeds the budget. The hard OS cap stays Rust-side; this
 * only confirms the TS process stays RAM-bounded (SQLite owns the data, the thesis).
 */

import process from "node:process";

// =============================================================================
// benchgraph — one deterministic DAG generator shared by both sides of the head-to-head
// =============================================================================
export namespace benchgraph {
//! Nodes 0 and 1 are roots (no parents); every other node has mixed support so retracting
//! root 0 leaves a non-trivial subset alive. A multi-relation reference graph: THREE logical
//! relations so the polymorphic `(tag, id)` key is load-bearing.

/** `parents[node]` = the parent GLOBAL node ids. Nodes 0 and 1 are roots. */
export function gen(layers: number, width: number): number[][] {
  const n = 2 + layers * width;
  const parents: number[][] = Array.from({ length: n }, () => []);
  for (let l = 0; l < layers; l++) {
    for (let w = 0; w < width; w++) {
      const id = 2 + l * width + w;
      if (l === 0) {
        parents[id]!.push(0);
        if (w % 3 === 0) parents[id]!.push(1);
      } else {
        const prev = 2 + (l - 1) * width;
        parents[id]!.push(prev + w);
        parents[id]!.push(prev + ((w + 1) % width));
      }
    }
  }
  return parents;
}

/** A multi-relation reference graph. tag 0 = modules, 1 = functions, 2 = types. */
export interface MultiGraph {
  /** (tag, id, weight) */
  rows: [number, number, number][];
  /** (parent_tag, parent_id, child_tag, child_id) */
  edges: [number, number, number, number][];
  /** the retract target (a root in relation 0). */
  seed: [number, number];
  /** rows per relation, index = tag. */
  per_tag: [number, number, number];
}

/** The proven layered DAG, tiered into THREE relations so `(tag, id)` is load-bearing. */
export function gen_multi(layers: number, width: number): MultiGraph {
  const parents = gen(layers, width);
  const n = parents.length;

  const tier = (g: number): number => (g < 2 ? 0 : 1 + Math.floor((g - 2) / width));
  const tag_of = (g: number): number => tier(g) % 3;

  // Assign a per-relation local id to every global node, in global order.
  const local = new Array<number>(n).fill(0);
  const per_tag: number[] = [0, 0, 0];
  for (let g = 0; g < n; g++) {
    const t = tag_of(g);
    local[g] = per_tag[t]!;
    per_tag[t]! += 1;
  }

  const rows: [number, number, number][] = [];
  const edges: [number, number, number, number][] = [];
  for (let g = 0; g < n; g++) {
    const w = parents[g]!.length === 0 ? 1 : parents[g]!.length;
    rows.push([tag_of(g), local[g]!, w]);
    for (const p of parents[g]!) {
      edges.push([tag_of(p), local[p]!, tag_of(g), local[g]!]);
    }
  }

  return {
    rows,
    edges,
    seed: [tag_of(0), local[0]!],
    per_tag: [per_tag[0]!, per_tag[1]!, per_tag[2]!],
  };
}

/** Encode `(tag, id)` into one dense integer. Stride must exceed any local id. */
export const TAG_STRIDE = 1_000_000_000;
export function encode(tag: number, id: number): number {
  return tag * TAG_STRIDE + id;
}

/**
 * The proven layered graph, but with CYCLES injected so the counting cascade is provably
 * WRONG and DRed is provably right. Back-edges point from a node to its layer-(l-1) parent,
 * forming a 2-cycle. `back_stride` selects which nodes get a back-edge (every node where
 * global_id % back_stride == 0); 0 = no back-edges.
 */
export function gen_multi_cyclic(layers: number, width: number, back_stride: number): MultiGraph {
  const g = gen_multi(layers, width);
  if (back_stride === 0) return g;
  const parents = gen(layers, width);
  const n = parents.length;
  const tier = (gid: number): number => (gid < 2 ? 0 : 1 + Math.floor((gid - 2) / width));
  const tag_of = (gid: number): number => tier(gid) % 3;
  const local = new Array<number>(n).fill(0);
  const per_tag: number[] = [0, 0, 0];
  for (let gid = 0; gid < n; gid++) {
    const t = tag_of(gid);
    local[gid] = per_tag[t]!;
    per_tag[t]! += 1;
  }
  // add back-support edges child -> first-parent, and bump the parent's weight.
  const extra_weight = new Map<string, number>();
  for (let gid = 2; gid < n; gid++) {
    if (gid % back_stride !== 0) continue;
    const p = parents[gid]![0];
    if (p === undefined) continue;
    if (p < 2) continue; // never draw a back-edge INTO a root
    const pt = tag_of(p);
    const pi = local[p]!;
    const ct = tag_of(gid);
    const ci = local[gid]!;
    g.edges.push([ct, ci, pt, pi]);
    const k = `${pt}:${pi}`;
    extra_weight.set(k, (extra_weight.get(k) ?? 0) + 1);
  }
  for (const row of g.rows) {
    const add = extra_weight.get(`${row[0]}:${row[1]}`);
    if (add !== undefined) row[2] += add;
  }
  return g;
}

/**
 * Independent ground truth: after cutting `cut`, which rows are still supported? A row
 * survives iff forward-reachable (over support edges) from a SURVIVING root (a row with no
 * incoming support edge). A dead-simple BFS owing nothing to counting, DRed, dd, or SQLite.
 * Returns encoded survivor keys, sorted ascending (matches the store's `alive_keys`).
 */
export function oracle_survivors(g: MultiGraph, cut: readonly [number, number]): number[] {
  const cut_key = encode(cut[0], cut[1]);
  const adj = new Map<number, number[]>();
  const has_parent = new Set<number>();
  for (const [pt, pi, ct, ci] of g.edges) {
    const pk = encode(pt, pi);
    const ck = encode(ct, ci);
    let list = adj.get(pk);
    if (list === undefined) {
      list = [];
      adj.set(pk, list);
    }
    list.push(ck);
    has_parent.add(ck);
  }

  const frontier: number[] = [];
  const seen = new Set<number>();
  for (const [t, id] of g.rows) {
    const k = encode(t, id);
    if (k !== cut_key && !has_parent.has(k)) {
      seen.add(k);
      frontier.push(k);
    }
  }
  while (frontier.length > 0) {
    const k = frontier.shift()!;
    const children = adj.get(k);
    if (children !== undefined) {
      for (const c of children) {
        if (c !== cut_key && !seen.has(c)) {
          seen.add(c);
          frontier.push(c);
        }
      }
    }
  }
  return [...seen].sort((a, b) => a - b);
}
}

// =============================================================================
// memcap — soft RSS guard (the hard OS cap stays Rust-side; see file header)
// =============================================================================
export namespace memcap {
//! OS-protective self-cap. The point: a runaway scale must fail loudly, never drive the
//! machine into swap. Node cannot setrlimit its address space, so this is a SOFT guard:
//! it records the peak sampled RSS and optionally throws when a sample exceeds the budget.
//! It does NOT prevent allocation (the Rust CappedAlloc does that, by returning null past
//! the cap; Node has no equivalent without a new dependency).

let CAP = 0; // bytes; 0 = unlimited (no enforcement)
let PEAK = 0; // high-water of sampled RSS since the last reset_peak

function bump_peak(now: number): void {
  if (now > PEAK) PEAK = now;
}

/** Cap this process's heap to `mb` megabytes (soft: throws on a sample over the cap). */
export function cap_address_space_mb(mb: number): void {
  const want = Math.floor(mb * 1024 * 1024);
  if (CAP === 0 || want < CAP) CAP = want;
}

/** Live RSS (the analog of Rust's live_bytes — Node reports process RSS, not owned heap). */
export function live_bytes(): number {
  return process.memoryUsage().rss;
}

/** High-water mark of sampled RSS since the last reset_peak. */
export function peak_bytes(): number {
  bump_peak(process.memoryUsage().rss);
  return PEAK;
}

/** Reset the high-water to the current live value (bracket the measured op). */
export function reset_peak(): void {
  PEAK = process.memoryUsage().rss;
}

/** The current soft cap in bytes; 0 = unlimited. */
export function cap_bytes(): number {
  return CAP;
}

/**
 * One RSS sample. Records the peak and, if a cap is set and the sample exceeds it, throws
 * (the soft-budget tripwire). Mirrors the role of Rust's `peak_rss_kb` sampling bracket:
 * sample around heavy ops, report peak in KiB.
 */
export function sample(): number {
  const rss = process.memoryUsage().rss;
  bump_peak(rss);
  if (CAP !== 0 && rss > CAP) {
    throw new Error(`memcap: RSS ${rss} exceeds soft cap ${CAP}`);
  }
  return rss;
}

/** Peak RSS in KiB (the unit Rust's `peak_rss_kb` reports). */
export function peak_rss_kb(): number {
  return Math.floor(peak_bytes() / 1024);
}
}
