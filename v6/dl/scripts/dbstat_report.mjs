#!/usr/bin/env node
/**
 * scripts/dbstat_report.mjs — generic SQLite dbstat reporter (M9-before measurement
 * harness, EngineStorageLaw pin, tasks.d.ts). Plain node + @libsql/client, no other deps.
 *
 * Usage: node scripts/dbstat_report.mjs <db-path> [out-json]
 *
 * Opens <db-path>, checkpoints the WAL (PRAGMA wal_checkpoint(TRUNCATE)) so the main
 * file's byte count is the honest on-disk total rather than a WAL-inflated snapshot,
 * then reads the `dbstat` virtual table: one row per name (table or index) with total
 * page bytes and page count. Primary path is a plain SELECT through @libsql/client;
 * if that build lacks the `dbstat` virtual table (it is compiled-in but not universal
 * across libsql builds), this falls back to shelling out to the `sqlite3` CLI for the
 * exact same query and continues transparently — the emitted report's "_meta.dbstat_path"
 * field always names which one actually ran.
 *
 * Every table's row count comes from one `SELECT count(*) FROM "<table>"` per table.
 * That is an intentional N+1 here (unlike engine code, which the repo's own N+1 law
 * forbids): this is a one-shot diagnostic script over a handful of tables, not a
 * per-request or per-tick hot path.
 *
 * Output: pretty JSON to stdout, and to <out-json> when given.
 */
import { createClient } from "@libsql/client";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/** @typedef {{ name: string | null, bytes: number, pages: number }} DbstatRow */

const DBSTAT_SQL = "SELECT name, sum(pgsize) AS bytes, count(*) AS pages FROM dbstat GROUP BY name";

/** Tries the dbstat query over the already-open libsql client. Returns null (never
 *  throws) so the caller can fall back to the sqlite3 CLI transparently. */
async function tryDbstatViaLibsql(db) {
  try {
    const res = await db.execute(DBSTAT_SQL);
    return res.rows.map((row) => ({
      name: row.name === null || row.name === undefined ? null : String(row.name),
      bytes: Number(row.bytes ?? 0),
      pages: Number(row.pages ?? 0),
    }));
  } catch {
    return null;
  }
}

/** Fallback: shell out to the sqlite3 CLI for the identical dbstat query, parsed from
 *  its `-json` output mode. Only reached when the libsql build's dbstat virtual table
 *  is unavailable. */
function dbstatViaSqlite3Cli(dbPath) {
  const stdout = execFileSync("sqlite3", ["-readonly", "-json", dbPath, DBSTAT_SQL], { encoding: "utf8" });
  const trimmed = stdout.trim();
  if (trimmed.length === 0) return [];
  /** @type {Array<{ name: string | null, bytes: number | string, pages: number | string }>} */
  const parsed = JSON.parse(trimmed);
  return parsed.map((row) => ({
    name: row.name === null || row.name === undefined ? null : String(row.name),
    bytes: Number(row.bytes ?? 0),
    pages: Number(row.pages ?? 0),
  }));
}

/** name -> { type: 'table' | 'index', tbl_name: string } for every real table/index in
 *  the schema (sqlite_master never lists itself, so the schema btree itself falls
 *  through to the "other" bucket below — that is intentional, not a bug). */
async function schemaMap(db) {
  const res = await db.execute("SELECT name, type, tbl_name FROM sqlite_master WHERE type IN ('table', 'index')");
  const map = new Map();
  for (const row of res.rows) {
    map.set(String(row.name), { type: String(row.type), tblName: String(row.tbl_name) });
  }
  return map;
}

async function rowCount(db, tableName) {
  // Table names are drawn straight from sqlite_master, never user input; still quoted
  // defensively since a couple of spine/derived names are SQL keywords-adjacent.
  const res = await db.execute(`SELECT count(*) AS n FROM "${tableName.replace(/"/g, '""')}"`);
  return Number(res.rows[0]?.n ?? 0);
}

