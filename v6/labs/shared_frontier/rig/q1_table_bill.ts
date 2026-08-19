/**
 * Q1: today's table bill.
 *
 * Reads the `ddl` array out of an emitted tsv2 module, boots it into an
 * in-memory SQLite through the same driver the runtime uses, then counts from
 * `sqlite_master`. Counts are read back from the booted database, never
 * estimated from the emitted text.
 */

import { readFileSync } from "node:fs";

import { isTransient, markdownTable, openMemory, transientFamily } from "./common.ts";
import { extractDdl } from "./emitted.ts";

interface ITableBill {
  readonly program: string;
  readonly ddlBytes: number;
  readonly createStatements: number;
  readonly durableTables: number;
  readonly transientTables: number;
  readonly indexes: number;
  readonly views: number;
  readonly relations: number;
  readonly transientPerRelation: number;
  readonly byFamily: ReadonlyMap<string, number>;
}

async function bill(program: string, modulePath: string): Promise<ITableBill> {
  const ddl = extractDdl(readFileSync(modulePath, "utf8"));
  const db = openMemory();
  for (const statement of ddl) await db.execute(statement);

  // `CREATE TEMP TABLE` lands in `sqlite_temp_master`; reading `sqlite_master`
  // alone reports zero transient tables.
  const objects = await db.execute(
    "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'" +
      " UNION ALL SELECT type, name FROM sqlite_temp_master WHERE name NOT LIKE 'sqlite_%'",
  );
  let durableTables = 0;
  let transientTables = 0;
  let indexes = 0;
  let views = 0;
  const byFamily = new Map<string, number>();
  for (const row of objects.rows) {
    const kind = String(row[0]);
    const name = String(row[1]);
    if (kind === "index") indexes += 1;
    else if (kind === "view") views += 1;
    else if (kind === "table") {
      if (isTransient(name)) {
        transientTables += 1;
        const family = transientFamily(name) as string;
        byFamily.set(family, (byFamily.get(family) ?? 0) + 1);
      } else durableTables += 1;
    }
  }

  const ddlBytes = ddl.reduce((total, statement) => total + Buffer.byteLength(statement, "utf8"), 0);
  const createStatements = ddl.filter((statement) => /^\s*CREATE/i.test(statement)).length;
  const relations = countRelations(ddl);
  return {
    program,
    ddlBytes,
    createStatements,
    durableTables,
    transientTables,
    indexes,
    views,
    relations,
    transientPerRelation: relations === 0 ? 0 : transientTables / relations,
    byFamily,
  };
}

/** One relation = one `__frontier_<rel>` table; that family is minted exactly once per lowered relation. */
function countRelations(ddl: readonly string[]): number {
  return ddl.filter((statement) => /CREATE TEMP TABLE "__frontier_/.test(statement)).length;
}

const TARGETS: readonly (readonly [string, string])[] = [
  ["pokeapi_gen", "out/pokeapi_gen.ts"],
  ["key_last_write_wins (keyed)", "../../prolog/compile/out/key_last_write_wins.ts"],
  ["mutual_recursion_matches_oracle (recursive)", "../../prolog/compile/out/mutual_recursion_matches_oracle.ts"],
  ["bool_relation_negation_is_two_valued (negation)", "../../prolog/compile/out/bool_relation_negation_is_two_valued.ts"],
];

const bills: ITableBill[] = [];
for (const [program, path] of TARGETS) bills.push(await bill(program, path));

console.log("### Q1a. Table bill per emitted program\n");
console.log(
  markdownTable(
    ["program", "relations", "durable tables", "transient tables", "transient/relation", "indexes", "views", "CREATE statements", "DDL bytes"],
    bills.map((entry) => [
      entry.program,
      String(entry.relations),
      String(entry.durableTables),
      String(entry.transientTables),
      entry.transientPerRelation.toFixed(2),
      String(entry.indexes),
      String(entry.views),
      String(entry.createStatements),
      String(entry.ddlBytes),
    ]),
  ),
);

const families = [...new Set(bills.flatMap((entry) => [...entry.byFamily.keys()]))].sort();
console.log("\n### Q1b. Transient tables by family\n");
console.log(
  markdownTable(
    ["program", ...families],
    bills.map((entry) => [entry.program, ...families.map((family) => String(entry.byFamily.get(family) ?? 0))]),
  ),
);

const machine = bills.map((entry) => ({ ...entry, byFamily: Object.fromEntries(entry.byFamily) }));
console.log(`\n<!-- json ${JSON.stringify(machine)} -->`);
