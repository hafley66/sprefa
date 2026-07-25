/**
 * oracle.ts — the from-scratch correctness math, ported from src/oracle.rs.
 *
 * Ports ONLY the independent ground-truth math:
 *   dd::mix            — the splitmix64 hash shared by every plane (cascade digest,
 *                        reach digest, reconcile cell hash).
 *   salsa::WIN / DEG   — the reconcile DAG shape knobs.
 *   salsa::cell_hash, node_digest, reconcile_graph, reconcile_stream (+oracle_answer)
 *                        — the from-scratch reconcile oracle. `oracle_answer` is the
 *                        parity key the SQLite reconcile plane is diffed against.
 *
 * NOT ported (they stay Rust-side as ground truth): dd::DdBfs (differential-dataflow
 * single-source reachability) and salsa::SalsaReconciler (the resident salsa-crate
 * oracle). The shipping TS engine is parity-checked against THIS file's math instead.
 *
 * i64/u64 fidelity: mix/cell_hash/node_digest operate on full 64-bit values. JS Number
 * cannot hold 64-bit products exactly, so every digest flows as a `bigint`. The splitmix
 * steps reduce mod 2^64 via BigInt.asUintN(64, ...) (wrapping_mul / wrapping_add) and the
 * final `as i64` reinterpretation via BigInt.asIntN(64, ...). XOR is carried out in
 * unsigned 64-bit space so the bit pattern matches Rust's i64 ^ exactly.
 */

/** 2^64 mask and width, used everywhere below. */
const U64 = 1n << 64n;

/** reinterpret u64 bits as signed i64 (Rust `as i64`). */
function asI64(x: bigint): bigint {
  return BigInt.asIntN(64, x);
}
/** reduce mod 2^64 into the unsigned range (Rust `as u64` / wrapping). */
function asU64(x: bigint): bigint {
  return BigInt.asUintN(64, x);
}

/**
 * dd::mix — splitmix64. The ONE hash every plane agrees on. `k` is the i64 argument
 * Rust passes (`k as u64` internally); returns the signed i64 Rust returns
 * (`(z ^ (z >> 31)) as i64`). Robust to signed or unsigned bigint input: the first
 * step reduces `k + CONST` mod 2^64, which is correct for either representation.
 */
export function mix(k: bigint): bigint {
  let hashValue = asU64(k + 0x9e3779b97f4a7c15n);
  hashValue = asU64((hashValue ^ (hashValue >> 30n)) * 0xbf58476d1ce4e5b9n);
  hashValue = asU64((hashValue ^ (hashValue >> 27n)) * 0x94d049bb133111ebn);
  return asI64(hashValue ^ (hashValue >> 31n));
}

export namespace salsa {
  /** A rel reads deps within the previous WIN ids. */
  export const WIN = 8;
  /** Up to DEG deps per rel. */
  export const DEG = 3;

  /** salsa::cell_hash — `mix((i as i64) << 12 ^ salt)`. i is a u32 node index. */
  export function cell_hash(i: number, salt: bigint): bigint {
    return mix((BigInt(i) << 12n) ^ salt);
  }

  /**
   * One node's digest from its value and its deps' (already-current) digests.
   * `dep_digests.fold(mix(value), |acc, d| acc ^ mix(d))`. XOR is carried in unsigned
   * 64-bit space; the final `as i64` matches Rust's i64 return.
   */
  export function node_digest(value: bigint, dep_digests: Iterable<bigint>): bigint {
    let digestAccumulator = asU64(mix(value));
    for (const digest of dep_digests) {
      digestAccumulator = asU64(digestAccumulator ^ asU64(mix(digest)));
    }
    return asI64(digestAccumulator);
  }

