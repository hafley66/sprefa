/**
 * ingest.ts — E2: the jsonl ingest seam (extractor stdout -> SQLite spine -> dirty frontier).
 *
 * Contract (v6/plans/2026-07-23-v6-rest-epic-golden-plan.md, "E2 · ingest seam"):
 *   ingestJsonl(store, rels, lines, rev) -> IngestReport{lines, stmts, changed}
 * `FactLine` there is a CANDIDATE union, pinned for real at F2 (task 2.5, the extractor
 * worktree's job — NOT this file's). Everything below is this file's own resolution of the
 * open questions the candidate schema leaves unstated, documented so 2.5 can revise cleanly.
 *
 * ---- design decisions (the parts the contract left open) -------------------------------
 *
 * 1. Cross-references are BY STREAM POSITION, not by a pre-assigned id, because the
 *    extractor cannot know a SQLite-assigned surrogate key before this file writes it:
 *      - `node.file`  = the 0-based index of a PRIOR "file" line (order of first appearance).
 *      - `node.name`  = the 0-based index of a PRIOR "str" line (extractor's local string id).
 *      - `edge.src`/`edge.dst` = the 0-based index of a PRIOR "node" line.
 *    `rel.row` values are passed through OPAQUE (no resolution): the contract gives no
 *    column-name/type metadata for a `rel` line, so there is no principled way to tell a
 *    "local ref" apart from a literal int. 2.5 should decide whether rel rows need one.
 *
 * 2. Two-phase, not single-pass streaming: phase 1 drains the WHOLE AsyncIterable into five
 *    typed arrays (str/file/node/edge/rel), so a forward reference (an edge naming a node
 *    that hasn't reached a CHUNK boundary yet) is always resolvable. Phase 2 resolves and
 *    flushes each kind in dependency order (str, file, node, edge; rel independently) in
 *    CHUNK_ROWS batches. This buffers O(total lines) fact lines in JS heap for the
 *    DURATION of one ingest call — acceptable at the ~200-line fixture / E2 scope this
 *    epic targets; E3's real-corpus stress (millions of lines) is the flag to revisit this
 *    (either a stricter extractor ordering contract that allows single-pass resolution, or
 *    chunked multi-pass re-reads from a spooled file).
 *
 * 3. New-row detection is a batched PRE-CHECK, not a `changes()`-count heuristic: the
 *    libsql local adapter's `executeMultiple` (`node_modules/@libsql/client/lib-esm/
 *    sqlite3.js:161`) always returns `undefined` — no `rowsAffected`, no `RETURNING` rows —
 *    so there is no way to learn WHICH of a chunk's candidate rows were newly inserted from
 *    the write call itself when using the existing `IStore.*_insert_batch` helpers (they all
 *    go through `executeMultiple`). Verified empirically this session (a plain `db.execute`
 *    with `RETURNING` DOES report the new rows precisely — `res.rows` gave exactly the
 *    non-conflicting rows in both a files-shaped and a node-shaped probe table — but the
 *    task's own instruction is to flush via the EXISTING batch inserts, so this file instead
 *    issues one batched, non-per-row SELECT per CHUNK against the identity columns (the
 *    `ux_node_identity` quadruple for nodes, the full PK for edges/rel rows, the existing
 *    `file_ids_by_hashes` helper for files) BEFORE the insert, and treats "not in that
 *    result" as new. One SELECT + one write per CHUNK, never per row.
 *
 * 4. Reconcile-plane addressing (`changed: [rel,row][]`, matching cascade's dense
 *    `(rel,row)` scheme in lib.ts/engine.ts):
 *      rel  = 0 (str) | 1 (file) | 2 (node) | 3 (edge) | 4 + hash(name) % 1e6 (a named
 *             `rel` line's table). The spine kinds get small fixed ids; named rels get a
 *             deterministic hash bucket — collisions across DISTINCT rel names sharing a
 *             bucket are a real (if unlikely at fixture scale) risk of this candidate
 *             scheme; 2.5 should replace it with a persisted name registry if that matters.
 *      row  = the REAL surrogate key for kinds that have one (string_id / file_id / node_id
 *             — SQLite-assigned, so already collision-free and already stable across a
 *             second identical ingest), or a deterministic hash of the row's full identity
 *             tuple for kinds that don't (edge has no surrogate; a `rel` line's PK is
 *             defined here as its FULL tuple — same "collision is a fixture-scale-only
 *             argument" caveat applies).
 *    Every reconcile row/key stays a plain `number` (not `bigint`): `cascade.key` multiplies
 *    by `KEY_STRIDE` (1e9) and both rel and row are kept well under that bound (rel via the
 *    hash-mod-1e6 bucket, row via hash-mod-1e9), so `rel*KEY_STRIDE+row` never approaches
 *    `Number.MAX_SAFE_INTEGER`.
 *
 * 5. Brand-new cells are SEEDED (`reconcile.seed`-shaped: a fresh `rx_memo` row, digest from
 *    content, no deps — a base fact reads nothing), not merely `mark_changed`-ed: a cell
 *    with no prior memo row is invisible to `dirty()` even after `mark_changed` (its
 *    `changed_at` UPDATE matches zero rows — there is nothing to update). `IRelStore` only
 *    exposes `seed_memo` per single (rel,row) — calling it once per new cell would be a
 *    per-row write — so this file writes its own batched multi-row `INSERT INTO rx_memo` via
 *    `rels.conn()`/`rels.ns()` (both public), mirroring `reconcile.seed`'s own statement
 *    shape. `rels.mark_changed(changed, rev)` is ALSO called afterward, matching the
 *    contract's literal pseudocode; for freshly-seeded cells it is a same-value no-op, and
 *    it is the correct primitive for a genuinely-revised existing cell (not reachable in
 *    THIS append-only ingest model, but kept for contract fidelity / forward compat).
 *
 * 6. Digests are `oracle.mix`-based (the one hash law every plane already agrees on):
 *    `content_digest` XORs `mix()` of each field, matching `salsa.node_digest`'s shape.
 *    Exported so a test's independent "from-scratch reference" can recompute the identical
 *    value without reaching into this file's internals.
 */

