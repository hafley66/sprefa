#!/usr/bin/env node
// Deterministic (seeded) big node/edge table generator for panel perf testing
// at scale (5k/20k/50k rows and beyond, tunable by count). Shaped like a
// discovered `_node`/`_edge` layer pair -- `rel_perf_node`/`rel_perf_edge` --
// so flow-panel.html's discoverLayers() (media/flow-panel.html) picks it up
// as a toggleable layer with zero preset/SQL edits, the same contract as
// tests/fixture-bridge.mjs's existing rel_demo_node/rel_demo_edge pair.
//
//   node tests/perf/big-graph-fixture.mjs 20000   # smoke check: prints counts
//
// Nodes are grouped into directories (dirSize files per dir, nodesPerFile
// nodes per file) so the panel's file-path grouping (buildRows in
// flow-panel.html) produces a deep, multi-level, foldable hierarchy -- this
// is what makes the "collapse a group" step in the perf interaction script
// mean something at scale, instead of collapsing a single flat dir. Edges are
// a forward chain (node[i] -> node[i+1], guarantees every node has at least
// one linked edge so the default "linked only" toggle never empties the
// graph) plus deterministic random cross edges up to ~edgeFactor * count.

function mulberry32(seed) {
  let s = seed >>> 0;
  return function () {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const NODE_KINDS = ["fn", "struct", "enum", "trait", "const"];
const EDGE_KINDS = ["calls", "uses", "impl"];

/**
 * @param {number} count total node rows
 * @param {object} [opts]
 * @param {number} [opts.seed=1] PRNG seed -- same seed+count always produces
 *   byte-identical rows (deterministic fixture, no flake from row content).
 * @param {number} [opts.edgeFactor=2] target edge count as a multiple of
 *   `count` (approximate -- the forward chain's count-1 edges count toward it).
 * @param {number} [opts.nodesPerFile=8] node rows sharing one synthetic file.
 * @param {number} [opts.filesPerDir=12] files sharing one synthetic directory.
 * @returns {{nodeCols: string[], nodeRows: any[][], edgeCols: string[], edgeRows: any[][]}}
 */
export function generateBigGraph(count, opts = {}) {
  const seed = opts.seed ?? 1;
  const edgeFactor = opts.edgeFactor ?? 2;
  const nodesPerFile = opts.nodesPerFile ?? 8;
  const filesPerDir = opts.filesPerDir ?? 12;
  const rand = mulberry32(seed);

  const nodeCols = ["id", "label", "kind", "file", "line"];
  const nodeRows = new Array(count);
  for (let i = 0; i < count; i++) {
    const fileIdx = Math.floor(i / nodesPerFile);
    const dirIdx = Math.floor(fileIdx / filesPerDir);
    const file = `perf/dir${dirIdx}/file${fileIdx % filesPerDir}.rs`;
    const kind = NODE_KINDS[i % NODE_KINDS.length];
    const line = 1 + (i % nodesPerFile) * 4;
    nodeRows[i] = [`perf-${i}`, `node_${i}`, kind, file, line];
  }

  const edgeCols = ["src", "dst", "kind"];
  const edgeRows = [];
  for (let i = 0; i < count - 1; i++) {
    edgeRows.push([`perf-${i}`, `perf-${i + 1}`, EDGE_KINDS[i % EDGE_KINDS.length]]);
  }
  const targetEdges = Math.max(0, Math.round(count * edgeFactor));
  let extraIdx = 0;
  while (edgeRows.length < targetEdges && count > 1) {
    const a = Math.floor(rand() * count);
    const b = Math.floor(rand() * count);
    if (a === b) { extraIdx++; continue; }
    edgeRows.push([`perf-${a}`, `perf-${b}`, EDGE_KINDS[extraIdx % EDGE_KINDS.length]]);
    extraIdx++;
  }

  return { nodeCols, nodeRows, edgeCols, edgeRows };
}

// tables map shape ready for fixture-bridge.mjs's createServer(extraTables).
export function perfTables(count, opts = {}) {
  const { nodeCols, nodeRows, edgeCols, edgeRows } = generateBigGraph(count, opts);
  return {
    rel_perf_node: { cols: nodeCols, rows: nodeRows },
    rel_perf_edge: { cols: edgeCols, rows: edgeRows },
  };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const count = Number(process.argv[2] || 5000);
  const g = generateBigGraph(count);
  console.log(`nodes=${g.nodeRows.length} edges=${g.edgeRows.length}`);
}