  /**
   * Layered dep DAG: node i reads up to DEG distinct j in [i-WIN, i). Real rule graphs
   * are shallow, sparse, mostly reading recently-defined rels; a DAG keeps the oracle
   * exact and ascending id a valid topo order. Returns `deps[i]` = sorted j indices.
   */
  export function reconcile_graph(n: number): number[][] {
    const deps: number[][] = Array.from({ length: n }, () => []);
    for (let nodeIndex = 0; nodeIndex < n; nodeIndex++) {
      const seen = new Set<number>();
      for (let depOffset = 0; depOffset < DEG; depOffset++) {
        const hashValue = asU64(mix((BigInt(nodeIndex) << 8n) ^ BigInt(depOffset)));
        const span = Number(hashValue % BigInt(WIN)) + 1;
        const depIndex = nodeIndex - span;
        if (depIndex >= 0 && !seen.has(depIndex)) {
          seen.add(depIndex);
          deps[nodeIndex]!.push(depIndex);
        }
      }
      deps[nodeIndex]!.sort((indexA, indexB) => indexA - indexB);
    }
    return deps;
  }

  /** The deterministic edit stream + the independent from-scratch oracle answer. */
  export interface RStream {
    /** initial cell values (one per node), in node-index order. */
    init: bigint[];
    /** edits[tick] = list of (node index, new value). */
    edits: [number, bigint][][];
    /** XOR of every node's from-scratch ascending digest after ALL edits. */
    oracle_answer: bigint;
  }

  /**
   * Deterministic stream: `ticks` rounds, each editing `per` cells. 1 in 4 edits
   * re-writes the SAME value (exercises early cutoff / backdating). `oracle_answer`
   * is the from-scratch ascending digest after ALL edits (independent of both engines).
   *
   * The LCG is a u64 linear congruential generator (wrapping mul/add); `next()` returns
   * `rng >> 16` (a 48-bit value). The call pattern MUST mirror Rust exactly: one `next()`
   * for the cell index, one for the same/new flag, and a THIRD only when the value
   * actually changes (the salt source).
   */
  export function reconcile_stream(
    n: number,
    deps: number[][],
    seed: number,
    ticks: number,
    per: number,
  ): RStream {
    const cellValues: bigint[] = [];
    for (let nodeIndex = 0; nodeIndex < n; nodeIndex++) cellValues.push(cell_hash(nodeIndex, 0n));
    const init = cellValues.slice();

    // rng = seed ^ (n as u64).wrapping_mul(0x9E3779B97F4A7C15)  (mod 2^64)
    let randomState = asU64(BigInt(seed) ^ asU64(BigInt(n) * 0x9e3779b97f4a7c15n));
    const next = (): bigint => {
      randomState = asU64(randomState * 6364136223846793005n + 1442695040888963407n);
      return randomState >> 16n;
    };

    const edits: [number, bigint][][] = [];
    for (let tickIndex = 0; tickIndex < ticks; tickIndex++) {
      const tickEdits: [number, bigint][] = [];
      for (let editIndex = 0; editIndex < per; editIndex++) {
        const cellIndex = Number(next() % BigInt(n));
        const same = next() % 4n === 0n;
        let newValue: bigint;
        if (same) {
          newValue = cellValues[cellIndex]!;
        } else {
          const saltValue = next();
          newValue = cell_hash(cellIndex, asI64(saltValue) | 1n);
        }
        cellValues[cellIndex] = newValue;
        tickEdits.push([cellIndex, newValue]);
      }
      edits.push(tickEdits);
    }

    // oracle: from-scratch, ascending (topo). Independent of both engines.
    const memo: bigint[] = new Array(n);
    for (let nodeIndex = 0; nodeIndex < n; nodeIndex++) {
      const depDigests = deps[nodeIndex]!.map((depIndex) => memo[depIndex]!);
      memo[nodeIndex] = node_digest(cellValues[nodeIndex]!, depDigests);
    }
    let digestAccumulator = 0n;
    for (const digest of memo) digestAccumulator = asU64(digestAccumulator ^ asU64(digest));
    const oracle_answer = asI64(digestAccumulator);

    return { init, edits, oracle_answer };
  }
}