import type { IRelStore, IStore } from "./types.ts";
import { key as cascade_key, KEY_STRIDE } from "./lib.ts";
import { stmt_counter } from "./engine.ts";
import { mix } from "./oracle.ts";
import { rels as rel_tables } from "./spine.ts";
type RelCol = rel_tables.RelCol;

// ---- the candidate FactLine union (contract, verbatim) ----------------------------------
export type FactLine =
  | { t: "str"; id: number; s: string }
  | { t: "file"; hash: string; size: number; lines: number }
  | { t: "node"; family: number; file: number; start: number; len: number; kind: number; name: number | null }
  | { t: "edge"; family: number; src: number; dst: number; kind: number }
  | { t: "rel"; name: string; row: readonly unknown[] };

export interface IngestReport {
  lines: number;
  stmts: number;
  changed: [number, number][];
}

/** Rows per batched SELECT/INSERT; keeps bound-param counts and IN-list sizes small. */
const CHUNK_ROWS = 100;

// ---- reconcile-plane rel ids (design decision 4 above) -----------------------------------
export const REL_STR = 0;
export const REL_FILE = 1;
export const REL_NODE = 2;
export const REL_EDGE = 3;
const REL_NAMED_BASE = 4;
const REL_NAMED_SPACE = 1_000_000;
/** Row hashes land in [0, ROW_HASH_SPACE); rel ids land in [0, REL_NAMED_BASE+REL_NAMED_SPACE). Both stay well under KEY_STRIDE so `cascade_key` never approaches Number.MAX_SAFE_INTEGER. */
const ROW_HASH_SPACE = 1_000_000_000n;
if (REL_NAMED_BASE + REL_NAMED_SPACE >= KEY_STRIDE || ROW_HASH_SPACE > BigInt(KEY_STRIDE)) {
  throw new Error("ingest.ts hash ranges no longer fit under cascade.KEY_STRIDE — revisit the bound");
}

