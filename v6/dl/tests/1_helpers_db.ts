/**
 * tests/1_helpers_db.ts - shared db/runtime test helpers for the M2 (schema + tick
 * runtime) test suite. Sibling `tests/0_helpers.ts` (owned by a different package) is
 * the bridge-helper module; this file never touches it.
 *
 * Exports: freshDbPath/cleanupDbFile (a scratch sqlite file per test), fakeBridgeOk (a
 * minimal hand-built BridgeOk), buildParentGrandparentProgram/singleEdbRelProgram (the
 * hand-built ast.ts programs the M2 tests exercise), bootFixture/bootParentFixture (boot
 * a DlRuntime against a fresh db), edbBatch (EdbBatch object-literal shorthand),
 * rowsOf/deltaDump (deterministic read helpers for snapshot comparison).
 */
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { createClient } from "@libsql/client";
import { derivedRel, edbRel, headVar, relRef, v, type Program, type Rule } from "sprefa-store-engine/src/lower/ast.ts";

import { DlRuntime } from "../src/3_runtime.ts";
import type { BridgeOk, EdbBatch, Retention, Row, Value } from "../tasks.d.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Scratch db files.
// ─────────────────────────────────────────────────────────────────────────────

export function freshDbPath(): string {
  return path.join(os.tmpdir(), `dl-m2-${crypto.randomBytes(8).toString("hex")}.sqlite`);
}

/** Removes the db file and its WAL/SHM/journal siblings. Call from `after()`. */
export function cleanupDbFile(dbPath: string): void {
  for (const suffix of ["", "-wal", "-shm", "-journal"]) {
    try {
      fs.unlinkSync(dbPath + suffix);
    } catch {
      // already gone; nothing to clean up.
    }
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixture programs (hand-built ast.ts, no parser this arc).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * EDB parent(child_name, parent_name) + derived
 * grandparent(grandchild_name, grandparent_name) <- parent(a, b), parent(b, c).
 * The M2 golden fixture (add/noop/remove).
 */
export function buildParentGrandparentProgram(): Program {
  const parentDecl = edbRel("parent", ["child_name", "parent_name"]);
  const grandparentDecl = derivedRel("grandparent", ["grandchild_name", "grandparent_name"]);
  const rule: Rule = {
    head: "grandparent",
    headTerms: [headVar("grandchild_name"), headVar("grandparent_name")],
    body: [
      relRef("parent", v("grandchild_name"), v("mid_name")),
      relRef("parent", v("mid_name"), v("grandparent_name")),
    ],
  };
  return { rels: [parentDecl, grandparentDecl], rules: [rule] };
}

/** A single standalone EDB rel with no rules (retention tests: rel(0)/rel(1)). */
export function singleEdbRelProgram(name: string, columns: readonly string[]): Program {
  return { rels: [edbRel(name, columns)], rules: [] };
}

// ─────────────────────────────────────────────────────────────────────────────
// A minimal hand-built BridgeOk (M1's bridge isn't in this package's scope).
// ─────────────────────────────────────────────────────────────────────────────

export function fakeBridgeOk(
  program: Program,
  literalSeeds: ReadonlyMap<string, Value> = new Map(),
  retentionOverrides: Readonly<Record<string, Retention>> = {},
  columnTypeOverrides: Readonly<Record<string, readonly ("text" | "int")[]>> = {},
): BridgeOk {
  const retention = new Map<string, Retention>();
  const columnTypes = new Map<string, readonly ("text" | "int")[]>();
  for (const decl of program.rels) {
    retention.set(decl.name, retentionOverrides[decl.name] ?? "all");
    // M9 columnType flow: real bridge() infers these; a hand-built program has no
    // declared types, so a caller passes overrides for any rel with numeric columns.
    // Default all-text is safe until the storage plane reads columnTypes (M9 wiring).
    columnTypes.set(decl.name, columnTypeOverrides[decl.name] ?? decl.columns.map(() => "text"));
  }
  return {
    kind: "ok",
    program,
    hosts: [],
    retention,
    queries: [],
    minted: [],
    literalSeeds,
    columnTypes,
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// Boot helpers.
// ─────────────────────────────────────────────────────────────────────────────

export async function bootFixture(
  program: Program,
  literalSeeds?: ReadonlyMap<string, Value>,
  retentionOverrides?: Readonly<Record<string, Retention>>,
): Promise<{ readonly rt: DlRuntime; readonly dbPath: string }> {
  const dbPath = freshDbPath();
  const bridge = fakeBridgeOk(program, literalSeeds, retentionOverrides);
  const rt = await DlRuntime.boot({ dbPath, bridge });
  return { rt, dbPath };
}

export async function bootParentFixture(): Promise<{ readonly rt: DlRuntime; readonly dbPath: string }> {
  return bootFixture(buildParentGrandparentProgram());
}

// ─────────────────────────────────────────────────────────────────────────────
// EdbBatch object-literal shorthand: edbBatch({ parent: [...] }, { parent: [...] }).
// ─────────────────────────────────────────────────────────────────────────────

export function edbBatch(
  insert: Readonly<Record<string, readonly Row[]>> = {},
  retract: Readonly<Record<string, readonly Row[]>> = {},
): EdbBatch {
  return {
    insert: new Map(Object.entries(insert)),
    retract: new Map(Object.entries(retract)),
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic read helpers (sorted, for snapshot / deepEqual comparison).
// ─────────────────────────────────────────────────────────────────────────────

function byJson(a: unknown, b: unknown): number {
  const left = JSON.stringify(a);
  const right = JSON.stringify(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

export async function rowsOf(rt: DlRuntime, rel: string): Promise<Row[]> {
  const rows = await rt.rows(rel);
  return [...rows].sort(byJson);
}

export interface DeltaLogEntry {
  readonly rel: string;
  readonly row_digest: number;
  readonly tick: number;
  readonly weight: number;
}

/** Reads the whole `delta` log directly (bypassing DlRuntime, which has no generic
 *  table reader): a fresh short-lived connection, closed before returning. */
export async function deltaDump(dbPath: string): Promise<DeltaLogEntry[]> {
  const db = createClient({ url: `file:${dbPath}` });
  try {
    const res = await db.execute("SELECT rel, row_digest, tick, weight FROM delta ORDER BY tick, rel, row_digest, weight");
    return res.rows
      .map((row) => ({
        rel: String(row.rel),
        row_digest: Number(row.row_digest),
        tick: Number(row.tick),
        weight: Number(row.weight),
      }))
      .sort(byJson);
  } finally {
    db.close();
  }
}
