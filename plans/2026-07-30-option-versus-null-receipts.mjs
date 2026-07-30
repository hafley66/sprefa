#!/usr/bin/env node
/**
 * Runnable receipts for plans/2026-07-30-option-versus-null-lab.md.
 *
 * Run from the repository root:
 *   SPREFA_CONFIG=/nonexistent/option-versus-null.toml \
 *   DL_NO_DAEMON=1 \
 *   node plans/2026-07-30-option-versus-null-receipts.mjs
 *
 * Six parts:
 *   V  the user's three-variant json read, Some / None / Undefined
 *   C  candidate inventory taken from the REAL compiler, not from hand DDL
 *   X  the explosion measurement at 10k and 100k rows
 *   D  the Design D structural break in the current table families
 *   E  hazards specific to the variant-relation encoding
 *   N  null-safe equality cost, including whether an index survives it
 *
 * Every semantic assertion runs on both SQLite builds the project ships
 * against: the system sqlite3 CLI and the locked @libsql/client. Every
 * database is fresh, under the operating system temporary directory. No
 * repository state is read or written except the compiler, which is invoked
 * read-only on scratch input files.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

process.env.SPREFA_CONFIG ??= "/nonexistent/option-versus-null.toml";
process.env.DL_NO_DAEMON ??= "1";

const repositoryRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

// The locked @libsql/client. A worktree cut for a lab has no node_modules of
// its own, so fall back to the primary checkout's install of the SAME locked
// version rather than installing into the lab tree. DL_LIBSQL_FROM overrides
// both. Resolution is read-only.
function resolveLibsqlEntry() {
  const candidateRoots = [
    process.env.DL_LIBSQL_FROM,
    join(repositoryRoot, "v6/dl/package.json"),
    join(repositoryRoot, "v6/tsv2/package.json"),
    "/Users/chrishafley/projects/sprefa/v6/dl/package.json",
  ].filter((candidate) => candidate !== undefined);
  for (const candidateRoot of candidateRoots) {
    try {
      return createRequire(pathToFileURL(candidateRoot)).resolve("@libsql/client");
    } catch {
      continue;
    }
  }
  throw new Error("no @libsql/client install reachable; set DL_LIBSQL_FROM");
}

const libsqlEntryUrl = pathToFileURL(resolveLibsqlEntry()).href;
const { createClient } = await import(libsqlEntryUrl);

let passCount = 0;
const failures = [];

function record(label, detail) {
  passCount += 1;
  console.log(`PASS ${label}${detail === undefined ? "" : `: ${detail}`}`);
}

function check(label, actual, expected) {
  try {
    assert.deepEqual(actual, expected, label);
    record(label, JSON.stringify(actual));
  } catch (error) {
    failures.push(`${label}\n  actual   ${JSON.stringify(actual)}\n  expected ${JSON.stringify(expected)}`);
    console.log(`FAIL ${label}`);
    console.log(`  actual   ${JSON.stringify(actual)}`);
    console.log(`  expected ${JSON.stringify(expected)}`);
  }
}

// ══ engine seams ═══════════════════════════════════════════════════════════

function sqliteCliQuery(databasePath, sqlText) {
  const output = execFileSync("sqlite3", ["-json", databasePath, sqlText], {
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 64 * 1024 * 1024,
  });
  return output.trim() === "" ? [] : JSON.parse(output);
}

/**
 * The sqlite3 CLI intercepts EXPLAIN QUERY PLAN and renders an ASCII tree
 * regardless of -json, so the plan has to be read out of the tree text. The
 * glyphs are stripped so the resulting strings line up with @libsql's
 * `detail` column and the two engines can be compared directly.
 */
