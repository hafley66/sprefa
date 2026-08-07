import { performance } from "node:perf_hooks";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { openDatabase, readEdges, loadEdges } from "./common.mjs";
import { variants } from "./variants.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_WORK = join(HERE, "..", "dl6", ".bench");
const PRAGMAS = "chosen";

const BANKED = {
  grid_10000: { derived: 1069200 },
  chain_10000: { derived: 9996213 },
  layered_10000: { derived: 9951396 },
};

const WINNER = variants.loop_range_rowid;

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const workDir = argumentValue("--work", DEFAULT_WORK);
const onlyCase = argumentValue("--only", null);
const caseNames = Object.keys(BANKED).filter(
  (name) => onlyCase === null || name === onlyCase,
);

const missing = caseNames.filter((name) => !existsSync(join(workDir, `${name}.in`)));
if (missing.length > 0) {
  process.stderr.write(
    `exp_tail: missing inputs ${missing.join(", ")} under ${workDir}; regenerate with the shootout harness\n`,
  );
  process.exit(2);
}

const TAIL_SCHEMA = `
  CREATE TEMP TABLE "__new_r" (
    source INTEGER NOT NULL,
    target INTEGER NOT NULL
  );
  CREATE TEMP TABLE "r" (
    source INTEGER NOT NULL,
    target INTEGER NOT NULL,
    "__refcount" INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (source, target)
  ) WITHOUT ROWID;
  CREATE TEMP TABLE "__delta_r" (
    "_sign" INTEGER NOT NULL,
    "_sequence" INTEGER NOT NULL,
    source INTEGER NOT NULL,
    target INTEGER NOT NULL
  );
  CREATE TEMP TABLE "__frontier_r" (
    "_phase" INTEGER NOT NULL,
    "_sequence" INTEGER NOT NULL,
    source INTEGER NOT NULL,
    target INTEGER NOT NULL
  );
`;

function measureCase(caseName, inputPath) {
  const banked = BANKED[caseName];
  const db = openDatabase(PRAGMAS);
  if (WINNER.schema.trim().length > 0) db.exec(WINNER.schema);
  const { edges } = readEdges(inputPath);
  loadEdges(db, edges);

  WINNER.derive(db);
  const derived = db.prepare(`SELECT count(*) AS rows FROM "reachable"`).get().rows;
  if (derived !== banked.derived) {
    throw new Error(
      `${caseName}: derived ${derived} != banked ${banked.derived}; refusing to time a wrong set`,
    );
  }

  db.exec(TAIL_SCHEMA);

  const clearNew = db.prepare(`DELETE FROM "__new_r"`);
  const clearHead = db.prepare(`DELETE FROM "r"`);
  const clearTail = db.prepare(`DELETE FROM "__delta_r"; DELETE FROM "__frontier_r";`);
  const reset = () => {
    clearNew.run();
    clearHead.run();
    clearTail.run();
  };

  const fillSimple = db.prepare(
    `INSERT INTO "__new_r" (source, target) SELECT source, target FROM "reachable" ORDER BY rowid`,
  );
  const insertHead = db.prepare(
    `INSERT OR IGNORE INTO "r" (source, target) SELECT source, target FROM "__new_r"`,
  );
  const deltaCopy = db.prepare(
    `INSERT INTO "__delta_r" ("_sign", "_sequence", source, target)
     SELECT 1, "rowid" - 1, source, target FROM "__new_r"`,
  );
  const frontierCopy = db.prepare(
    `INSERT INTO "__frontier_r" ("_phase", "_sequence", source, target)
     SELECT 0, "rowid" - 1, source, target FROM "__new_r"`,
  );
  const fillAntijoin = db.prepare(
    `INSERT INTO "__new_r" (source, target)
     SELECT n.source, n.target
     FROM "reachable" n LEFT JOIN "r" h
       ON h.source = n.source AND h.target = n.target
     WHERE h.source IS NULL`,
  );
  const headCount = () => db.prepare(`SELECT count(*) AS rows FROM "r"`).get();

  const ms = (t0, t1) => Math.round(t1 - t0);

  function runTailA() {
    reset();
    const t0 = performance.now();
    fillSimple.run();
    const t1 = performance.now();
    insertHead.run();
    const t2 = performance.now();
    return { fill: ms(t0, t1), insert: ms(t1, t2), total: ms(t0, t2) };
  }

  function runTailB() {
    reset();
    const t0 = performance.now();
    fillSimple.run();
    insertHead.run();
    deltaCopy.run();
    frontierCopy.run();
    const t1 = performance.now();
    return { total: ms(t0, t1) };
  }

  function runAntijoin() {
    reset();
    const t0 = performance.now();
    fillAntijoin.run();
    const t1 = performance.now();
    insertHead.run();
    const t2 = performance.now();
    return { fill: ms(t0, t1), insert: ms(t1, t2), total: ms(t0, t2) };
  }

  const bestOf2 = caseName === "chain_10000";
  const runs = bestOf2 ? 2 : 1;

  let bestTailA = null;
  let bestAntijoin = null;
  let bestTailB = null;

  for (let i = 0; i < runs; i++) {
    const a = runTailA();
    if (bestTailA === null || a.total < bestTailA.total) bestTailA = a;
    reset();
    const aj = runAntijoin();
    if (bestAntijoin === null || aj.total < bestAntijoin.total) bestAntijoin = aj;
    reset();
    const b = runTailB();
    if (bestTailB === null || b.total < bestTailB.total) bestTailB = b;
  }

  const headRows = headCount().rows;
  const headOkay = headRows === banked.derived ? "MATCH" : "MISMATCH";

  const result = {
    case: caseName,
    derived,
    headRows,
    match: headOkay,
    tailA_fill_ms: bestTailA.fill,
    tailA_insert_ms: bestTailA.insert,
    tailA_total_ms: bestTailA.total,
    antijoin_total_ms: bestAntijoin.total,
    tailB_total_ms: bestTailB.total,
  };
  db.close();
  return result;
}

for (const caseName of caseNames) {
  const result = measureCase(caseName, join(workDir, `${caseName}.in`));
  process.stdout.write(`${JSON.stringify(result)}\n`);
}
