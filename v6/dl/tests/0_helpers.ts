/**
 * 0_helpers.ts — shared test setup for this package (and later ones, per ownership).
 * No test bodies here, only fixtures/builders every *.test.ts file can import.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { builtinDecls } from "../src/5_diag.ts";
import type { RelDecl } from "sprefa-store-engine/src/lower/ast.ts";
import { bridge } from "../src/0_ast_bridge.ts";
import type { BridgeOk, BridgeResult } from "../tasks.d.ts";

/** The builtin spine rels (SpineRelName, tasks.d.ts) + diag, all declared EDB — the
 *  bridge itself decides which of these become IDB (headed by a user rule). The
 *  canonical decl list lives in src/5_diag.ts (builtinDecls); this is a re-export
 *  kept under the test-helper name so test files read consistently (two-copies law:
 *  the inline list this function used to build was consolidated at integration). */
export function builtinRelsForTests(): readonly RelDecl[] {
  return builtinDecls;
}

/** Read a fixture file by name, relative to v6/dl/fixtures/. */
export function readFixture(name: string): string {
  const path = fileURLToPath(new URL(`../fixtures/${name}`, import.meta.url));
  return readFileSync(path, "utf8");
}

/** bridge() a fixture .dl file against the standard test builtins. */
export function bridgeFixture(name: string): BridgeResult {
  return bridge(readFixture(name), builtinRelsForTests());
}

// ─────────────────────────────────────────────────────────────────────────────
// Stable serialization: Maps -> sorted [key, value] entry arrays, plain objects ->
// keys sorted alphabetically at every level. Deterministic regardless of Map/object
// insertion order, so a golden snapshot compares byte-for-byte across re-bridges.
// ─────────────────────────────────────────────────────────────────────────────

function stableValue(value: unknown): unknown {
  if (value instanceof Map) {
    return [...value.entries()]
      .sort(([keyA], [keyB]) => (keyA < keyB ? -1 : keyA > keyB ? 1 : 0))
      .map(([key, mapValue]) => [key, stableValue(mapValue)]);
  }
  if (Array.isArray(value)) return value.map(stableValue);
  if (value !== null && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(record).sort()) out[key] = stableValue(record[key]);
    return out;
  }
  return value;
}

/** The golden shape: {program, hosts, minted, retention, literalSeeds, queries},
 *  Maps flattened to sorted entry arrays, object keys sorted at every level. */
export function stableSerialize(bridgeOk: BridgeOk): unknown {
  return stableValue({
    program: bridgeOk.program,
    hosts: bridgeOk.hosts,
    minted: bridgeOk.minted,
    retention: bridgeOk.retention,
    literalSeeds: bridgeOk.literalSeeds,
    queries: bridgeOk.queries,
  });
}