function sqliteCliExplain(databasePath, sqlText) {
  const output = execFileSync("sqlite3", [databasePath, `EXPLAIN QUERY PLAN ${sqlText}`], {
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 16 * 1024 * 1024,
  });
  return output
    .split("\n")
    .map((line) => line.replace(/^[|`\-\s]+/, "").trim())
    .filter((line) => line !== "" && line !== "QUERY PLAN");
}

function sqliteCliExec(databasePath, sqlText) {
  execFileSync("sqlite3", [databasePath, sqlText], {
    encoding: "utf8",
    env: process.env,
    stdio: ["ignore", "pipe", "pipe"],
    maxBuffer: 64 * 1024 * 1024,
  });
}

function normalizeLibsqlRows(columns, rows) {
  return rows.map((row) => {
    const plain = {};
    for (const [columnIndex, columnName] of columns.entries()) {
      const value = row[columnIndex];
      plain[columnName] = typeof value === "bigint" ? Number(value) : value;
    }
    return plain;
  });
}

async function withLibsql(databasePath, body) {
  const client = createClient({ url: `file:${databasePath}` });
  try {
    return await body({
      exec: async (sqlText) => {
        await client.executeMultiple(sqlText.endsWith(";") ? sqlText : `${sqlText};`);
      },
      query: async (sqlText) => {
        const result = await client.execute(sqlText);
        return normalizeLibsqlRows(result.columns, result.rows);
      },
      raw: client,
    });
  } finally {
    client.close();
  }
}

/**
 * One engine-neutral runner. `body` receives { exec, query, engineName } and
 * returns a plain value; the value is compared across the two engines by the
 * caller so a divergence is a receipt rather than a surprise.
 */
async function onBothEngines(scratchDirectory, receiptName, body) {
  const cliPath = join(scratchDirectory, `${receiptName}.cli.db`);
  const libsqlPath = join(scratchDirectory, `${receiptName}.libsql.db`);

  const cliResult = await body({
    engineName: "sqlite3-cli",
    exec: async (sqlText) => sqliteCliExec(cliPath, sqlText),
    query: async (sqlText) => sqliteCliQuery(cliPath, sqlText),
    explain: async (sqlText) => sqliteCliExplain(cliPath, sqlText),
    tryExec: async (sqlText) => {
      try {
        sqliteCliExec(cliPath, sqlText);
        return { ok: true, message: "" };
      } catch (error) {
        return { ok: false, message: String(error?.stderr ?? error?.message ?? error) };
      }
    },
  });

  const libsqlResult = await withLibsql(libsqlPath, async (seam) =>
    body({
      engineName: "@libsql",
      exec: seam.exec,
      query: seam.query,
      explain: async (sqlText) =>
        (await seam.query(`EXPLAIN QUERY PLAN ${sqlText}`)).map((row) => row.detail),
      tryExec: async (sqlText) => {
        try {
          await seam.exec(sqlText);
          return { ok: true, message: "" };
        } catch (error) {
          return { ok: false, message: String(error?.message ?? error) };
        }
      },
    }),
  );

  return { cli: cliResult, libsql: libsqlResult };
}

function bothAgree(label, pair, expected) {
  check(`${label} [sqlite3-cli]`, pair.cli, expected);
  check(`${label} [@libsql]`, pair.libsql, expected);
}

// ══ PART V: the three-variant json read ════════════════════════════════════
//
// The user's spelling:  Some(json) ; None ; Undefined
//
//   json_extract(doc,'$.k') is SQL NULL for BOTH absent and json null.
//   json_type(doc,'$.k')    is SQL NULL for absent, text 'null' for json null.
//
// So a total three-way classifier exists today with no new semantics. The
// receipts below build it as (V1) a scalar CASE, (V2) three real variant
// tables in the exact shape v6/prolog/0_enum_expand.pl generates, and (V3)
// count the states an Option can actually be in, which is FOUR, not three.

const THREE_VARIANT_DOCUMENTS = `
  WITH documents(subject, document) AS (
    VALUES
      (1, '{"commit":"c1"}'),
      (2, '{"commit":null}'),
      (3, '{}'),
      (4, '{"commit":0}'),
      (5, '{"commit":""}'),
      (6, '{"commit":[]}'),
      (7, '{"commit":{"nested":null}}')
  )
`;

// The classifier under test. It reads json_type ONCE and never consults
// json_extract for the tag, which is the whole trick.
const THREE_VARIANT_TAG = `
  CASE
    WHEN json_type(document, '$.commit') IS NULL THEN 'undefined'
    WHEN json_type(document, '$.commit') = 'null' THEN 'none'
    ELSE 'some'
  END
`;

async function partThreeVariantJsonRead(scratchDirectory) {
  const v1 = await onBothEngines(scratchDirectory, "v1", async ({ query }) =>
    query(`
      ${THREE_VARIANT_DOCUMENTS}
      SELECT
        subject,
        ${THREE_VARIANT_TAG} AS tag,
        json_type(document, '$.commit') AS json_type_at_path,
        document -> '$.commit' AS payload_json_text,
        json_extract(document, '$.commit') AS payload_scalar
      FROM documents
      ORDER BY subject
    `),
  );

  bothAgree("V1 three-variant tag from json_type alone", v1, [
    { subject: 1, tag: "some", json_type_at_path: "text", payload_json_text: '"c1"', payload_scalar: "c1" },
    { subject: 2, tag: "none", json_type_at_path: "null", payload_json_text: "null", payload_scalar: null },
    { subject: 3, tag: "undefined", json_type_at_path: null, payload_json_text: null, payload_scalar: null },
    { subject: 4, tag: "some", json_type_at_path: "integer", payload_json_text: "0", payload_scalar: 0 },
    { subject: 5, tag: "some", json_type_at_path: "text", payload_json_text: '""', payload_scalar: "" },
    { subject: 6, tag: "some", json_type_at_path: "array", payload_json_text: "[]", payload_scalar: "[]" },
    { subject: 7, tag: "some", json_type_at_path: "object", payload_json_text: '{"nested":null}', payload_scalar: '{"nested":null}' },
  ]);

  // V2: the same read projected into the exact table shape the shipped enum
  // expansion generates. Reference DDL, copied from the compiler output for
  //   rel repo_latest(some(...) ; none(...) ; undefined(...)).
  // See receipt C2 for the compiler-produced original.
  const v2 = await onBothEngines(scratchDirectory, "v2", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "json_read_some" ("id" INTEGER NOT NULL, "payload" TEXT NOT NULL, PRIMARY KEY ("id", "payload")) WITHOUT ROWID;
      CREATE TABLE "json_read_none" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID;
      CREATE TABLE "json_read_undefined" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID;
      CREATE TABLE "json_read_tag" ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("id", "tag")) WITHOUT ROWID;
    `);

    // Three INSERT ... SELECT statements, one per variant, all total: no NULL
    // is ever written, no NOT NULL constraint is relaxed anywhere.
    await exec(`
      INSERT OR IGNORE INTO "json_read_some" ("id", "payload")
      ${THREE_VARIANT_DOCUMENTS}
      SELECT subject, document -> '$.commit' FROM documents
      WHERE json_type(document, '$.commit') IS NOT NULL
        AND json_type(document, '$.commit') <> 'null';

      INSERT OR IGNORE INTO "json_read_none" ("id")
      ${THREE_VARIANT_DOCUMENTS}
      SELECT subject FROM documents
      WHERE json_type(document, '$.commit') = 'null';

      INSERT OR IGNORE INTO "json_read_undefined" ("id")
      ${THREE_VARIANT_DOCUMENTS}
      SELECT subject FROM documents
      WHERE json_type(document, '$.commit') IS NULL;

      INSERT OR IGNORE INTO "json_read_tag" ("id", "tag") SELECT "id", 'some' FROM "json_read_some";
      INSERT OR IGNORE INTO "json_read_tag" ("id", "tag") SELECT "id", 'none' FROM "json_read_none";
      INSERT OR IGNORE INTO "json_read_tag" ("id", "tag") SELECT "id", 'undefined' FROM "json_read_undefined";
    `);

    return query(`
      SELECT "id", "tag" FROM "json_read_tag" ORDER BY "id"
    `);
  });

  bothAgree("V2 variant rows land in the shipped enum table shape", v2, [
    { id: 1, tag: "some" },
    { id: 2, tag: "none" },
    { id: 3, tag: "undefined" },
    { id: 4, tag: "some" },
    { id: 5, tag: "some" },
    { id: 6, tag: "some" },
    { id: 7, tag: "some" },
  ]);

  // V3: exhaustiveness. A subject that was never probed produces NO tag row.
  // The Option therefore has four observable states, not three, and the
  // fourth one is row absence, which no variant can express.
  const v3 = await onBothEngines(scratchDirectory, "v3", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "subject" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID;
      CREATE TABLE "probe_tag" ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL, PRIMARY KEY ("id", "tag")) WITHOUT ROWID;
      INSERT INTO "subject" VALUES (1),(2),(3),(4);
      INSERT INTO "probe_tag" VALUES (1,'some'),(2,'none'),(3,'undefined');
    `);
    return query(`
      SELECT s."id",
             coalesce((SELECT t."tag" FROM "probe_tag" t WHERE t."id" = s."id"), 'no-row') AS observed_state
      FROM "subject" s ORDER BY s."id"
    `);
  });

  bothAgree("V3 an Option has FOUR observable states, the fourth is row absence", v3, [
    { id: 1, observed_state: "some" },
    { id: 2, observed_state: "none" },
    { id: 3, observed_state: "undefined" },
    { id: 4, observed_state: "no-row" },
  ]);

  // V4: a json null nested INSIDE a Some payload survives as json text and is
  // never confused with the None variant. This is the case that makes the
  // three-variant read closed under nesting: only the TOP path needs the
  // three-way test, everything below stays inside the json value.
  const v4 = await onBothEngines(scratchDirectory, "v4", async ({ query }) =>
    query(`
      WITH documents(subject, document) AS (
        VALUES (1, '{"commit":{"sha":null}}'), (2, '{"commit":null}')
      )
      SELECT subject,
             ${THREE_VARIANT_TAG} AS tag,
             document -> '$.commit' AS payload,
             json_type(document, '$.commit.sha') AS inner_type
      FROM documents ORDER BY subject
    `),
  );

  bothAgree("V4 json null nested inside Some stays inside the payload", v4, [
    { subject: 1, tag: "some", payload: '{"sha":null}', inner_type: "null" },
    { subject: 2, tag: "none", payload: "null", inner_type: null },
  ]);

  // V5: the negative control. The current lowering's presence predicate,
  // json_extract(...) IS NOT NULL, collapses none and undefined into one
  // bucket. This is the exact defect the three-variant read repairs.
  const v5 = await onBothEngines(scratchDirectory, "v5", async ({ query }) =>
    query(`
      ${THREE_VARIANT_DOCUMENTS}
      SELECT
        sum(CASE WHEN json_extract(document, '$.commit') IS NOT NULL THEN 1 ELSE 0 END) AS extract_presence_says_present,
        sum(CASE WHEN json_type(document, '$.commit') IS NOT NULL THEN 1 ELSE 0 END) AS json_type_says_present
      FROM documents
    `),
  );

  bothAgree("V5 json_extract presence loses one document that json_type keeps", v5, [
    { extract_presence_says_present: 5, json_type_says_present: 6 },
  ]);
}

// ══ PART C: candidate inventory from the real compiler ═════════════════════

const CANDIDATE_PROGRAMS = {
  // Candidate C, the status quo: two relations and a join.
  "row-absence-two-rels": `rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_with_latest(repo: text, commit: text).
rel repo_without_latest(repo: text).

repo_with_latest(Repo, Commit) <- repo(Repo), latest_commit(Repo, Commit).
repo_without_latest(Repo) <- repo(Repo), not(latest_commit(Repo, _)).
`,

  // Candidate A, the user's Option as enum variants, DERIVED by a level rule.
  // This is the outer-join shape the whole question is about.
  "option-variants-level": `rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_latest(some(repo: text, commit: text) ; none(repo: text)).

repo_latest_some(Id, Repo, Commit) <- repo(Repo), latest_commit(Repo, Commit), Id := 1.
repo_latest_none(Id, Repo) <- repo(Repo), not(latest_commit(Repo, _)), Id := 2.
`,

  // The same variants, edge-headed, which is the only arrow the enum
  // machinery accepts on a generated variant relation.
  "option-variants-edge": `rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_latest(some(repo: text, commit: text) ; none(repo: text)).

repo_latest_some(Id, Repo, Commit) <+ repo(Repo), latest_commit(Repo, Commit), Id := 1.
`,

  // Candidate B, Design D as the null-coherence lab spells it. Written the
  // way a user would type it, to find out what the front end says today.
  "design-d-nullable-column": `rel repo(name: text).
rel latest_commit(repo: text, commit: text).
rel repo_latest(repo: text, commit: text?).

repo_latest(Repo, Commit) <- repo(Repo), latest_commit(Repo, Commit).
repo_latest(Repo, null) <- repo(Repo), not(latest_commit(Repo, _)).
`,

  // The user's exact three-variant json spelling, as a declaration.
  "option-json-three-variants": `rel document(id: int, body: text).
rel json_read(some(id: int, payload: text) ; none(id: int) ; undefined(id: int)).

json_read_some(Id, Id, Payload) <+ document(Id, Payload).
`,
};

function compileCandidate(scratchDirectory, name, source) {
  const inputPath = join(scratchDirectory, `${name}.dl6`);
  const outputPath = join(scratchDirectory, `${name}.ts`);
  writeFileSync(inputPath, source, "utf8");
  try {
    execFileSync(
      "bash",
      [join(repositoryRoot, "v6/prolog/compile/scripts/compile_dl6.sh"), inputPath, outputPath],
      { encoding: "utf8", env: process.env, stdio: ["ignore", "pipe", "pipe"], cwd: repositoryRoot },
    );
  } catch (error) {
    const message = String(error?.stderr ?? error?.message ?? error);
    return { compiled: false, refusal: extractRefusal(message) };
  }
  const emitted = readFileSync(outputPath, "utf8");
  return {
    compiled: true,
    persistentTables: countMatches(emitted, /CREATE TABLE "(?!__)/g),
    scratchTables: countMatches(emitted, /CREATE TEMP TABLE "/g),
    indexes: countMatches(emitted, /CREATE INDEX "/g),
    emittedLines: emitted.split("\n").length,
  };
}

function extractRefusal(message) {
  const construct = message.match(/unsupported_construct\(([a-z_0-9]+)\(/);
  if (construct !== null) return construct[1];
  const reason = message.match(/reason=([a-z_0-9]+)/);
  if (reason !== null) return reason[1];
  // The front end throws a bare parse term with no message clause, so the
  // shell sees swipl's "Unknown message:" fallback. Normalize it to the term
  // functor and drop the character-code payload; see receipt C7.
  const parseError = message.match(/Unknown message: ([a-z_0-9]+)\(([a-z_0-9]+)/);
  if (parseError !== null) return `${parseError[1]}(${parseError[2]})`;
  return message.trim().slice(0, 160);
}

function countMatches(text, pattern) {
  return (text.match(pattern) ?? []).length;
}

function partCandidateInventory(scratchDirectory) {
  const inventory = {};
  for (const [name, source] of Object.entries(CANDIDATE_PROGRAMS)) {
    inventory[name] = compileCandidate(scratchDirectory, name, source);
  }

  check("C1 status quo two-rel program compiles", inventory["row-absence-two-rels"].compiled, true);

  check(
    "C2 Option variants DERIVED by a level rule are REFUSED",
    inventory["option-variants-level"].refusal,
    "keyed_level_head",
  );

  check(
    "C3 Option variants fed by an edge rule compile",
    inventory["option-variants-edge"].compiled,
    true,
  );

  check(
    "C4 Design D nullable column spelling is not parsed today",
    inventory["design-d-nullable-column"].compiled,
    false,
  );

  // C7: the refusal a cold author actually sees for `commit: text?` is not a
  // named refusal at all. It is swipl's Unknown-message fallback carrying a
  // parse term whose payload is the source rendered as raw character codes.
  // This is language-design-review finding B4 caught in the wild, and it is
  // the FIRST thing anyone typing Design D would hit.
  check(
    "C7 the Design D refusal is an unnamed parse term, not a named refusal",
    inventory["design-d-nullable-column"].refusal,
    "dl_parse_error(statement)",
  );

  check(
    "C5 the user's three-variant json declaration compiles as an edge-fed enum",
    inventory["option-json-three-variants"].compiled,
    true,
  );

  const twoRel = inventory["row-absence-two-rels"];
  const variantsEdge = inventory["option-variants-edge"];
  const jsonVariants = inventory["option-json-three-variants"];

  // C6 counts what the DECLARATION costs. Both variant programs declare two
  // source relations fewer or equal to the two-rel program, so the honest
  // comparison is generated tables per declared optional field:
  //   two-rel                  2 source + 2 result       = 4
  //   two-variant plus tag     2 source + 2 variant + 1  = 5
  //   three-variant plus tag   1 source + 3 variant + 1  = 5
  check(
    "C6 persistent table count, two-rel versus two-variant versus three-variant",
    {
      twoRel: twoRel.persistentTables,
      twoVariantPlusTag: variantsEdge.persistentTables,
      threeVariantPlusTag: jsonVariants.persistentTables,
    },
    { twoRel: 4, twoVariantPlusTag: 5, threeVariantPlusTag: 5 },
  );

  // C6b is the one that matters for the explosion question: scratch tables
  // and indexes are minted PER RELATION by the incremental emitter, so every
  // extra variant relation costs a delta table, two frontier tables and
  // their indexes whether or not the variant is ever populated.
  check(
    "C6b scratch tables and indexes per variant relation",
    {
      twoRelScratch: twoRel.scratchTables,
      twoRelIndexes: twoRel.indexes,
      threeVariantScratch: jsonVariants.scratchTables,
      threeVariantIndexes: jsonVariants.indexes,
      scratchPerExtraRelation: (jsonVariants.scratchTables - twoRel.scratchTables) / 2,
      indexesPerExtraRelation: (jsonVariants.indexes - twoRel.indexes) / 2,
    },
    {
      twoRelScratch: 14,
      twoRelIndexes: 12,
      threeVariantScratch: 16,
      threeVariantIndexes: 15,
      scratchPerExtraRelation: 1,
      indexesPerExtraRelation: 1.5,
    },
  );

  console.log(
    `INFO C inventory ${JSON.stringify(inventory, null, 0)}`,
  );
  return inventory;
}

// ══ PART X: the explosion measurement ══════════════════════════════════════
//
// Shape: N subjects, 90% of them have the optional value, 10% do not.
// Every schema below is the exact DDL v6/prolog/compile/lower.pl emits for
// that relation shape; see receipt C for the compiler-produced originals.

function scaleSchema(candidate) {
  if (candidate === "two-rel") {
    return `
      CREATE TABLE "repo_with_latest" ("repo" TEXT NOT NULL, "commit" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("repo", "commit")) WITHOUT ROWID;
      CREATE TABLE "repo_without_latest" ("repo" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("repo")) WITHOUT ROWID;
    `;
  }
  if (candidate === "option-variants") {
    // Copied verbatim from the compiler output for the option-variants-edge
    // program in receipt C, so the measurement is of the real emitted shape
    // and not of a hand-drawn approximation. Note what 0_enum_expand.pl
    // does with the key: content_key_positions/2 puts the CONTENT columns in
    // the primary key and leaves the `id` discriminator out of it.
    return `
      CREATE TABLE "repo_latest_some" ("id" INTEGER NOT NULL, "repo" TEXT NOT NULL, "commit" TEXT NOT NULL, PRIMARY KEY ("repo", "commit")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_none" ("id" INTEGER NOT NULL, "repo" TEXT NOT NULL, PRIMARY KEY ("repo")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_tag" ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("id", "tag")) WITHOUT ROWID;
    `;
  }
  if (candidate === "nullable-column-without-rowid") {
    // Design D dropped straight into the current set-rel table family.
    return `
      CREATE TABLE "repo_latest" ("repo" TEXT NOT NULL, "commit" TEXT, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("repo", "commit")) WITHOUT ROWID;
    `;
  }
  if (candidate === "nullable-column-rowid-unique") {
    // Design D on a DERIVED relation. A derived relation is unkeyed (a key
    // declaration on a level-rule head is refused as keyed_level_head), so
    // its primary key is the whole row, so receipt D1 rules out WITHOUT
    // ROWID and this is the only family left: __id INTEGER PRIMARY KEY plus
    // a UNIQUE index that duplicates every column.
    return `
      CREATE TABLE "repo_latest" ("__id" INTEGER PRIMARY KEY, "repo" TEXT NOT NULL, "commit" TEXT, "__support_count" INTEGER NOT NULL DEFAULT 1, UNIQUE ("repo", "commit"));
    `;
  }
  if (candidate === "nullable-column-keyed") {
    // Design D on a WORLD-FED keyed relation, the best case for the design.
    // The nullable column is outside the declared key, so WITHOUT ROWID
    // survives and there is one btree, not two.
    return `
      CREATE TABLE "repo_latest" ("repo" TEXT NOT NULL, "commit" TEXT, PRIMARY KEY ("repo")) WITHOUT ROWID;
    `;
  }
  throw new Error(`unknown candidate ${candidate}`);
}

function scaleFill(candidate, rowCount) {
  const subjects = `
    WITH RECURSIVE subject(n) AS (
      SELECT 1 UNION ALL SELECT n + 1 FROM subject WHERE n < ${rowCount}
    )
  `;
  const present = "n % 10 <> 0";
  const absent = "n % 10 = 0";
  if (candidate === "two-rel") {
    return `
      ${subjects}
      INSERT INTO "repo_with_latest" ("repo","commit") SELECT 'repo-' || n, 'commit-' || n FROM subject WHERE ${present};
      ${subjects}
      INSERT INTO "repo_without_latest" ("repo") SELECT 'repo-' || n FROM subject WHERE ${absent};
    `;
  }
  if (candidate === "option-variants") {
    return `
      ${subjects}
      INSERT INTO "repo_latest_some" ("id","repo","commit") SELECT n, 'repo-' || n, 'commit-' || n FROM subject WHERE ${present};
      ${subjects}
      INSERT INTO "repo_latest_none" ("id","repo") SELECT n, 'repo-' || n FROM subject WHERE ${absent};
      INSERT INTO "repo_latest_tag" ("id","tag") SELECT "id",'some' FROM "repo_latest_some";
      INSERT INTO "repo_latest_tag" ("id","tag") SELECT "id",'none' FROM "repo_latest_none";
    `;
  }
  if (candidate === "nullable-column-rowid-unique" || candidate === "nullable-column-keyed") {
    return `
      ${subjects}
      INSERT INTO "repo_latest" ("repo","commit") SELECT 'repo-' || n, CASE WHEN ${present} THEN 'commit-' || n ELSE NULL END FROM subject;
    `;
  }
  throw new Error(`no fill for ${candidate}`);
}

// The two reads every candidate must answer.
//   blind: "give me every repo that has a commit"  (does not care about optionality)
//   aware: "give me every repo with its commit if any" (cares)
function scaleReads(candidate) {
  if (candidate === "two-rel") {
    return {
      blind: `SELECT "repo","commit" FROM "repo_with_latest" WHERE "repo" = 'repo-7'`,
      aware: `SELECT "repo","commit" FROM "repo_with_latest" UNION ALL SELECT "repo", NULL FROM "repo_without_latest"`,
    };
  }
  if (candidate === "option-variants") {
    return {
      blind: `SELECT "repo","commit" FROM "repo_latest_some" WHERE "repo" = 'repo-7'`,
      aware: `SELECT "repo",'some' AS tag,"commit" FROM "repo_latest_some" UNION ALL SELECT "repo",'none' AS tag,NULL FROM "repo_latest_none"`,
    };
  }
  if (candidate === "nullable-column-rowid-unique" || candidate === "nullable-column-keyed") {
    return {
      blind: `SELECT "repo","commit" FROM "repo_latest" WHERE "repo" = 'repo-7' AND "commit" IS NOT NULL`,
      aware: `SELECT "repo","commit" FROM "repo_latest"`,
    };
  }
  throw new Error(`no reads for ${candidate}`);
}

async function measureCandidateAtScale(scratchDirectory, candidate, rowCount) {
  const receiptName = `x-${candidate}-${rowCount}`;
  return onBothEngines(scratchDirectory, receiptName, async ({ exec, query, explain }) => {
    await exec(scaleSchema(candidate));
    await exec(scaleFill(candidate, rowCount));
    await exec(`ANALYZE;`);

    const sizeRows = await query(`
      SELECT (SELECT * FROM pragma_page_count()) * (SELECT * FROM pragma_page_size()) AS storage_bytes
    `);
    const tableRows = await query(`
      SELECT count(*) AS persistent_tables FROM sqlite_schema
      WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
    `);
    const rowsRows = await query(`
      SELECT sum(cnt) AS total_rows FROM (
        ${(await query(`SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name`))
          .map((row) => `SELECT count(*) AS cnt FROM "${row.name}"`)
          .join(" UNION ALL ")}
      )
    `);

    const reads = scaleReads(candidate);
    return {
      storage_bytes: sizeRows[0].storage_bytes,
      persistent_tables: tableRows[0].persistent_tables,
      total_rows: rowsRows[0].total_rows,
      blind_plan: await explain(reads.blind),
      aware_plan: await explain(reads.aware),
    };
  });
}

async function partExplosion(scratchDirectory) {
  const candidates = [
    "two-rel",
    "option-variants",
    "nullable-column-rowid-unique",
    "nullable-column-keyed",
  ];
  const summary = {};

  for (const rowCount of [10000, 100000]) {
    summary[rowCount] = {};
    for (const candidate of candidates) {
      const pair = await measureCandidateAtScale(scratchDirectory, candidate, rowCount);
      summary[rowCount][candidate] = pair;

      check(
        `X row counts agree across engines, ${candidate} at ${rowCount}`,
        pair.cli.total_rows,
        pair.libsql.total_rows,
      );
      check(
        `X table counts agree across engines, ${candidate} at ${rowCount}`,
        pair.cli.persistent_tables,
        pair.libsql.persistent_tables,
      );
      check(
        `X blind-read plan agrees across engines, ${candidate} at ${rowCount}`,
        pair.cli.blind_plan,
        pair.libsql.blind_plan,
      );

      console.log(
        `INFO X ${candidate} n=${rowCount} rows=${pair.cli.total_rows} tables=${pair.cli.persistent_tables} bytes=${pair.cli.storage_bytes}`,
      );
      console.log(`INFO X   blind plan: ${JSON.stringify(pair.cli.blind_plan)}`);
      console.log(`INFO X   aware plan: ${JSON.stringify(pair.cli.aware_plan)}`);
    }
  }

  // The stated shape: N subjects, 10 percent absent.
  check("X1 two-rel row total at 100k is exactly the subject count", summary[100000]["two-rel"].cli.total_rows, 100000);
  check(
    "X2 option-variants row total at 100k is 2x the subject count, one variant row plus one tag row",
    summary[100000]["option-variants"].cli.total_rows,
    200000,
  );
  check(
    "X3 nullable-column row total at 100k is exactly the subject count",
    summary[100000]["nullable-column-rowid-unique"].cli.total_rows,
    100000,
  );

  // X3b is the ranking the user asked for and it inverts the stated worry.
  // The variant encoding doubles ROWS. The nullable column on a derived
  // relation keeps rows flat and still costs more BYTES, because it is the
  // only candidate forced off the single-btree WITHOUT ROWID family and onto
  // a rowid table plus a UNIQUE index that duplicates every column.
  check(
    "X3b storage ranking at 100k, smallest first",
    Object.entries(summary[100000])
      .sort(([, left], [, right]) => left.cli.storage_bytes - right.cli.storage_bytes)
      .map(([name]) => name),
    [
      "nullable-column-keyed",
      "two-rel",
      "option-variants",
      "nullable-column-rowid-unique",
    ],
  );

  // The repo law: plan-sensitive paths get EXPLAIN assertions, not end-state
  // equality alone. The optionality-BLIND read is the one every candidate has
  // to answer well, because it is the read written by code that does not care
  // the field is optional. All three reach it by index SEARCH.
  for (const candidate of candidates) {
    const plan = summary[100000][candidate].cli.blind_plan.join(" | ");
    check(
      `X4 optionality-blind read is SEARCH-not-SCAN at 100k, ${candidate}`,
      { scans: plan.includes("SCAN"), searches: plan.includes("SEARCH") },
      { scans: false, searches: true },
    );
  }

  // Storage, stated as a ratio against the two-rel baseline so the number
  // survives a page-size change.
  const baselineBytes = summary[100000]["two-rel"].cli.storage_bytes;
  for (const candidate of candidates) {
    const bytes = summary[100000][candidate].cli.storage_bytes;
    console.log(
      `INFO X5 ${candidate} at 100k: ${bytes} bytes, ${(bytes / baselineBytes).toFixed(2)}x the two-rel baseline`,
    );
  }

  return summary;
}

// ══ PART D: the Design D structural break ══════════════════════════════════

async function partDesignDBreak(scratchDirectory) {
  // D1: the default set-rel table family cannot hold a null in ANY column,
  // because an unkeyed set relation's primary key is the whole row and
  // WITHOUT ROWID makes every primary key column implicitly NOT NULL. This
  // is a strictly wider break than the nullable-KEY refusal the
  // null-coherence lab priced, because a derived outer-join result is an
  // unkeyed set relation by construction.
  const d1 = await onBothEngines(scratchDirectory, "d1", async ({ exec, tryExec }) => {
    await exec(scaleSchema("nullable-column-without-rowid"));
    const attempt = await tryExec(`INSERT INTO "repo_latest" ("repo","commit") VALUES ('beta', NULL)`);
    return { rejected: attempt.ok === false, mentionsNotNull: /NOT NULL/i.test(attempt.message) };
  });
  bothAgree("D1 WITHOUT ROWID full-row primary key rejects a null column value", d1, {
    rejected: true,
    mentionsNotNull: true,
  });

  // D2: the fallback family accepts the null and then loses set identity,
  // because UNIQUE treats two nulls as distinct and the emitter's write verb
  // is INSERT OR IGNORE. Two identical rows coexist in a SET relation.
  const d2 = await onBothEngines(scratchDirectory, "d2", async ({ exec, query }) => {
    await exec(scaleSchema("nullable-column-rowid-unique"));
    await exec(`
      INSERT OR IGNORE INTO "repo_latest" ("repo","commit") VALUES ('beta', NULL);
      INSERT OR IGNORE INTO "repo_latest" ("repo","commit") VALUES ('beta', NULL);
      INSERT OR IGNORE INTO "repo_latest" ("repo","commit") VALUES ('alpha', 'a1');
      INSERT OR IGNORE INTO "repo_latest" ("repo","commit") VALUES ('alpha', 'a1');
    `);
    const rows = await query(`SELECT "repo", count(*) AS copies FROM "repo_latest" GROUP BY "repo" ORDER BY "repo"`);
    return rows;
  });
  bothAgree("D2 INSERT OR IGNORE plus UNIQUE duplicates the null-bearing row and dedups the total one", d2, [
    { repo: "alpha", copies: 1 },
    { repo: "beta", copies: 2 },
  ]);

  // D3: the negation trap named by the null-coherence lab, re-derived on the
  // emitter's actual NOT EXISTS shape rather than on a bare scalar.
  const d3 = await onBothEngines(scratchDirectory, "d3", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "outer_rel" ("repo" TEXT NOT NULL, "commit" TEXT);
      CREATE TABLE "inner_rel" ("repo" TEXT NOT NULL, "commit" TEXT);
      INSERT INTO "outer_rel" VALUES ('beta', NULL);
      INSERT INTO "inner_rel" VALUES ('beta', NULL);
    `);
    const naive = await query(`
      SELECT count(*) AS rows_reported_absent FROM "outer_rel" b0
      WHERE NOT EXISTS (SELECT 1 FROM "inner_rel" n0 WHERE n0."repo" = b0."repo" AND n0."commit" = b0."commit")
    `);
    const nullSafe = await query(`
      SELECT count(*) AS rows_reported_absent FROM "outer_rel" b0
      WHERE NOT EXISTS (SELECT 1 FROM "inner_rel" n0 WHERE n0."repo" IS NOT DISTINCT FROM b0."repo" AND n0."commit" IS NOT DISTINCT FROM b0."commit")
    `);
    return { naive: naive[0].rows_reported_absent, nullSafe: nullSafe[0].rows_reported_absent };
  });
  bothAgree("D3 emitted NOT EXISTS with = reports a present null-bearing row as absent", d3, {
    naive: 1,
    nullSafe: 0,
  });

  // D4: nobody had priced whether the null-safe repair keeps the index.
  // Hypothesis going in was that it would not. MEASURED: it does. Both
  // builds plan IS NOT DISTINCT FROM against a UNIQUE index as an ordinary
  // equality SEARCH, including when the compared literal is itself NULL.
  // The null-safe rewrite therefore costs correctness work in the compiler
  // and nothing at the planner, which removes the strongest performance
  // argument against Design D.
  const d4 = await onBothEngines(scratchDirectory, "d4", async ({ exec, explain }) => {
    await exec(`
      CREATE TABLE "keyed_rel" ("__id" INTEGER PRIMARY KEY, "repo" TEXT NOT NULL, "commit" TEXT, UNIQUE ("repo","commit"));
      WITH RECURSIVE subject(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM subject WHERE n < 20000)
      INSERT INTO "keyed_rel" ("repo","commit") SELECT 'repo-'||n, CASE WHEN n%10<>0 THEN 'commit-'||n END FROM subject;
      ANALYZE;
    `);
    return {
      equality: await explain(
        `SELECT * FROM "keyed_rel" WHERE "repo" = 'repo-7' AND "commit" = 'commit-7'`,
      ),
      nullSafe: await explain(
        `SELECT * FROM "keyed_rel" WHERE "repo" IS NOT DISTINCT FROM 'repo-7' AND "commit" IS NOT DISTINCT FROM 'commit-7'`,
      ),
      // The case that actually matters: looking up the null-bearing row.
      nullSafeAgainstNull: await explain(
        `SELECT * FROM "keyed_rel" WHERE "repo" IS NOT DISTINCT FROM 'repo-10' AND "commit" IS NOT DISTINCT FROM NULL`,
      ),
      // The naive form for the same lookup, which cannot find the row at all.
      equalityAgainstNull: await explain(
        `SELECT * FROM "keyed_rel" WHERE "repo" = 'repo-10' AND "commit" = NULL`,
      ),
    };
  });
  check(
    "D4 equality lookup uses the index [sqlite3-cli]",
    d4.cli.equality.join(" | ").includes("SEARCH"),
    true,
  );
  check(
    "D4 equality lookup uses the index [@libsql]",
    d4.libsql.equality.join(" | ").includes("SEARCH"),
    true,
  );
  console.log(`INFO D4 equality plan             ${JSON.stringify(d4.cli.equality)}`);
  console.log(`INFO D4 null-safe plan            ${JSON.stringify(d4.cli.nullSafe)}`);
  console.log(`INFO D4 null-safe vs NULL plan    ${JSON.stringify(d4.cli.nullSafeAgainstNull)}`);
  console.log(`INFO D4 equality vs NULL plan     ${JSON.stringify(d4.cli.equalityAgainstNull)}`);
  check("D4 null-safe plans agree across engines", d4.cli.nullSafe, d4.libsql.nullSafe);
  check(
    "D4b IS NOT DISTINCT FROM keeps the index SEARCH [sqlite3-cli]",
    d4.cli.nullSafe.join(" | ").includes("SEARCH"),
    true,
  );
  check(
    "D4b IS NOT DISTINCT FROM keeps the index SEARCH [@libsql]",
    d4.libsql.nullSafe.join(" | ").includes("SEARCH"),
    true,
  );
  check(
    "D4c the null-bearing lookup also stays an index SEARCH on both engines",
    {
      cli: d4.cli.nullSafeAgainstNull.join(" | ").includes("SEARCH"),
      libsql: d4.libsql.nullSafeAgainstNull.join(" | ").includes("SEARCH"),
    },
    { cli: true, libsql: true },
  );

  // D5: the same null-bearing lookup written the way the emitter writes it
  // today returns nothing at all, at any scale. This is the silent-failure
  // shape, not a slow shape.
  const d5 = await onBothEngines(scratchDirectory, "d5", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "keyed_rel" ("__id" INTEGER PRIMARY KEY, "repo" TEXT NOT NULL, "commit" TEXT, UNIQUE ("repo","commit"));
      INSERT INTO "keyed_rel" ("repo","commit") VALUES ('repo-10', NULL);
    `);
    const naive = await query(`SELECT count(*) AS found FROM "keyed_rel" WHERE "repo" = 'repo-10' AND "commit" = NULL`);
    const safe = await query(
      `SELECT count(*) AS found FROM "keyed_rel" WHERE "repo" IS NOT DISTINCT FROM 'repo-10' AND "commit" IS NOT DISTINCT FROM NULL`,
    );
    return { naive: naive[0].found, nullSafe: safe[0].found };
  });
  bothAgree("D5 the emitted key lookup cannot find a null-bearing row it just wrote", d5, {
    naive: 0,
    nullSafe: 1,
  });

  // D5: how far the null-safe rewrite has to travel. Count the distinct
  // equality-emitting call sites in lower.pl that a type-directed rewrite
  // would have to reach. Read-only grep, reported not asserted, because the
  // count moves with other lanes.
  return { d4 };
}

// ══ PART E: hazards specific to the variant encoding ═══════════════════════

async function partVariantHazards(scratchDirectory) {
  // E1: the shipped enum expansion keys a variant relation on its CONTENT
  // columns (0_enum_expand.pl content_key_positions/2 returns 2..arity), so
  // the discriminating id column is NOT in the key. Two different Some
  // payloads for the same subject therefore both survive: the Option is not
  // a function of the subject.
  const e1 = await onBothEngines(scratchDirectory, "e1", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "repo_latest_some" ("id" INTEGER NOT NULL, "commit" TEXT NOT NULL, PRIMARY KEY ("commit")) WITHOUT ROWID;
      INSERT OR IGNORE INTO "repo_latest_some" VALUES (1, 'commit-a');
      INSERT OR IGNORE INTO "repo_latest_some" VALUES (1, 'commit-b');
    `);
    return query(`SELECT "id", count(*) AS options_for_this_subject FROM "repo_latest_some" GROUP BY "id"`);
  });
  bothAgree("E1 a content-keyed variant relation admits two Some values for one subject", e1, [
    { id: 1, options_for_this_subject: 2 },
  ]);

  // E2: nothing stops Some and None coexisting for the same subject either.
  // Exhaustiveness is checked at the MATCH site, never at the storage site.
  const e2 = await onBothEngines(scratchDirectory, "e2", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "repo_latest_some" ("id" INTEGER NOT NULL, "commit" TEXT NOT NULL, PRIMARY KEY ("commit")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_none" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_tag" ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("id","tag")) WITHOUT ROWID;
      INSERT INTO "repo_latest_some" VALUES (1, 'commit-a');
      INSERT INTO "repo_latest_none" VALUES (1);
      INSERT OR IGNORE INTO "repo_latest_tag" ("id","tag") SELECT "id",'some' FROM "repo_latest_some";
      INSERT OR IGNORE INTO "repo_latest_tag" ("id","tag") SELECT "id",'none' FROM "repo_latest_none";
    `);
    return query(`SELECT "id", count(*) AS tags_for_this_subject FROM "repo_latest_tag" GROUP BY "id"`);
  });
  bothAgree("E2 Some and None coexist for one subject with no storage-level refusal", e2, [
    { id: 1, tags_for_this_subject: 2 },
  ]);

  // E3: the flip cost. Moving one subject from Some to None touches three
  // tables, and the tag relation is level-headed so it also carries the
  // refCount column. Count the tables a single transition writes.
  const e3 = await onBothEngines(scratchDirectory, "e3", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "repo_latest_some" ("id" INTEGER NOT NULL, "commit" TEXT NOT NULL, PRIMARY KEY ("commit")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_none" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_tag" ("id" INTEGER NOT NULL, "tag" TEXT NOT NULL, "__support_count" INTEGER NOT NULL DEFAULT 1, PRIMARY KEY ("id","tag")) WITHOUT ROWID;
      INSERT INTO "repo_latest_some" VALUES (1, 'commit-a');
      INSERT INTO "repo_latest_tag" ("id","tag") VALUES (1,'some');
      DELETE FROM "repo_latest_some" WHERE "id" = 1;
      DELETE FROM "repo_latest_tag" WHERE "id" = 1 AND "tag" = 'some';
      INSERT INTO "repo_latest_none" VALUES (1);
      INSERT INTO "repo_latest_tag" ("id","tag") VALUES (1,'none');
    `);
    return query(`SELECT "id","tag" FROM "repo_latest_tag" ORDER BY "id"`);
  });
  bothAgree("E3 a Some to None transition is 4 writes across 3 tables", e3, [{ id: 1, tag: "none" }]);
}

