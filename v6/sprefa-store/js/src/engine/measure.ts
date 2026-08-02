/**
 * measure.ts — the measurement harness helpers, ported from src/measure.rs.
 *
 * Ports: `benchgraph` (the deterministic layered DAG generator + the independent
 * `oracle_survivors` from-scratch BFS referee) and `memcap` (a soft RSS guard).
 *
 * DIVERGENCE (documented): Rust reads peak RSS via `getrusage` and enforces a hard address-
 * space cap via a counting `#[global_allocator]` (CappedAlloc). Node can do neither:
 * `process.memoryUsage().residentSetSizeBytes` is the analog peak sampler, and Node cannot `setrlimit` its
 * own address space. So memcap here is a SOFT guard: it records the peak sampled RSS and
 * optionally throws when a sample exceeds the budget. The hard OS cap stays Rust-side; this
 * only confirms the TS process stays RAM-bounded (SQLite owns the data, the thesis).
 */

import process from "node:process";

// =============================================================================
// benchgraph — one deterministic DAG generator shared by both sides of the head-to-head
// =============================================================================
export namespace benchgraph {
//! Nodes 0 and 1 are roots (no parents); every other node has mixed refCount so retracting
//! root 0 leaves a non-trivial subset alive. A multi-relation reference graph: THREE logical
//! relations so the polymorphic `(tag, id)` key is load-bearing.

/** `parents[node]` = the parent GLOBAL node ids. Nodes 0 and 1 are roots. */
export function gen(layers: number, width: number): number[][] {
  const totalNodeCount = 2 + layers * width;
  const parents: number[][] = Array.from({ length: totalNodeCount }, () => []);
  for (let layerIndex = 0; layerIndex < layers; layerIndex++) {
    for (let widthIndex = 0; widthIndex < width; widthIndex++) {
      const id = 2 + layerIndex * width + widthIndex;
      if (layerIndex === 0) {
        parents[id]!.push(0);
        if (widthIndex % 3 === 0) parents[id]!.push(1);
      } else {
        const prev = 2 + (layerIndex - 1) * width;
        parents[id]!.push(prev + widthIndex);
        parents[id]!.push(prev + ((widthIndex + 1) % width));
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
  const nodeCount = parents.length;

  const tier = (globalNodeId: number): number => (globalNodeId < 2 ? 0 : 1 + Math.floor((globalNodeId - 2) / width));
  const tag_of = (globalNodeId: number): number => tier(globalNodeId) % 3;

  // Assign a per-relation local id to every global node, in global order.
  const local = new Array<number>(nodeCount).fill(0);
  const per_tag: number[] = [0, 0, 0];
  for (let globalNodeId = 0; globalNodeId < nodeCount; globalNodeId++) {
    const tag = tag_of(globalNodeId);
    local[globalNodeId] = per_tag[tag]!;
    per_tag[tag]! += 1;
  }

  const rows: [number, number, number][] = [];
  const edges: [number, number, number, number][] = [];
  for (let globalNodeId = 0; globalNodeId < nodeCount; globalNodeId++) {
    const weight = parents[globalNodeId]!.length === 0 ? 1 : parents[globalNodeId]!.length;
    rows.push([tag_of(globalNodeId), local[globalNodeId]!, weight]);
    for (const parentGlobalId of parents[globalNodeId]!) {
      edges.push([tag_of(parentGlobalId), local[parentGlobalId]!, tag_of(globalNodeId), local[globalNodeId]!]);
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
  const graph = gen_multi(layers, width);
  if (back_stride === 0) return graph;
  const parents = gen(layers, width);
  const nodeCount = parents.length;
  const tier = (globalNodeId: number): number => (globalNodeId < 2 ? 0 : 1 + Math.floor((globalNodeId - 2) / width));
  const tag_of = (globalNodeId: number): number => tier(globalNodeId) % 3;
  const local = new Array<number>(nodeCount).fill(0);
  const per_tag: number[] = [0, 0, 0];
  for (let globalNodeId = 0; globalNodeId < nodeCount; globalNodeId++) {
    const tag = tag_of(globalNodeId);
    local[globalNodeId] = per_tag[tag]!;
    per_tag[tag]! += 1;
  }
  // add back-refCount edges child -> first-parent, and bump the parent's weight.
  const extra_weight = new Map<string, number>();
  for (let globalNodeId = 2; globalNodeId < nodeCount; globalNodeId++) {
    if (globalNodeId % back_stride !== 0) continue;
    const firstParentGlobalId = parents[globalNodeId]![0];
    if (firstParentGlobalId === undefined) continue;
    if (firstParentGlobalId < 2) continue; // never draw a back-edge INTO a root
    const parentTag = tag_of(firstParentGlobalId);
    const parentLocalId = local[firstParentGlobalId]!;
    const childTag = tag_of(globalNodeId);
    const childLocalId = local[globalNodeId]!;
    graph.edges.push([childTag, childLocalId, parentTag, parentLocalId]);
    const key = `${parentTag}:${parentLocalId}`;
    extra_weight.set(key, (extra_weight.get(key) ?? 0) + 1);
  }
  for (const row of graph.rows) {
    const extraWeightDelta = extra_weight.get(`${row[0]}:${row[1]}`);
    if (extraWeightDelta !== undefined) row[2] += extraWeightDelta;
  }
  return graph;
}

/**
 * Independent ground truth: after cutting `cut`, which rows are still supported? A row
 * survives iff forward-reachable (over ref-count edges) from a SURVIVING root (a row with no
 * incoming ref-count edge). A dead-simple BFS owing nothing to counting, DRed, dd, or SQLite.
 * Returns encoded survivor keys, sorted ascending (matches the store's `alive_keys`).
 */
export function oracle_survivors(g: MultiGraph, cut: readonly [number, number]): number[] {
  const cutKey = encode(cut[0], cut[1]);
  const adjacency = new Map<number, number[]>();
  const has_parent = new Set<number>();
  for (const [parentTag, parentLocalId, childTag, childLocalId] of g.edges) {
    const parentKey = encode(parentTag, parentLocalId);
    const childKey = encode(childTag, childLocalId);
    let adjacencyList = adjacency.get(parentKey);
    if (adjacencyList === undefined) {
      adjacencyList = [];
      adjacency.set(parentKey, adjacencyList);
    }
    adjacencyList.push(childKey);
    has_parent.add(childKey);
  }

  const frontier: number[] = [];
  const seen = new Set<number>();
  for (const [tag, id] of g.rows) {
    const key = encode(tag, id);
    if (key !== cutKey && !has_parent.has(key)) {
      seen.add(key);
      frontier.push(key);
    }
  }
  while (frontier.length > 0) {
    const key = frontier.shift()!;
    const children = adjacency.get(key);
    if (children !== undefined) {
      for (const childKey of children) {
        if (childKey !== cutKey && !seen.has(childKey)) {
          seen.add(childKey);
          frontier.push(childKey);
        }
      }
    }
  }
  return [...seen].sort((keyA, keyB) => keyA - keyB);
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
 * (the soft-budget tripwire). Mirrors the role of Rust's `peak_residentSetSizeBytes_kb` sampling bracket:
 * sample around heavy ops, report peak in KiB.
 */
export function sample(): number {
  const residentSetSizeBytes = process.memoryUsage().rss;
  bump_peak(residentSetSizeBytes);
  if (CAP !== 0 && residentSetSizeBytes > CAP) {
    throw new Error(`memcap: RSS ${residentSetSizeBytes} exceeds soft cap ${CAP}`);
  }
  return residentSetSizeBytes;
}

/** Peak RSS in KiB (the unit Rust's `peak_rss_kb` reports). */
export function peak_rss_kb(): number {
  return Math.floor(peak_bytes() / 1024);
}
}