/** FNV-1a 64-bit fold of a part list, joined with a separator that cannot appear in a part. */
function fold_to_bigint(parts: ReadonlyArray<string | number>): bigint {
  let acc = 0xcbf29ce484222325n;
  const joined = parts.join("");
  for (let i = 0; i < joined.length; i++) {
    acc = BigInt.asUintN(64, (acc ^ BigInt(joined.charCodeAt(i))) * 0x100000001b3n);
  }
  return acc;
}

/** A deterministic dense row id in [0, ROW_HASH_SPACE) for kinds with no surrogate key. */
function hash_to_row(parts: ReadonlyArray<string | number>): number {
  return Number(BigInt.asUintN(64, mix(fold_to_bigint(parts))) % ROW_HASH_SPACE);
}

/** A named `rel` line's reconcile rel id: a deterministic hash bucket (see decision 4). */
export function rel_id_for_name(name: string): number {
  return REL_NAMED_BASE + (hash_to_row([name]) % REL_NAMED_SPACE);
}

/** `mix`-XOR digest of a content tuple (the shape `salsa.node_digest` already uses). Exported
 *  so tests can recompute the identical "from-scratch reference" independently. */
export function content_digest(parts: ReadonlyArray<number | string | bigint | null>): bigint {
  let acc = 0n;
  for (const p of parts) {
    const as_big =
      p === null ? -1n : typeof p === "bigint" ? p : typeof p === "number" ? BigInt(p) : BigInt(hash_to_row([p]));
    acc = BigInt.asUintN(64, acc ^ BigInt.asUintN(64, mix(as_big)));
  }
  return BigInt.asIntN(64, acc);
}