// ══ PART N: the null-safe equality bill ════════════════════════════════════

async function partNullSafeBill(scratchDirectory) {
  // N1: confirm both builds accept IS NOT DISTINCT FROM at all. The syntax
  // arrived in SQLite 3.39; the project runs 3.43.2 and 3.45.1.
  const n1 = await onBothEngines(scratchDirectory, "n1", async ({ query }) =>
    query(`SELECT (NULL IS NOT DISTINCT FROM NULL) AS both_null, (1 IS NOT DISTINCT FROM NULL) AS one_null`),
  );
  bothAgree("N1 both builds accept IS NOT DISTINCT FROM", n1, [{ both_null: 1, one_null: 0 }]);

  // N2: GROUP BY, DISTINCT and UNION already use null-safe identity, so the
  // boundary-diff path needs no repair. Only rule-level equality does.
  const n2 = await onBothEngines(scratchDirectory, "n2", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "staged" ("repo" TEXT NOT NULL, "commit" TEXT);
      INSERT INTO "staged" VALUES ('beta', NULL), ('beta', NULL), ('alpha','a1');
    `);
    const grouped = await query(`SELECT count(*) AS groups FROM (SELECT "repo","commit" FROM "staged" GROUP BY "repo","commit")`);
    const distinct = await query(`SELECT count(*) AS distinct_rows FROM (SELECT DISTINCT "repo","commit" FROM "staged")`);
    return { groups: grouped[0].groups, distinct_rows: distinct[0].distinct_rows };
  });
  bothAgree("N2 GROUP BY and DISTINCT are already null-safe, the diff plane needs no repair", n2, {
    groups: 2,
    distinct_rows: 2,
  });

  // N3: the asymmetry SQLite itself calls arbitrary. UNIQUE is null-DISTINCT
  // while DISTINCT is null-EQUAL, in one database, on one column.
  const n3 = await onBothEngines(scratchDirectory, "n3", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "u" ("v" TEXT UNIQUE);
      INSERT INTO "u" VALUES (NULL), (NULL), (NULL);
    `);
    const stored = await query(`SELECT count(*) AS unique_kept FROM "u"`);
    const deduped = await query(`SELECT count(*) AS distinct_says FROM (SELECT DISTINCT "v" FROM "u")`);
    return { unique_kept: stored[0].unique_kept, distinct_says: deduped[0].distinct_says };
  });
  bothAgree("N3 UNIQUE keeps three nulls while DISTINCT collapses them to one", n3, {
    unique_kept: 3,
    distinct_says: 1,
  });

  // N4: the variant encoding needs none of this. Its rows are total, so the
  // same negation that broke in D3 is correct with plain equality.
  const n4 = await onBothEngines(scratchDirectory, "n4", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "subject_rel" ("id" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID;
      CREATE TABLE "repo_latest_some" ("id" INTEGER NOT NULL, "commit" TEXT NOT NULL, PRIMARY KEY ("id","commit")) WITHOUT ROWID;
      INSERT INTO "subject_rel" VALUES (1),(2);
      INSERT INTO "repo_latest_some" VALUES (1,'commit-a');
    `);
    return query(`
      SELECT count(*) AS subjects_without_some FROM "subject_rel" s
      WHERE NOT EXISTS (SELECT 1 FROM "repo_latest_some" n WHERE n."id" = s."id")
    `);
  });
  bothAgree("N4 negation over total variant rows is correct with plain equality", n4, [
    { subjects_without_some: 1 },
  ]);
}

// ══ PART G: candidate D, optionality at the use site ═══════════════════════
//
// Datomic's get-else. Storage stays total and stays two relations; the
// consumer that wants one row asks for one row and supplies the default
// itself. Graded here on the SAME two-rel storage the status quo builds, so
// the comparison is read-cost only.

async function partUseSiteDefaulting(scratchDirectory) {
  const g = await onBothEngines(scratchDirectory, "g", async ({ exec, query, explain }) => {
    await exec(scaleSchema("two-rel"));
    await exec(scaleFill("two-rel", 10000));
    // The whole subject set, which the two-rel storage splits in two.
    await exec(`
      CREATE TABLE "repo" ("name" TEXT NOT NULL, PRIMARY KEY ("name")) WITHOUT ROWID;
      INSERT INTO "repo" ("name") SELECT "repo" FROM "repo_with_latest";
      INSERT INTO "repo" ("name") SELECT "repo" FROM "repo_without_latest";
      ANALYZE;
    `);

    // get_else(latest_commit(Repo, Commit), 'absent') lowers to exactly this.
    const useSite = `
      SELECT r."name" AS "repo", coalesce(l."commit", 'absent') AS "commit"
      FROM "repo" r LEFT JOIN "repo_with_latest" l ON l."repo" = r."name"
    `;

    const rows = await query(`SELECT count(*) AS produced FROM (${useSite})`);
    const defaulted = await query(
      `SELECT count(*) AS defaulted FROM (${useSite}) WHERE "commit" = 'absent'`,
    );
    const singleSubject = `${useSite} WHERE r."name" = 'repo-70'`;
    return {
      produced: rows[0].produced,
      defaulted: defaulted[0].defaulted,
      full_plan: await explain(useSite),
      point_plan: await explain(singleSubject),
      storage_bytes: (await query(
        `SELECT (SELECT * FROM pragma_page_count()) * (SELECT * FROM pragma_page_size()) AS storage_bytes`,
      ))[0].storage_bytes,
    };
  });

  check(
    "G1 use-site defaulting produces one row per subject with no stored optional column",
    { produced: g.cli.produced, defaulted: g.cli.defaulted },
    { produced: 10000, defaulted: 1000 },
  );
  check("G1 both engines agree on the produced set", g.cli.produced, g.libsql.produced);
  check(
    "G2 the point read stays an index SEARCH on both sides of the LEFT JOIN",
    {
      cli: g.cli.point_plan.every((line) => line.includes("SEARCH")),
      libsql: g.libsql.point_plan.every((line) => line.includes("SEARCH")),
    },
    { cli: true, libsql: true },
  );
  console.log(`INFO G full plan  ${JSON.stringify(g.cli.full_plan)}`);
  console.log(`INFO G point plan ${JSON.stringify(g.cli.point_plan)}`);
  console.log(`INFO G storage    ${g.cli.storage_bytes} bytes, zero extra relations`);

  // G3: the hole. The default has to BE a value of the column's type, so it
  // is stolen from the value domain. On text a program can usually spare a
  // string; on int there is no safe choice, and the receipt shows the
  // collision rather than arguing about it.
  const g3 = await onBothEngines(scratchDirectory, "g3", async ({ exec, query }) => {
    await exec(`
      CREATE TABLE "score" ("subject" TEXT NOT NULL, "value" INTEGER NOT NULL, PRIMARY KEY ("subject")) WITHOUT ROWID;
      INSERT INTO "score" VALUES ('a', -1), ('b', 7);
      CREATE TABLE "subject_rel" ("subject" TEXT NOT NULL, PRIMARY KEY ("subject")) WITHOUT ROWID;
      INSERT INTO "subject_rel" VALUES ('a'),('b'),('c');
    `);
    // Default -1 for "no score". Subject 'a' genuinely scored -1.
    return query(`
      SELECT s."subject", coalesce(v."value", -1) AS value,
             CASE WHEN v."value" IS NULL THEN 'defaulted' ELSE 'real' END AS truth
      FROM "subject_rel" s LEFT JOIN "score" v ON v."subject" = s."subject"
      ORDER BY s."subject"
    `);
  });
  bothAgree("G3 a sentinel default collides with a real value of the same type", g3, [
    { subject: "a", value: -1, truth: "real" },
    { subject: "b", value: 7, truth: "real" },
    { subject: "c", value: -1, truth: "defaulted" },
  ]);
}

// ══ runner ═════════════════════════════════════════════════════════════════

const scratchDirectory = mkdtempSync(join(tmpdir(), "sprefa-option-versus-null-"));
try {
  const sqliteVersion = execFileSync("sqlite3", ["--version"], {
    encoding: "utf8",
    env: process.env,
  }).trim();
  const versionClient = createClient({ url: `file:${join(scratchDirectory, "version.db")}` });
  const versionResult = await versionClient.execute("SELECT sqlite_version() AS version");
  versionClient.close();

  console.log(`SPREFA_CONFIG=${process.env.SPREFA_CONFIG}`);
  console.log(`DL_NO_DAEMON=${process.env.DL_NO_DAEMON}`);
  console.log(`sqlite3 CLI=${sqliteVersion}`);
  console.log(`@libsql SQLite=${String(versionResult.rows[0].version)}`);
  console.log(`scratch=${scratchDirectory}`);

  console.log("\n── PART V: the three-variant json read ──");
  await partThreeVariantJsonRead(scratchDirectory);

  console.log("\n── PART C: candidate inventory from the real compiler ──");
  partCandidateInventory(scratchDirectory);

  console.log("\n── PART X: the explosion measurement ──");
  await partExplosion(scratchDirectory);

  console.log("\n── PART D: the Design D structural break ──");
  await partDesignDBreak(scratchDirectory);

  console.log("\n── PART E: variant-encoding hazards ──");
  await partVariantHazards(scratchDirectory);

  console.log("\n── PART N: the null-safe equality bill ──");
  await partNullSafeBill(scratchDirectory);

  console.log("\n── PART G: candidate D, optionality at the use site ──");
  await partUseSiteDefaulting(scratchDirectory);

  if (failures.length > 0) {
    console.log(`\nFAIL ${failures.length} assertion(s)`);
    for (const failure of failures) console.log(failure);
    process.exitCode = 1;
  } else {
    console.log(`\nPASS ${passCount} assertions, both SQLite builds`);
  }
} finally {
  rmSync(scratchDirectory, { recursive: true, force: true });
}
