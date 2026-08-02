/**
 * golden.test.ts — the ONE confirmation gate for the sprefa-store TS cascade port.
 *
 * It touches every knob:
 *   - Cascade: benchgraphs (DAG, cyclic, phantom-cycle) through EACH retract variant
 *     (retract / retract_scc / retract_dred / retract_dred_cte), each on its own fresh
 *     stamp, with survivors diffed against the ported independent `oracle_survivors`
 *     (the way to tell the cascade is correct without differential-dataflow).
 *   - assert: forward aliveness propagation.
 *   - Reconcile: reconcile_graph + reconcile_stream driven through `propagate` (the
 *     ascending topo sweep), final answer digest === `reconcile_stream.oracle_answer`
 *     (the way to tell the reconcile is correct without salsa), plus mark_changed/dirty
 *     frontier and explicit early-cutoff.
 *   - Reach: reaches_from / reached_by / scc_labels / count_pairs on known small graphs,
 *     diffed against a from-scratch JS tarjan reference (ported from tests/covering.rs).
 *   - Knobs: the cascade runs under two PRAGMA cache_size settings.
 *   - RSS: peak process RSS sampled around the heavy ops via measure.ts (soft guard).
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { createClient } from "@libsql/client";
import { firstValueFrom } from "rxjs";
import process from "node:process";

import { cascade, reach, reconcile } from "../../src/engine/engine.ts";
import { benchgraph, memcap } from "../../src/engine/measure.ts";
import { GraphNs, RelStore } from "../../src/engine/lib.ts";
import { SqlRunner } from "../../src/engine/sqlRunner.ts";
import { salsa } from "../../src/engine/oracle.ts";

// =============================================================================
// Golden-flake diagnostic (ARCH task golden_flake_hunt): this file has failed the whole
// FILE (every named subtest green, no assertion printed) 2x under `just green`'s
// full-parallel load and never in isolation (see docs/failure-modes.md incident notes).
// Every `freshStore()`/`createClient` call below opens a NEW @libsql `:memory:` client
// that this file never explicitly closes (~40+ per run) — a plausible resource-pressure
// contributor when 3 copies of the whole suite run concurrently. Purely observational:
// this hook changes NOTHING about pass/fail (no uncaughtException/unhandledRejection
// listener is installed, so default crash semantics are untouched); it only prints
// context if the process is about to exit non-zero, to catch the next occurrence.
// =============================================================================
let storesOpened = 0;
process.on("exit", (code) => {
  if (code !== 0) {
    console.error(
      `[golden.test.ts diagnostic] pid=${process.pid} exitCode=${code} rss=${process.memoryUsage().rss} storesOpened=${storesOpened}`,
    );
  }
});

/** A fresh in-memory store stamped with the default namespace. */
async function freshStore(): Promise<RelStore> {
  storesOpened++;
  return firstValueFrom(RelStore.attach(createClient({ url: ":memory:", intMode: "bigint" })));
}

/** Load a benchgraph (rows + edges) into a store. */
async function load(store: RelStore, g: benchgraph.MultiGraph): Promise<void> {
  await firstValueFrom(store.add_rows(g.rows));
  await firstValueFrom(store.add_deps(g.edges));
}