/** Lowercase hex, matching lib.ts's private `key_of` (unexported; duplicated here — 3 lines). */
function hex_of(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

// ---- phase 1: parse the whole stream into typed, position-indexed buffers ---------------
interface ParsedFile {
  hash: Uint8Array;
  size: number;
  lines: number;
}
interface ParsedNode {
  family: number;
  file_local: number;
  start: number;
  len: number;
  kind: number;
  name_local: number | null;
}
interface ParsedEdge {
  family: number;
  src_local: number;
  dst_local: number;
  kind: number;
}
interface ParsedRel {
  name: string;
  row: readonly unknown[];
}

interface Parsed {
  lines: number;
  strs: { id: number; s: string }[];
  files: ParsedFile[];
  nodes: ParsedNode[];
  edges: ParsedEdge[];
  rels: ParsedRel[];
}

async function parse_all(lines: AsyncIterable<string>): Promise<Parsed> {
  const out: Parsed = { lines: 0, strs: [], files: [], nodes: [], edges: [], rels: [] };
  for await (const raw of lines) {
    if (raw.trim().length === 0) continue;
    out.lines++;
    const fact = JSON.parse(raw) as FactLine;
    switch (fact.t) {
      case "str":
        out.strs.push({ id: fact.id, s: fact.s });
        break;
      case "file":
        out.files.push({ hash: Buffer.from(fact.hash, "hex"), size: fact.size, lines: fact.lines });
        break;
      case "node":
        out.nodes.push({
          family: fact.family,
          file_local: fact.file,
          start: fact.start,
          len: fact.len,
          kind: fact.kind,
          name_local: fact.name,
        });
        break;
      case "edge":
        out.edges.push({ family: fact.family, src_local: fact.src, dst_local: fact.dst, kind: fact.kind });
        break;
      case "rel":
        out.rels.push({ name: fact.name, row: fact.row });
        break;
    }
  }
  return out;
}

function node_identity_key(family: number, file_id: number, start: number, kind: number): string {
  return `${family}|${file_id}|${start}|${kind}`;
}

function chunks<T>(items: readonly T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}

// ---- phase 2: resolve + flush each kind, dependency-ordered ------------------------------

/** Strings resolve entirely in-memory (the resident Interner already mirrors the durable
 *  table); a string is "new this call" iff its assigned id is >= the count that existed
 *  before this call started (ids are dense/monotonic — see Interner in lib.ts). */
async function resolve_strs(
  store: IStore,
  strs: { id: number; s: string }[],
  changed: Map<number, [number, number]>,
): Promise<{ local_to_string_id: Map<number, number> }> {
  stmt_counter.incr();
  const before = await store.db().execute("SELECT count(*) FROM strings");
  const initial_count = Number(before.rows[0]?.[0] ?? 0);

  const local_to_string_id = new Map<number, number>();
  for (const { id, s } of strs) {
    const string_id = store.intern(s);
    local_to_string_id.set(id, string_id);
    if (string_id >= initial_count) {
      changed.set(cascade_key(REL_STR, string_id), [REL_STR, string_id]);
    }
  }
  await store.flush_strings();
  return { local_to_string_id };
}

async function resolve_files(
  store: IStore,
  files: ParsedFile[],
  changed: Map<number, [number, number]>,
): Promise<{ local_to_file_id: Map<number, number> }> {
  const local_to_file_id = new Map<number, number>();
  for (const chunk of chunks(files.map((f, i) => ({ ...f, local: i })), CHUNK_ROWS)) {
    const hashes = chunk.map((f) => f.hash);
    const before_map = await store.file_ids_by_hashes(hashes);
    await store.files_insert_batch(chunk.map((f) => [f.hash, f.size, f.lines] as const));
    const after_map = await store.file_ids_by_hashes(hashes);
    for (const f of chunk) {
      const hex = hex_of(f.hash);
      const file_id = after_map.get(hex);
      if (file_id === undefined) throw new Error(`file ${hex} vanished after insert (should be impossible)`);
      local_to_file_id.set(f.local, file_id);
      if (!before_map.has(hex)) {
        changed.set(cascade_key(REL_FILE, file_id), [REL_FILE, file_id]);
      }
    }
  }
  return { local_to_file_id };
}

async function resolve_nodes(
  store: IStore,
  nodes: ParsedNode[],
  local_to_file_id: Map<number, number>,
  local_to_string_id: Map<number, number>,
  changed: Map<number, [number, number]>,
): Promise<{ local_to_node_id: Map<number, number> }> {
  const db = store.db();
  stmt_counter.incr();
  const max_res = await db.execute("SELECT COALESCE(MAX(node_id),0) FROM node");
  let next_node_id = Number(max_res.rows[0]?.[0] ?? 0) + 1;

  const local_to_node_id = new Map<number, number>();
  const indexed = nodes.map((n, i) => ({ ...n, local: i }));
  for (const chunk of chunks(indexed, CHUNK_ROWS)) {
    const resolved = chunk.map((n) => {
      const file_id = local_to_file_id.get(n.file_local);
      if (file_id === undefined) throw new Error(`node references unresolved file local index ${n.file_local}`);
      const name_id = n.name_local === null ? null : (local_to_string_id.get(n.name_local) ?? null);
      return { ...n, file_id, name_id };
    });

    if (resolved.length > 0) {
      const values = resolved
        .map((n) => `(${n.family},${n.file_id},${n.start},${n.kind})`)
        .join(",");
      stmt_counter.incr();
      const existing = await db.execute(
        `SELECT node_id, family, file_id, byte_start, kind FROM node WHERE (family, file_id, byte_start, kind) IN (VALUES ${values})`,
      );
      const existing_map = new Map<string, number>();
      for (const row of existing.rows) {
        existing_map.set(
          node_identity_key(Number(row.family), Number(row.file_id), Number(row.byte_start), Number(row.kind)),
          Number(row.node_id),
        );
      }

      const new_rows: { node_id: number; family: number; file_id: number; start: number; len: number; kind: number; name_id: number | null }[] = [];
      for (const n of resolved) {
        const ident = node_identity_key(n.family, n.file_id, n.start, n.kind);
        const existing_id = existing_map.get(ident);
        if (existing_id !== undefined) {
          local_to_node_id.set(n.local, existing_id);
        } else {
          const node_id = next_node_id++;
          local_to_node_id.set(n.local, node_id);
          new_rows.push({ node_id, family: n.family, file_id: n.file_id, start: n.start, len: n.len, kind: n.kind, name_id: n.name_id });
        }
      }

      if (new_rows.length > 0) {
        await store.nodes_insert_batch(
          new_rows.map((r) => ({
            node_id: r.node_id,
            family: r.family,
            file_id: r.file_id,
            byte_start: r.start,
            byte_len: r.len,
            kind: r.kind,
            name_id: r.name_id,
          })),
        );
        for (const r of new_rows) {
          changed.set(cascade_key(REL_NODE, r.node_id), [REL_NODE, r.node_id]);
        }
      }
    }
  }
  return { local_to_node_id };
}

async function resolve_edges(
  store: IStore,
  edges: ParsedEdge[],
  local_to_node_id: Map<number, number>,
  changed: Map<number, [number, number]>,
): Promise<void> {
  const db = store.db();
  for (const chunk of chunks(edges, CHUNK_ROWS)) {
    const resolved = chunk.map((e) => {
      const src_id = local_to_node_id.get(e.src_local);
      const dst_id = local_to_node_id.get(e.dst_local);
      if (src_id === undefined || dst_id === undefined) {
        throw new Error(`edge references unresolved node local index (${e.src_local} -> ${e.dst_local})`);
      }
      return { family: e.family, src_id, dst_id, kind: e.kind };
    });
    if (resolved.length === 0) continue;

    const values = resolved.map((e) => `(${e.family},${e.src_id},${e.dst_id},${e.kind})`).join(",");
    stmt_counter.incr();
    const existing = await db.execute(
      `SELECT family, src_id, dst_id, kind FROM edge WHERE (family, src_id, dst_id, kind) IN (VALUES ${values})`,
    );
    const existing_set = new Set<string>();
    for (const row of existing.rows) {
      existing_set.add(node_identity_key(Number(row.family), Number(row.src_id), Number(row.dst_id), Number(row.kind)));
    }

    const new_rows = resolved.filter(
      (e) => !existing_set.has(node_identity_key(e.family, e.src_id, e.dst_id, e.kind)),
    );
    if (new_rows.length > 0) {
      await store.edges_insert_batch(new_rows.map((e) => ({ family: e.family, src_id: e.src_id, dst_id: e.dst_id, kind: e.kind })));
      for (const e of new_rows) {
        const rel_row = hash_to_row([e.family, e.src_id, e.dst_id, e.kind]);
        changed.set(cascade_key(REL_EDGE, rel_row), [REL_EDGE, rel_row]);
      }
    }
  }
}

/** Infer a rel table's column kind (int/int_null/text) from every value seen at that
 *  position, across ALL of this ingest call's rows for that name (rel lines are fully
 *  buffered by phase 1, so this is a complete view, not a first-row guess). */
function infer_cols(rows: readonly (readonly unknown[])[]): RelCol[] {
  const width = rows.reduce((m, r) => Math.max(m, r.length), 0);
  const cols: RelCol[] = [];
  for (let i = 0; i < width; i++) {
    let saw_text = false;
    let saw_null = false;
    for (const row of rows) {
      const v = row[i];
      if (v === null || v === undefined) saw_null = true;
      else if (typeof v === "string") saw_text = true;
    }
    const name = `c${i}`;
    cols.push(saw_text ? rel_tables.rel_col_text(name) : saw_null ? rel_tables.rel_col_int_null(name) : rel_tables.rel_col_int(name));
  }
  return cols;
}

function sql_literal(v: unknown): string {
  if (v === null || v === undefined) return "NULL";
  if (typeof v === "number" || typeof v === "bigint") return v.toString();
  if (typeof v === "boolean") return v ? "1" : "0";
  return `'${String(v).replace(/'/g, "''")}'`;
}

async function resolve_rels(store: IStore, rel_lines: ParsedRel[], changed: Map<number, [number, number]>): Promise<void> {
  const db = store.db();
  const by_name = new Map<string, readonly unknown[][]>();
  for (const r of rel_lines) {
    const list = by_name.get(r.name);
    if (list) (list as unknown[][]).push(r.row as unknown[]);
    else by_name.set(r.name, [r.row as unknown[]]);
  }

  for (const [name, rows] of by_name) {
    const cols = infer_cols(rows);
    const col_names = cols.map((c) => c.name);
    await rel_tables.create_rel_table(db, name, cols, col_names);
    const rel_id = rel_id_for_name(name);

    for (const chunk of chunks(rows, CHUNK_ROWS)) {
      if (chunk.length === 0) continue;
      const values = chunk.map((row) => `(${col_names.map((_, i) => sql_literal(row[i])).join(",")})`).join(",");
      stmt_counter.incr();
      const existing = await db.execute(
        `SELECT ${col_names.join(",")} FROM ${name} WHERE (${col_names.join(",")}) IN (VALUES ${values})`,
      );
      const existing_set = new Set<string>();
      for (const row of existing.rows) {
        existing_set.add(col_names.map((c) => String(row[c as unknown as number])).join("|"));
      }

      const new_rows = chunk.filter((row) => !existing_set.has(col_names.map((_, i) => String(row[i])).join("|")));
      if (new_rows.length > 0) {
        const insert_values = new_rows
          .map((row) => `(${col_names.map((_, i) => sql_literal(row[i])).join(",")})`)
          .join(",");
        stmt_counter.incr();
        await db.execute(`INSERT INTO ${name}(${col_names.join(",")}) VALUES ${insert_values} ON CONFLICT DO NOTHING`);
        for (const row of new_rows) {
          const rel_row = hash_to_row([name, ...row.map((v) => String(v))]);
          changed.set(cascade_key(rel_id, rel_row), [rel_id, rel_row]);
        }
      }
    }
  }
}

/** Batched multi-row `rx_memo` seed for brand-new cells (design decision 5). Mirrors
 *  `reconcile.seed`'s statement shape but covers the WHOLE new-cell set in CHUNK_ROWS
 *  batches instead of one `IRelStore.seed_memo` call per cell (that would be per-row). */
async function seed_new_cells(rels: IRelStore, cells: readonly [number, number][], rev: number): Promise<void> {
  const ns = rels.ns();
  for (const chunk of chunks(cells, CHUNK_ROWS)) {
    if (chunk.length === 0) continue;
    const values = chunk
      .map(([rel, row]) => {
        const digest = content_digest([rel, row]);
        return `(${cascade_key(rel, row)},${digest},${rev},${rev})`;
      })
      .join(",");
    stmt_counter.incr();
    await rels.conn().execute(
      `INSERT INTO ${ns.memo}(id,digest,changed_at,verified_at) VALUES ${values} ON CONFLICT(id) DO NOTHING`,
    );
  }
}

/**
 * ingestJsonl(store, rels, lines, rev) -> IngestReport
 *
 * Phase 1: drain `lines` into typed buffers (parse_all).
 * Phase 2: resolve + flush str -> file -> node -> edge -> rel, in that dependency order,
 *          each in CHUNK_ROWS batches; every new (rel,row) cell lands in `changed`.
 * Phase 3: seed_new_cells (fresh rx_memo rows) + rels.mark_changed(changed, rev) (contract
 *          fidelity — a no-op for the cells just seeded, the correct primitive for a
 *          genuinely-revised existing one).
 */
export async function ingestJsonl(
  store: IStore,
  rels: IRelStore,
  lines: AsyncIterable<string>,
  rev: number,
): Promise<IngestReport> {
  const stmts_before = stmt_counter.get();
  const parsed = await parse_all(lines);

  const changed = new Map<number, [number, number]>();
  const { local_to_string_id } = await resolve_strs(store, parsed.strs, changed);
  const { local_to_file_id } = await resolve_files(store, parsed.files, changed);
  const { local_to_node_id } = await resolve_nodes(
    store,
    parsed.nodes,
    local_to_file_id,
    local_to_string_id,
    changed,
  );
  await resolve_edges(store, parsed.edges, local_to_node_id, changed);
  await resolve_rels(store, parsed.rels, changed);

  const changed_list = [...changed.values()];
  await seed_new_cells(rels, changed_list, rev);
  if (changed_list.length > 0) {
    await rels.mark_changed(changed_list, rev);
  }

  return {
    lines: parsed.lines,
    stmts: stmt_counter.get() - stmts_before,
    changed: changed_list,
  };
}
