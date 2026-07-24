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
 * OWNER AMENDMENT (folded into M9-before, string-FK-ban gate prep): every table also
 * carries its column affinity dump (`PRAGMA table_info`, one {name, type, pk} per
 * column) — the AFTER-gate at integration fails on any join column with TEXT affinity
 * (the ruling is ZERO string foreign keys: no TEXT column referencing another row —
 * paths, rel names, host names, hex digests included). aggregates.text_affinity_columns
 * flags every column, across every table, whose declared type is empty (the rel_*
 * tables' columns carry NO declared type at all -- see src/2_schema.ts's `ddl()`: the
 * CREATE TABLE column list is bare names, so PRAGMA table_info reports type="" for
 * all of them, BLOB affinity by SQLite's rules, used in practice to store paths/hex
 * digests as TEXT) or textually contains CHAR/CLOB/TEXT — the honest BEFORE picture
 * this schema's join columns present, prior to any interned-id rework.
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

/** PRAGMA table_info(<table>) -> [{name, type, pk}] in column-definition order. `type`
 *  is the RAW declared type string (often "" for this schema's untyped rel_* columns —
 *  see the file header). `pk` is the 1-based position of the column within a composite
 *  PRIMARY KEY (0 when the column is not part of the PK), straight from table_info's
 *  own `pk` field. */
async function columnInfo(db, tableName) {
  const res = await db.execute(`PRAGMA table_info("${tableName.replace(/"/g, '""')}")`);
  // table_info rows already come back in column-definition (cid) order; kept as-is.
  return res.rows.map((row) => ({
    name: String(row.name),
    type: String(row.type ?? ""),
    pk: Number(row.pk ?? 0),
  }));
}

function isTextAffinity(declaredType) {
  return declaredType === "" || /char|clob|text/i.test(declaredType);
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
        const columns = await columnInfo(db, row.name);
        tables.push({
          name: row.name,
          bytes: row.bytes,
          pages: row.pages,
          rows,
          bytes_per_row: rows > 0 ? row.bytes / rows : null,
          columns,
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

    // Owner amendment: string-FK-ban gate prep. Every column, across every table,
    // whose declared type is empty (this schema's rel_* columns carry none at all)
    // or textually names a TEXT-ish SQLite type affinity (CHAR/CLOB/TEXT).
    const textAffinityColumns = [];
    for (const table of tables) {
      for (const column of table.columns) {
        if (isTextAffinity(column.type)) {
          textAffinityColumns.push({ table: table.name, column: column.name, type: column.type || "(none)" });
        }
      }
    }

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
        text_affinity_columns: textAffinityColumns,
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