// =============================================================================
// The from-scratch tarjan reference (ported verbatim from tests/covering.rs).
// =============================================================================
// The on-disk reach functions MUST agree with this byte-for-byte (partition-canonical for
// SCC). It is the independent referee, owing nothing to SQLite.
namespace ref_oracle {
  export function tarjan(adj: number[][]): [number[], number] {
    const n = adj.length;
    const index = new Array<number>(n).fill(-1);
    const low = new Array<number>(n).fill(0);
    const on_stack = new Array<boolean>(n).fill(false);
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
          on_stack[v] = true;
        }
        if (ci < adj[v]!.length) {
          top[1] += 1;
          const w = adj[v]![ci]!;
          if (index[w] === -1) {
            work.push([w, 0]);
          } else if (on_stack[w] && index[w]! < low[v]!) {
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
              on_stack[w] = false;
              comp[w] = ncomp;
              if (w === v) break;
            }
            ncomp++;
          }
        }
      }
    }
    return [comp, ncomp];
  }

  interface Cond {
    comp: number[];
    ncomp: number;
    size: bigint[];
    cyclic: boolean[];
    cadj: number[][];
    members: number[][];
  }

  export function build_condensed(adj: number[][]): Cond {
    const [comp, ncomp] = tarjan(adj);
    const size = new Array<bigint>(ncomp).fill(0n);
    const members: number[][] = Array.from({ length: ncomp }, () => []);
    for (let node = 0; node < adj.length; node++) {
      const c = comp[node]!;
      size[c]! += 1n;
      members[c]!.push(node);
    }
    const cyclic = new Array<boolean>(ncomp).fill(false);
    for (let c = 0; c < ncomp; c++) if (size[c]! > 1n) cyclic[c] = true;
    for (let u = 0; u < adj.length; u++) {
      for (const w of adj[u]!) {
        if (w === u) cyclic[comp[u]!] = true;
      }
    }
    const sets: Set<number>[] = Array.from({ length: ncomp }, () => new Set<number>());
    for (let u = 0; u < adj.length; u++) {
      const cu = comp[u]!;
      for (const w of adj[u]!) {
        const cw = comp[w]!;
        if (cu !== cw) sets[cu]!.add(cw);
      }
    }
    const cadj = sets.map((s) => [...s].sort((a, b) => a - b));
    return { comp, ncomp, size, cyclic, cadj, members };
  }

  function trailing_zeros(x: bigint): number {
    let n = 0;
    let v = x;
    if ((v & 0xffffffffn) === 0n) {
      n += 32;
      v >>= 32n;
    }
    if ((v & 0xffffn) === 0n) {
      n += 16;
      v >>= 16n;
    }
    if ((v & 0xffn) === 0n) {
      n += 8;
      v >>= 8n;
    }
    if ((v & 0xfn) === 0n) {
      n += 4;
      v >>= 4n;
    }
    if ((v & 0x3n) === 0n) {
      n += 2;
      v >>= 2n;
    }
    if ((v & 0x1n) === 0n) n += 1;
    return n;
  }

  export function count_pairs(c: Cond): bigint {
    const ncomp = c.ncomp;
    const indeg = new Array<number>(ncomp).fill(0);
    for (let cc = 0; cc < ncomp; cc++) for (const s of c.cadj[cc]!) indeg[s]! += 1;
    const topo: number[] = [];
    for (let x = 0; x < ncomp; x++) if (indeg[x] === 0) topo.push(x);
    let qi = 0;
    while (qi < topo.length) {
      const x = topo[qi]!;
      qi++;
      for (const s of c.cadj[x]!) {
        indeg[s]! -= 1;
        if (indeg[s] === 0) topo.push(s);
      }
    }
    const words = (ncomp + 63) >> 6;
    const reach = new Array<bigint>(ncomp * words).fill(0n);
    for (let ti = topo.length - 1; ti >= 0; ti--) {
      const cc = topo[ti]!;
      for (const s of c.cadj[cc]!.slice()) {
        reach[cc * words + (s >> 6)]! |= 1n << BigInt(s & 63);
        for (let w = 0; w < words; w++) reach[cc * words + w]! |= reach[s * words + w]!;
      }
    }
    let total = 0n;
    for (let cc = 0; cc < ncomp; cc++) {
      if (c.cyclic[cc]) total += c.size[cc]! * c.size[cc]!;
      let wsum = 0n;
      for (let w = 0; w < words; w++) {
        let bits = reach[cc * words + w]!;
        while (bits !== 0n) {
          const b = w * 64 + trailing_zeros(bits);
          wsum += c.size[b]!;
          bits &= bits - 1n;
        }
      }
      total += c.size[cc]! * wsum;
    }
    return total;
  }

  export function reaches_from(c: Cond, start: number): number[] {
    const c0 = c.comp[start]!;
    const seen_comp = new Array<boolean>(c.ncomp).fill(false);
    const q: number[] = [...c.cadj[c0]!];
    while (q.length > 0) {
      const cc = q.shift()!;
      if (seen_comp[cc]) continue;
      seen_comp[cc] = true;
      for (const s of c.cadj[cc]!) if (!seen_comp[s]) q.push(s);
    }
    if (c.cyclic[c0]) seen_comp[c0] = true;
    const out: number[] = [];
    for (let cc = 0; cc < c.ncomp; cc++) if (seen_comp[cc]) out.push(...c.members[cc]!);
    return out;
  }

  export function reached_by(c: Cond, target: number): number[] {
    const c0 = c.comp[target]!;
    const cadj_rev: number[][] = Array.from({ length: c.ncomp }, () => []);
    for (let u = 0; u < c.ncomp; u++) for (const v of c.cadj[u]!) cadj_rev[v]!.push(u);
    const seen_comp = new Array<boolean>(c.ncomp).fill(false);
    const q: number[] = [...cadj_rev[c0]!];
    while (q.length > 0) {
      const cc = q.shift()!;
      if (seen_comp[cc]) continue;
      seen_comp[cc] = true;
      for (const s of cadj_rev[cc]!) if (!seen_comp[s]) q.push(s);
    }
    if (c.cyclic[c0]) seen_comp[c0] = true;
    const out: number[] = [];
    for (let cc = 0; cc < c.ncomp; cc++) if (seen_comp[cc]) out.push(...c.members[cc]!);
    return out;
  }

  /** (node, comp_repr=min member idx) over dense idx, sorted by node. */
  export function labels(c: Cond): [number, number][] {
    const reprOf = new Array<number>(c.ncomp).fill(0);
    for (let cc = 0; cc < c.ncomp; cc++) reprOf[cc] = Math.min(...c.members[cc]!);
    const out: [number, number][] = [];
    for (let node = 0; node < c.comp.length; node++) out.push([node, reprOf[c.comp[node]!]!]);
    out.sort((a, b) => a[0] - b[0]);
    return out;
  }
}

