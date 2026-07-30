#!/usr/bin/env node
/**
 * Runnable receipts for plans/2026-07-30-null-coherence-lab.md.
 *
 * Run:
 *   SPREFA_CONFIG=/nonexistent/null-coherence.toml \
 *   DL_NO_DAEMON=1 \
 *   node plans/2026-07-30-null-coherence-receipts.mjs
 *
 * The same SQL assertions run through the system sqlite3 CLI and the
 * repository's locked @libsql/client dependency. Every receipt gets a fresh
 * database under the operating system temporary directory.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

process.env.SPREFA_CONFIG ??= "/nonexistent/null-coherence.toml";
process.env.DL_NO_DAEMON ??= "1";

const repositoryPackageRequire = createRequire(
  new URL("../v6/dl/package.json", import.meta.url),
);
const libsqlEntryUrl = pathToFileURL(
  repositoryPackageRequire.resolve("@libsql/client"),
).href;
const { createClient } = await import(libsqlEntryUrl);

const receipts = [
  {
    name: "J1 json1 absent versus json null",
    setup: [],
    query: `
      WITH documents(label, document) AS (
        VALUES ('absent', '{}'), ('json-null', '{"k":null}')
      )
      SELECT
        label,
        json_extract(document, '$.k') AS extract_value,
        typeof(json_extract(document, '$.k')) AS extract_sql_type,
        json_type(document, '$.k') AS json_type_at_path,
        document -> '$.k' AS arrow_json,
        typeof(document -> '$.k') AS arrow_sql_type,
        document ->> '$.k' AS arrow_text,
        (SELECT count(*) FROM json_each(document) WHERE key = 'k') AS each_rows
      FROM documents
      ORDER BY label
    `,
    expected: [
      {
        label: "absent",
        extract_value: null,
        extract_sql_type: "null",
        json_type_at_path: null,
        arrow_json: null,
        arrow_sql_type: "null",
        arrow_text: null,
        each_rows: 0,
      },
      {
        label: "json-null",
        extract_value: null,
        extract_sql_type: "null",
        json_type_at_path: "null",
        arrow_json: "null",
        arrow_sql_type: "text",
        arrow_text: null,
        each_rows: 1,
      },
    ],
  },
  {
    name: "L1 comparison truth table",
    setup: [],
    query: `
      SELECT
        NULL = NULL AS equals_result,
        NULL != NULL AS not_equals_result,
        NULL IS NULL AS is_result,
        NULL IS NOT NULL AS is_not_result,
        NULL IS NOT DISTINCT FROM NULL AS null_safe_equal,
        1 IS NOT DISTINCT FROM NULL AS value_null_safe_equal,
        NOT NULL AS not_unknown,
        NULL AND 0 AS unknown_and_false,
        NULL AND 1 AS unknown_and_true,
        NULL OR 0 AS unknown_or_false,
        NULL OR 1 AS unknown_or_true,
        (SELECT count(*) FROM (SELECT 1) WHERE NULL = NULL) AS equals_filter_rows,
        (SELECT count(*) FROM (SELECT 1) WHERE NULL != NULL) AS not_equals_filter_rows
    `,
    expected: [
      {
        equals_result: null,
        not_equals_result: null,
        is_result: 1,
        is_not_result: 0,
        null_safe_equal: 1,
        value_null_safe_equal: 0,
        not_unknown: null,
        unknown_and_false: 0,
        unknown_and_true: null,
        unknown_or_false: null,
        unknown_or_true: 1,
        equals_filter_rows: 0,
        not_equals_filter_rows: 0,
      },
    ],
  },
  {
    name: "L2 NULL inside NOT EXISTS",
    setup: [],
    query: `
      WITH
        outer_values(value) AS (VALUES (NULL), (1), (2)),
        inner_values(value) AS (VALUES (NULL), (1))
      SELECT
        CASE WHEN outer_values.value IS NULL
          THEN '<NULL>'
          ELSE CAST(outer_values.value AS TEXT)
        END AS outer_value,
        NOT EXISTS (
          SELECT 1
          FROM inner_values
          WHERE inner_values.value = outer_values.value
        ) AS equals_not_exists,
        NOT EXISTS (
          SELECT 1
          FROM inner_values
          WHERE inner_values.value IS NOT DISTINCT FROM outer_values.value
        ) AS null_safe_not_exists
      FROM outer_values
      ORDER BY outer_values.value
    `,
    expected: [
      { outer_value: "<NULL>", equals_not_exists: 1, null_safe_not_exists: 0 },
      { outer_value: "1", equals_not_exists: 0, null_safe_not_exists: 0 },
      { outer_value: "2", equals_not_exists: 1, null_safe_not_exists: 1 },
    ],
  },
  {
    name: "L3 GROUP BY and aggregate null handling",
    setup: [],
    query: `
      WITH samples(group_name, value) AS (
        VALUES (NULL, NULL), (NULL, 2), ('named', 5), ('named', NULL)
      )
      SELECT
        CASE WHEN group_name IS NULL THEN '<NULL>' ELSE group_name END AS group_name,
        count(*) AS row_count,
        count(value) AS value_count,
        sum(value) AS value_sum,
        min(value) AS value_min,
        max(value) AS value_max
      FROM samples
      GROUP BY group_name
      ORDER BY group_name
    `,
    expected: [
      {
        group_name: "<NULL>",
        row_count: 2,
        value_count: 1,
        value_sum: 2,
        value_min: 2,
        value_max: 2,
      },
      {
        group_name: "named",
        row_count: 2,
        value_count: 1,
        value_sum: 5,
        value_min: 5,
        value_max: 5,
      },
    ],
  },
  {
    name: "L4 aggregates over only null inputs",
    setup: [],
    query: `
      WITH samples(value) AS (VALUES (NULL), (NULL))
      SELECT
        count(*) AS row_count,
        count(value) AS value_count,
        sum(value) AS value_sum,
        min(value) AS value_min,
        max(value) AS value_max
      FROM samples
    `,
    expected: [
      {
        row_count: 2,
        value_count: 0,
        value_sum: null,
        value_min: null,
        value_max: null,
      },
    ],
  },
  {
    name: "L5 ORDER BY null placement",
    setup: [],
    query: `
      WITH samples(value) AS (VALUES (NULL), (2), (1))
      SELECT
        (SELECT group_concat(label, ',') FROM (
          SELECT CASE WHEN value IS NULL THEN '<NULL>' ELSE CAST(value AS TEXT) END AS label
          FROM samples ORDER BY value ASC
        )) AS ascending_default,
        (SELECT group_concat(label, ',') FROM (
          SELECT CASE WHEN value IS NULL THEN '<NULL>' ELSE CAST(value AS TEXT) END AS label
          FROM samples ORDER BY value DESC
        )) AS descending_default,
        (SELECT group_concat(label, ',') FROM (
          SELECT CASE WHEN value IS NULL THEN '<NULL>' ELSE CAST(value AS TEXT) END AS label
          FROM samples ORDER BY value ASC NULLS LAST
        )) AS ascending_nulls_last,
        (SELECT group_concat(label, ',') FROM (
          SELECT CASE WHEN value IS NULL THEN '<NULL>' ELSE CAST(value AS TEXT) END AS label
          FROM samples ORDER BY value DESC NULLS FIRST
        )) AS descending_nulls_first
    `,
    expected: [
      {
        ascending_default: "<NULL>,1,2",
        descending_default: "2,1,<NULL>",
        ascending_nulls_last: "1,2,<NULL>",
        descending_nulls_first: "<NULL>,2,1",
      },
    ],
  },
  {
    name: "L6 DISTINCT equality inconsistency",
    setup: [],
    query: `
      WITH samples(value) AS (VALUES (NULL), (NULL), (1))
      SELECT
        (SELECT count(*) FROM (SELECT DISTINCT value FROM samples)) AS distinct_rows,
        (SELECT count(*) FROM samples left_side JOIN samples right_side
          ON left_side.value = right_side.value
          WHERE left_side.value IS NULL) AS equals_null_join_rows,
        (SELECT count(*) FROM samples left_side JOIN samples right_side
          ON left_side.value IS NOT DISTINCT FROM right_side.value
          WHERE left_side.value IS NULL) AS null_safe_join_rows
    `,
    expected: [
      {
        distinct_rows: 2,
        equals_null_join_rows: 0,
        null_safe_join_rows: 4,
      },
    ],
  },
  {
    name: "K1 UNIQUE admits multiple null keys",
    setup: [
      `CREATE TABLE unique_keys (key_value TEXT UNIQUE, payload TEXT NOT NULL)`,
      `INSERT INTO unique_keys VALUES (NULL, 'first'), (NULL, 'second')`,
    ],
    query: `
      SELECT count(*) AS stored_rows, group_concat(payload, ',') AS payloads
      FROM unique_keys
    `,
    expected: [{ stored_rows: 2, payloads: "first,second" }],
  },
  {
    name: "K2 current ON CONFLICT upsert accumulates on a null key",
    setup: [
      `CREATE TABLE keyed_upsert (key_value TEXT UNIQUE, payload TEXT NOT NULL)`,
      `INSERT INTO keyed_upsert VALUES (NULL, 'old')`,
      `INSERT INTO keyed_upsert VALUES (NULL, 'new')
        ON CONFLICT (key_value) DO UPDATE SET payload = excluded.payload`,
    ],
    query: `
      SELECT count(*) AS stored_rows, group_concat(payload, ',') AS payloads
      FROM keyed_upsert
    `,
    expected: [{ stored_rows: 2, payloads: "old,new" }],
  },
  {
    name: "K3 INSERT OR REPLACE accumulates on a null key",
    setup: [
      `CREATE TABLE keyed_replace (key_value TEXT UNIQUE, payload TEXT NOT NULL)`,
      `INSERT OR REPLACE INTO keyed_replace VALUES (NULL, 'old')`,
      `INSERT OR REPLACE INTO keyed_replace VALUES (NULL, 'new')`,
    ],
    query: `
      SELECT count(*) AS stored_rows, group_concat(payload, ',') AS payloads
      FROM keyed_replace
    `,
    expected: [{ stored_rows: 2, payloads: "old,new" }],
  },
  {
    name: "K4 ordinary PRIMARY KEY also admits multiple null keys",
    setup: [
      `CREATE TABLE ordinary_primary_key (key_value TEXT PRIMARY KEY, payload TEXT NOT NULL)`,
      `INSERT OR REPLACE INTO ordinary_primary_key VALUES (NULL, 'old')`,
      `INSERT OR REPLACE INTO ordinary_primary_key VALUES (NULL, 'new')`,
    ],
    query: `
      SELECT count(*) AS stored_rows, group_concat(payload, ',') AS payloads
      FROM ordinary_primary_key
    `,
    expected: [{ stored_rows: 2, payloads: "old,new" }],
  },
  {
    name: "K5 WITHOUT ROWID primary key refuses null",
    setup: [
      `CREATE TABLE without_rowid_primary_key (
        key_value TEXT,
        payload TEXT NOT NULL,
        PRIMARY KEY (key_value)
      ) WITHOUT ROWID`,
    ],
    query: `INSERT INTO without_rowid_primary_key VALUES (NULL, 'value')`,
    errorIncludes: "NOT NULL constraint failed",
  },
  {
    name: "K6 equality cannot find a stored null key",
    setup: [
      `CREATE TABLE nullable_lookup (key_value TEXT, payload TEXT NOT NULL)`,
      `INSERT INTO nullable_lookup VALUES (NULL, 'stored')`,
    ],
    query: `
      SELECT
        count(*) FILTER (WHERE key_value = NULL) AS equals_matches,
        count(*) FILTER (
          WHERE key_value IS NOT DISTINCT FROM NULL
        ) AS null_safe_matches
      FROM nullable_lookup
    `,
    expected: [{ equals_matches: 0, null_safe_matches: 1 }],
  },
  {
    name: "D1 boundary grouping for null transitions",
    setup: [
      `CREATE TABLE delta_events (
        scenario TEXT NOT NULL,
        sign INTEGER NOT NULL,
        key_value TEXT,
        payload TEXT
      )`,
      `INSERT INTO delta_events VALUES
        ('null-to-null', -1, 'repo', NULL),
        ('null-to-null',  1, 'repo', NULL),
        ('null-to-value', -1, 'repo', NULL),
        ('null-to-value',  1, 'repo', 'commit-1')`,
    ],
    query: `
      SELECT
        scenario,
        key_value,
        payload,
        sum(sign) AS net_weight,
        count(*) AS staged_events
      FROM delta_events
      GROUP BY scenario, key_value, payload
      ORDER BY scenario, payload
    `,
    expected: [
      {
        scenario: "null-to-null",
        key_value: "repo",
        payload: null,
        net_weight: 0,
        staged_events: 2,
      },
      {
        scenario: "null-to-value",
        key_value: "repo",
        payload: null,
        net_weight: -1,
        staged_events: 1,
      },
      {
        scenario: "null-to-value",
        key_value: "repo",
        payload: "commit-1",
        net_weight: 1,
        staged_events: 1,
      },
    ],
  },
];

function normalizeRows(columns, rawRows) {
  return rawRows.map((rawRow) =>
    Object.fromEntries(
      columns.map((column) => {
        const value = rawRow[column];
        return [column, typeof value === "bigint" ? Number(value) : value];
      }),
    ),
  );
}

function runSqliteReceipt(databasePath, receipt) {
  const sql = [...receipt.setup, receipt.query].join(";\n");
  const output = execFileSync("sqlite3", ["-json", databasePath, sql], {
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  return output.trim() === "" ? [] : JSON.parse(output);
}

async function runLibsqlReceipt(databasePath, receipt) {
  const client = createClient({ url: `file:${databasePath}` });
  try {
    if (receipt.setup.length > 0) {
      await client.executeMultiple(receipt.setup.join(";\n") + ";");
    }
    const result = await client.execute(receipt.query);
    return normalizeRows(result.columns, result.rows);
  } finally {
    client.close();
  }
}

async function assertReceipt(engineName, receipt, runReceipt, databasePath) {
  if (receipt.errorIncludes !== undefined) {
    let message = "";
    try {
      await runReceipt(databasePath, receipt);
    } catch (error) {
      message = String(error?.stderr ?? error?.message ?? error);
    }
    assert.match(
      message,
      new RegExp(receipt.errorIncludes),
      `${engineName} ${receipt.name}`,
    );
    console.log(`PASS ${engineName} ${receipt.name}: ${receipt.errorIncludes}`);
    return;
  }

  const actual = await runReceipt(databasePath, receipt);
  assert.deepEqual(actual, receipt.expected, `${engineName} ${receipt.name}`);
  console.log(`PASS ${engineName} ${receipt.name}: ${JSON.stringify(actual)}`);
}

async function runEngine(engineName, extension, runReceipt, scratchDirectory) {
  console.log(`\n${engineName}`);
  for (const [receiptIndex, receipt] of receipts.entries()) {
    const databasePath = join(
      scratchDirectory,
      `${engineName.replaceAll(/[^a-z0-9]/gi, "_")}-${receiptIndex}.${extension}`,
    );
    await assertReceipt(
      engineName,
      receipt,
      runReceipt,
      databasePath,
    );
  }
}

const scratchDirectory = mkdtempSync(join(tmpdir(), "sprefa-null-coherence-"));
try {
  const sqliteVersion = execFileSync("sqlite3", ["--version"], {
    encoding: "utf8",
    env: process.env,
  }).trim();
  const libsqlVersionClient = createClient({
    url: `file:${join(scratchDirectory, "libsql-version.db")}`,
  });
  const libsqlVersionResult = await libsqlVersionClient.execute(
    "SELECT sqlite_version() AS version",
  );
  libsqlVersionClient.close();
  const libsqlVersion = String(libsqlVersionResult.rows[0].version);

  console.log(`SPREFA_CONFIG=${process.env.SPREFA_CONFIG}`);
  console.log(`DL_NO_DAEMON=${process.env.DL_NO_DAEMON}`);
  console.log(`sqlite3 CLI=${sqliteVersion}`);
  console.log(`@libsql SQLite=${libsqlVersion}`);

  await runEngine("sqlite3-cli", "sqlite3", runSqliteReceipt, scratchDirectory);
  await runEngine("@libsql", "libsql", runLibsqlReceipt, scratchDirectory);
  console.log(`\nPASS ${receipts.length * 2} dual-engine receipt assertions`);
} finally {
  rmSync(scratchDirectory, { recursive: true, force: true });
}
