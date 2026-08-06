import {
  openDatabase,
  loadEdges,
  readEdges,
  foldChecksumStreaming,
  foldChecksumPaged,
  foldChecksumPagedRowid,
  foldChecksumConcat,
} from "./common.mjs";
import { variants } from "./variants.mjs";

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const inputPath = argumentValue("--input", null);
const variantName = argumentValue("--variant", "loop_notexists_wor");
const pragmaSet = argumentValue("--pragmas", "tuned");
const foldMode = argumentValue("--fold", "streaming");
if (inputPath === null) {
  process.stderr.write("race_one: --input <path> is required\n");
  process.exit(2);
}
const variant = variants[variantName];
if (variant === undefined) {
  process.stderr.write(`race_one: unknown variant ${variantName}\n`);
  process.exit(2);
}

const loadStartedAt = performance.now();
const { edges } = readEdges(inputPath);
const db = openDatabase(pragmaSet);
if (variant.schema.trim().length > 0) db.exec(variant.schema);
loadEdges(db, edges);
const loadMs = performance.now() - loadStartedAt;

const fixpointStartedAt = performance.now();
const { rounds, statements } = variant.derive(db);
const fixpointMs = performance.now() - fixpointStartedAt;

const countStartedAt = performance.now();
const derivedRows = variant.streamOnly
  ? -1
  : db.prepare(`SELECT count(*) AS rows FROM reachable`).get().rows;
const countMs = performance.now() - countStartedAt;

const foldStartedAt = performance.now();
let folded;
if (foldMode === "concat") {
  folded = foldChecksumConcat(db, "reachable");
} else if (foldMode === "covering") {
  folded = foldChecksumStreaming(db, `SELECT source, target FROM reachable ORDER BY source, target`);
} else if (foldMode === "paged" && variant.pagedKind === "pk") {
  folded = foldChecksumPaged(db, "reachable");
} else if (foldMode === "paged" && variant.pagedKind === "rowid") {
  folded = foldChecksumPagedRowid(db, "reachable");
} else {
  folded = foldChecksumStreaming(db, variant.scanSql);
}
const foldMs = performance.now() - foldStartedAt;

process.stdout.write(
  `${JSON.stringify({
    variant: variantName,
    pragmas: pragmaSet,
    fold: foldMode,
    input: inputPath.split("/").pop(),
    edges: edges.length,
    derived: derivedRows === -1 ? folded.rowCount : derivedRows,
    foldedRows: folded.rowCount,
    checksum: folded.checksum,
    loadMs: Math.round(loadMs),
    fixpointMs: Math.round(fixpointMs),
    countMs: Math.round(countMs),
    foldMs: Math.round(foldMs),
    rounds,
    statements,
    peakRssKb: process.resourceUsage().maxRSS,
  })}\n`,
);
db.close();