/** Dense-index an explicit edge list: node universe = sorted unique endpoints, 0..k. */
function dense(edges: ReadonlyArray<readonly [number, number]>): number[][] {
  const nodes = new Set<number>();
  for (const [a, b] of edges) {
    nodes.add(a);
    nodes.add(b);
  }
  const sorted = [...nodes].sort((a, b) => a - b);
  const remap = new Map<number, number>();
  sorted.forEach((n, i) => remap.set(n, i));
  const adj: number[][] = Array.from({ length: sorted.length }, () => []);
  for (const [a, b] of edges) adj[remap.get(a)!]!.push(remap.get(b)!);
  for (const v of adj) v.sort((a, b) => a - b);
  return adj;
}

/** Load adjacency into a store keyed so store key == dense idx (rel 0, row = idx). */
async function load_dense(adj: number[][]): Promise<RelStore> {
  const store = await freshStore();
  const rows: [number, number, number][] = [];
  for (let k = 0; k < adj.length; k++) rows.push([0, k, 1]);
  await firstValueFrom(store.add_rows(rows));
  const deps: [number, number, number, number][] = [];
  for (let u = 0; u < adj.length; u++) for (const w of adj[u]!) deps.push([0, u, 0, w]);
  await firstValueFrom(store.add_deps(deps));
  return store;
}

function sorted(v: number[]): number[] {
  return v.slice().sort((a, b) => a - b);
}

// =============================================================================
// Cascade — every retract variant agrees with the independent oracle
// =============================================================================
type Variant = [string, (s: RelStore, seed: readonly [number, number]) => Promise<number>];

const RETRACT_VARIANTS: Variant[] = [
  ["retract", (s, seed) => firstValueFrom(s.retract([seed]))],
  ["retract_scc", (s, seed) => firstValueFrom(s.retract_scc([seed]))],
  ["retract_dred", (s, seed) => firstValueFrom(s.retract_dred([seed]))],
  ["retract_dred_cte", (s, seed) => firstValueFrom(s.retract_dred_cte([seed]))],
];

test("cascade: DAG — every variant matches oracle_survivors", async () => {
  for (const [layers, width] of [
    [2, 200],
    [5, 500],
  ] as const) {
    const g = benchgraph.gen_multi(layers, width);
    const oracle = benchgraph.oracle_survivors(g, g.seed);
    for (const [name, fn] of RETRACT_VARIANTS) {
      const s = await freshStore();
      await load(s, g);
      // assert the seed first (exercises the assert knob; a fresh benchgraph seed is
      // already alive, so this is a near-no-op that still runs the forward-add path).
      await firstValueFrom(s.assert([g.seed]));
      await fn(s, g.seed);
      assert.deepStrictEqual(
        await firstValueFrom(s.alive_keys()),
        oracle,
        `DAG l=${layers} w=${width} ${name} disagreed with oracle`,
      );
    }
  }
});