function isRelPlaneName(name) {
  return /^rel_/.test(name);
}

async function buildReport(dbPath) {
  const db = createClient({ url: `file:${dbPath}` });
  let dbstatPath = "libsql";
  try {
    try {
      await db.execute("PRAGMA wal_checkpoint(TRUNCATE)");
    } catch {
      // Not every db is in WAL mode; a failed checkpoint just means there is nothing
      // to fold back into the main file, which is fine for the byte count below.
    }

    let dbstatRows = await tryDbstatViaLibsql(db);
    if (dbstatRows === null) {
      dbstatPath = "sqlite3-cli";
      dbstatRows = dbstatViaSqlite3Cli(dbPath);
    }

    const schema = await schemaMap(db);

    const tables = [];
    const indexes = [];
    let otherBytes = 0;

    for (const row of dbstatRows) {
      if (row.name === null) {
        otherBytes += row.bytes; // freelist pages: dbstat reports these with a NULL name.
        continue;
      }
      const entry = schema.get(row.name);
      if (entry === undefined) {
        otherBytes += row.bytes; // e.g. the sqlite_schema btree itself.
        continue;
      }
      if (entry.type === "table") {
        const rows = await rowCount(db, row.name);
        tables.push({
          name: row.name,
          bytes: row.bytes,
          pages: row.pages,
          rows,
          bytes_per_row: rows > 0 ? row.bytes / rows : null,
        });
      } else {
        indexes.push({ name: row.name, bytes: row.bytes, pages: row.pages, owning_table: entry.tblName });
      }
    }

    tables.sort((a, b) => b.bytes - a.bytes);
    indexes.sort((a, b) => b.bytes - a.bytes);

    const fileBytes = fs.statSync(dbPath).size;
    const tableBytes = tables.reduce((sum, t) => sum + t.bytes, 0);
    const indexBytes = indexes.reduce((sum, i) => sum + i.bytes, 0);

    const relTables = tables.filter((t) => isRelPlaneName(t.name));
    const relIndexes = indexes.filter((i) => isRelPlaneName(i.owning_table));
    const relTableBytes = relTables.reduce((sum, t) => sum + t.bytes, 0);
    const relIndexBytes = relIndexes.reduce((sum, i) => sum + i.bytes, 0);

    return {
      _meta: {
        dbstat_path: dbstatPath,
        db_path: path.resolve(dbPath),
      },
      tables,
      indexes,
      aggregates: {
        file_bytes: fileBytes,
        table_bytes: tableBytes,
        index_bytes: indexBytes,
        other_bytes: otherBytes,
        // Ratio of the WHOLE file taken by index bytes (all indexes, every table).
        index_ratio_of_file: fileBytes > 0 ? indexBytes / fileBytes : null,
        rel_plane: {
          tables: relTables.map((t) => t.name),
          table_bytes: relTableBytes,
          index_bytes: relIndexBytes,
          // Ratio scoped to the rel plane itself: index_bytes / (table_bytes + index_bytes)
          // among names matching /^rel_/ plus every index (named or sqlite_autoindex_*)
          // whose owning table matches /^rel_/.
          index_ratio: relTableBytes + relIndexBytes > 0 ? relIndexBytes / (relTableBytes + relIndexBytes) : null,
        },
      },
    };
  } finally {
    db.close();
  }
}

async function main() {
  const [, , dbPath, outJson] = process.argv;
  if (!dbPath) {
    console.error("usage: node scripts/dbstat_report.mjs <db-path> [out-json]");
    process.exit(1);
  }
  const report = await buildReport(dbPath);
  const text = `${JSON.stringify(report, null, 2)}\n`;
  process.stdout.write(text);
  if (outJson) {
    fs.mkdirSync(path.dirname(outJson), { recursive: true });
    fs.writeFileSync(outJson, text);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
