import {
  openDatabase,
  loadEdges,
  readEdges,
  foldChecksumConcat,
} from "./common.mjs";
import { variants } from "./variants.mjs";

const DEFAULT_VARIANT = "loop_range_rowid";
const DEFAULT_PRAGMAS = "chosen";

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const inputPath = argumentValue("--input", null);
if (inputPath === null) {
  process.stderr.write("sqlite_raw: --input <path> is required\n");
  process.exit(2);
}
const variantName = argumentValue("--variant", DEFAULT_VARIANT);
const pragmaSet = argumentValue("--pragmas", DEFAULT_PRAGMAS);
const variant = variants[variantName];
if (variant === undefined) {
  process.stderr.write(`sqlite_raw: unknown variant ${variantName}\n`);
  process.exit(2);
}

try {
  const loadStartedAt = performance.now();
  const { edges } = readEdges(inputPath);
  const db = openDatabase(pragmaSet);
  if (variant.schema.trim().length > 0) db.exec(variant.schema);
  loadEdges(db, edges);
  const loadMs = Math.round(performance.now() - loadStartedAt);
  process.stdout.write(`{"event":"loaded","edges":${edges.length},"ms":${loadMs}}\n`);

  const fixpointStartedAt = performance.now();
  const shape = variant.derive(db);
  const fixpointMs = Math.round(performance.now() - fixpointStartedAt);
  const derived = db.prepare(`SELECT count(*) AS rows FROM reachable`).get().rows;
  process.stdout.write(`{"event":"fixpoint","derived":${derived},"ms":${fixpointMs}}\n`);
  process.stderr.write(
    `sqlite_raw: variant=${variantName} pragmas=${pragmaSet} rounds=${shape.rounds} statements=${shape.statements}\n`,
  );

  const foldStartedAt = performance.now();
  const folded = foldChecksumConcat(db, "reachable");
  const foldMs = Math.round(performance.now() - foldStartedAt);
  if (folded.rowCount !== derived) {
    process.stderr.write(`sqlite_raw: folded ${folded.rowCount} rows but counted ${derived}\n`);
    process.exit(1);
  }
  process.stdout.write(
    `{"event":"done","checksum":"${folded.checksum}","peak_rss_kb":${process.resourceUsage().maxRSS}}\n`,
  );
  process.stderr.write(`sqlite_raw: checksum_fold_ms=${foldMs}\n`);
  db.close();
} catch (failure) {
  process.stderr.write(`sqlite_raw: ${failure.message}\n`);
  process.exit(1);
}