test("cascade: cyclic — SCC/DRed/CTE match the oracle across densities", async () => {
  for (const [layers, width, stride] of [
    [3, 300, 2],
    [6, 400, 3],
    [5, 400, 7],
  ] as const) {
    const g = benchgraph.gen_multi_cyclic(layers, width, stride);
    const oracle = benchgraph.oracle_survivors(g, g.seed);
    for (const [name, fn] of RETRACT_VARIANTS.filter((v) => v[0] !== "retract")) {
      const s = await freshStore();
      await load(s, g);
      await fn(s, g.seed);
      assert.deepStrictEqual(
        await firstValueFrom(s.alive_keys()),
        oracle,
        `CYCLIC l=${layers} w=${width} s=${stride} ${name} disagreed with oracle`,
      );
    }
  }
});

test("cascade: phantom cycle — counting over-keeps, DRed kills (the correctness gap)", async () => {
  // A root-anchored dead cycle: counting keeps phantom rows alive; DRed/dd do not.
  const g = benchgraph.gen_multi_cyclic(4, 400, 7);
  const oracle = benchgraph.oracle_survivors(g, g.seed);

  const dredStore = await freshStore();
  await load(dredStore, g);
  await firstValueFrom(dredStore.retract_dred([g.seed]));
  assert.deepStrictEqual(await firstValueFrom(dredStore.alive_keys()), oracle, "DRed must stay correct on the phantom cycle");

  const countStore = await freshStore();
  await load(countStore, g);
  await firstValueFrom(countStore.retract([g.seed]));
  const count = await firstValueFrom(countStore.alive_keys());
  assert.ok(count.length > oracle.length, "counting must over-keep phantom rows");
  // every oracle survivor is in the counting set (counting only ADDS phantoms)
  const count_set = new Set(count);
  for (const k of oracle) assert.ok(count_set.has(k), `counting dropped a real survivor ${k}`);
});

test("cascade: assert propagates aliveness forward", async () => {
  // R alive, A/B dead; R->A->B. assert R's reach -> R,A,B all alive.
  const s = await freshStore();
  await firstValueFrom(
    s.add_rows([
      [0, 0, 1],
      [0, 1, 0],
      [0, 2, 0],
    ]),
  );
  await firstValueFrom(
    s.add_deps([
      [0, 0, 0, 1],
      [0, 1, 0, 2],
    ]),
  );
  assert.strictEqual(await firstValueFrom(s.alive()), 1);
  await firstValueFrom(s.assert([[0, 0]]));
  assert.strictEqual(await firstValueFrom(s.alive()), 3, "R->A->B all alive after assert");
  assert.deepStrictEqual(await firstValueFrom(s.alive_keys()), [
    cascade.key(0, 0),
    cascade.key(0, 1),
    cascade.key(0, 2),
  ]);
});

// =============================================================================
// Reconcile — propagate matches the from-scratch oracle (no salsa linked)
// =============================================================================
test("reconcile: propagate answer === oracle_answer, byte-identical", async () => {
  for (const [n, ticks, per, seed] of [
    [32, 10, 3, 1],
    [64, 8, 4, 3],
    [16, 20, 1, 99],
  ] as const) {
    const deps = salsa.reconcile_graph(n);
    const stream = salsa.reconcile_stream(n, deps, seed, ticks, per);

    const store = await freshStore();
    const value: bigint[] = stream.init.slice();
    // seed the cold ascending digests (engine_build shape).
    const memo: bigint[] = new Array(n);
    for (let i = 0; i < n; i++) {
      memo[i] = salsa.node_digest(value[i]!, deps[i]!.map((j) => memo[j]!));
      await firstValueFrom(reconcile.seed(store.conn(), store.ns(), i, memo[i]!, deps[i]!, 0));
    }

    // apply every edit tick through the ascending topo sweep.
    for (let ti = 0; ti < stream.edits.length; ti++) {
      const tick = stream.edits[ti]!;
      const rev = ti + 1;
      for (const [i, v] of tick) value[i] = v;
      const seeds = tick.map(([i]) => i);
      await firstValueFrom(
        reconcile.propagate(store.conn(), store.ns(), seeds, rev, (id, dd) => salsa.node_digest(value[id]!, dd)),
      );
    }

    const eng_answer = await firstValueFrom(reconcile.answer(store.conn(), store.ns()));
    assert.strictEqual(
      eng_answer,
      stream.oracle_answer,
      `reconcile answer mismatch (n=${n}, ticks=${ticks}): engine ${eng_answer} != oracle ${stream.oracle_answer}`,
    );
  }
});

test("reconcile: mark_changed/dirty frontier + early cutoff", async () => {
  // chain 1 <- 2 <- 3 (3 reads 2 reads 1). Mark 1 changed -> stale frontier is just {2}.
  const s = await freshStore();
  await firstValueFrom(reconcile.seed(s.conn(), s.ns(), 1, 100n, [], 0));
  await firstValueFrom(reconcile.seed(s.conn(), s.ns(), 2, 200n, [1], 0));
  await firstValueFrom(reconcile.seed(s.conn(), s.ns(), 3, 300n, [2], 0));
  assert.deepStrictEqual(await firstValueFrom(reconcile.dirty(s.conn(), s.ns())), [], "nothing stale at rev 0");
  await firstValueFrom(reconcile.mark_changed(s.conn(), s.ns(), [1], 1));
  assert.deepStrictEqual(
    await firstValueFrom(reconcile.dirty(s.conn(), s.ns())),
    [2],
    "one-hop frontier after mark_changed",
  );

  // recompute 2 with the SAME digest -> verify returns false (early cutoff), wave stops.
  const moved = await firstValueFrom(reconcile.verify(s.conn(), s.ns(), 2, 200n, 1));
  assert.strictEqual(moved, false, "same-value recompute must not move");
  assert.deepStrictEqual(await firstValueFrom(reconcile.dirty(s.conn(), s.ns())), [], "early cutoff stops the wave");

  // a REAL move on 2 re-dirties its reader 3.
  await firstValueFrom(reconcile.mark_changed(s.conn(), s.ns(), [1], 2));
  assert.ok((await firstValueFrom(reconcile.dirty(s.conn(), s.ns()))).includes(2));
  const moved2 = await firstValueFrom(reconcile.verify(s.conn(), s.ns(), 2, 7777n, 2));
  assert.strictEqual(moved2, true, "different digest must move");
  assert.deepStrictEqual(await firstValueFrom(reconcile.dirty(s.conn(), s.ns())), [3], "3 stale once 2 moves");
});

// =============================================================================
// Reach — on-disk cx_dep queries vs the from-scratch tarjan reference
// =============================================================================
const SHAPES: [string, [number, number][]][] = [
  ["chain5", [[0, 1], [1, 2], [2, 3], [3, 4]]],
  ["diamond", [[0, 1], [0, 2], [1, 3], [2, 3]]],
  ["cycle3", [[1, 2], [2, 3], [3, 1]]],
  ["cycle3+tail", [[0, 1], [1, 2], [2, 3], [3, 1], [2, 4]]],
  ["two_cycles", [[0, 1], [1, 0], [2, 3], [3, 2], [1, 2]]],
  ["self_loop", [[0, 0], [0, 1], [1, 2]]],
  ["figure8", [[0, 1], [1, 2], [2, 0], [2, 3], [3, 4], [4, 2]]],
];

test("reach: scc_labels and count_pairs agree with the from-scratch reference", async () => {
  for (const [name, edges] of SHAPES) {
    const adj = dense(edges);
    const cond = ref_oracle.build_condensed(adj);
    const store = await load_dense(adj);

    assert.deepStrictEqual(
      await firstValueFrom(reach.scc_labels(store.conn(), store.ns())),
      ref_oracle.labels(cond),
      `${name}: SCC partition disagreed`,
    );
    assert.strictEqual(
      await firstValueFrom(reach.count_pairs(store.conn(), store.ns())),
      ref_oracle.count_pairs(cond),
      `${name}: count_pairs disagreed`,
    );
  }
});

test("reach: reaches_from / reached_by agree with the reference, every start node", async () => {
  for (const [name, edges] of SHAPES) {
    const adj = dense(edges);
    const cond = ref_oracle.build_condensed(adj);
    const store = await load_dense(adj);
    for (let start = 0; start < adj.length; start++) {
      const wantFwd = sorted(ref_oracle.reaches_from(cond, start));
      const gotFwd = sorted(await firstValueFrom(reach.reaches_from(store.conn(), store.ns(), start)));
      assert.deepStrictEqual(gotFwd, wantFwd, `${name}: reaches_from(${start}) disagreed`);

      const wantRev = sorted(ref_oracle.reached_by(cond, start));
      const gotRev = sorted(await firstValueFrom(reach.reached_by(store.conn(), store.ns(), start)));
      assert.deepStrictEqual(gotRev, wantRev, `${name}: reached_by(${start}) disagreed`);
    }
  }
});

// =============================================================================
// Knobs — the cascade loads and runs under two PRAGMA cache_size settings
// =============================================================================
test("knobs: cascade correct under two cache_size settings", async () => {
  const g = benchgraph.gen_multi(3, 120);
  const oracle = benchgraph.oracle_survivors(g, g.seed);
  // a small cache (1 MiB) and a large one (256 MiB) — both must agree with the oracle.
  for (const cacheKib of [-1024, -262144]) {
    const s = await freshStore();
    await firstValueFrom(SqlRunner.executeMultiple(s.conn(), `PRAGMA cache_size=${cacheKib};`));
    await load(s, g);
    await firstValueFrom(s.retract_dred([g.seed]));
    assert.deepStrictEqual(
      await firstValueFrom(s.alive_keys()),
      oracle,
      `cache_size=${cacheKib} disagreed with oracle`,
    );
  }
});

// =============================================================================
// RSS — peak process memory stays bounded (SQLite owns the data)
// =============================================================================
test("rss: peak RSS sampled and bounded", async () => {
  memcap.reset_peak();
  const store = await freshStore();
  const g = benchgraph.gen_multi(4, 300);
  memcap.sample();
  await load(store, g);
  memcap.sample();
  await firstValueFrom(store.retract_dred([g.seed]));
  memcap.sample();
  const oracle = benchgraph.oracle_survivors(g, g.seed);
  assert.deepStrictEqual(await firstValueFrom(store.alive_keys()), oracle, "RSS-run graph must still be correct");

  const peakKib = memcap.peak_rss_kb();
  // Budget: 512 MiB. SOFT guard only — the hard OS cap stays Rust-side (Node cannot
  // setrlimit its address space; see measure.ts). See the port report for the measured
  // peak under @libsql/client (vs the prior better-sqlite3 baseline).
  const BUDGET_KIB = 512 * 1024; // 524288 KiB = 512 MiB
  assert.ok(peakKib < BUDGET_KIB, `peak RSS ${peakKib} KiB exceeds budget ${BUDGET_KIB} KiB`);
  // Echo for the record (node --test captures stdout).
  console.log(`peak RSS: ${peakKib} KiB (${(peakKib / 1024).toFixed(1)} MiB)`);
});

// =============================================================================
// Namespacing (Epic 3) — two stores in one db retract without cross-talk
// =============================================================================
test("namespacing: two GraphNs in one db are independent", async () => {
  const db = createClient({ url: ":memory:", intMode: "bigint" });
  const ns = GraphNs.default();
  const nb = GraphNs.new("b_");
  // Stamp BOTH namespaces into the one db.
  await firstValueFrom(
    SqlRunner.executeMultiple(
      db,
      "PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA foreign_keys=ON;PRAGMA temp_store=MEMORY;",
    ),
  );
  await firstValueFrom(cascade.create_schema(db, ns));
  await firstValueFrom(reconcile.create_schema(db, ns));
  // b_ namespace: the cascade create_schema re-issues PRAGMA cache_size/mmap (harmless).
  // TEMP table names are namespaced by prefix, so the two stores' working sets don't clash.
  await firstValueFrom(cascade.create_schema(db, nb));
  await firstValueFrom(reconcile.create_schema(db, nb));

  // Same ref-count graph in each: (0,0) anchors (0,1) via one dep edge.
  await firstValueFrom(
    cascade.insert_rows(db, ns, [
      [0, 0, 1],
      [0, 1, 1],
    ]),
  );
  await firstValueFrom(cascade.insert_deps(db, ns, [[0, 0, 0, 1]]));
  await firstValueFrom(
    cascade.insert_rows(db, nb, [
      [0, 0, 1],
      [0, 1, 1],
    ]),
  );
  await firstValueFrom(cascade.insert_deps(db, nb, [[0, 0, 0, 1]]));

  const countAlive = (table: string): Promise<number> =>
    firstValueFrom(SqlRunner.scalar(db, `SELECT count(*) FROM ${table} WHERE weight>0`));
  assert.strictEqual(await countAlive(ns.row), 2);
  assert.strictEqual(await countAlive(nb.row), 2);

  // Retract the anchor in the DEFAULT namespace only.
  await firstValueFrom(cascade.retract(db, ns, [[0, 0]]));
  assert.strictEqual(await countAlive(ns.row), 0, "default ns retracted to zero");
  assert.strictEqual(await countAlive(nb.row), 2, "b_ ns untouched (no cross-talk)");
});
