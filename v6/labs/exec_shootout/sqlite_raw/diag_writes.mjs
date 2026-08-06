import { openDatabase, loadEdges, readEdges } from "./common.mjs";

const inputPath = process.argv[2];
if (inputPath === undefined) {
  process.stderr.write("diag_writes: <input path>\n");
  process.exit(2);
}

const { edges } = readEdges(inputPath);
const db = openDatabase("chosen");
db.exec(`
  CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL);
  CREATE UNIQUE INDEX reachable_pair ON reachable (source, target);
`);
loadEdges(db, edges);

const countCandidates = db.prepare(`
  SELECT count(*) AS candidates
  FROM reachable known JOIN edge ON edge.source = known.target
  WHERE known.rowid BETWEEN ? AND ?
`);
const step = db.prepare(`
  INSERT OR IGNORE INTO reachable (source, target)
  SELECT known.source, edge.target
  FROM reachable known JOIN edge ON edge.source = known.target
  WHERE known.rowid BETWEEN ? AND ?
`);

let candidates = 0;
let low = 1;
let high = db
  .prepare(`INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM edge`)
  .run().changes;
let rounds = 0;
for (;;) {
  candidates += countCandidates.get(low, high).candidates;
  const derived = step.run(low, high).changes;
  if (derived === 0) break;
  low = high + 1;
  high += derived;
  rounds += 1;
}
const derivedRows = db.prepare(`SELECT count(*) AS rows FROM reachable`).get().rows;
process.stdout.write(
  `${JSON.stringify({
    input: inputPath.split("/").pop(),
    derived: derivedRows,
    rounds,
    joinCandidates: candidates,
    rejectedDuplicates: candidates - (derivedRows - edges.length),
    btreeWritesPerDerivedRow:
      Number((candidates + 2 * derivedRows) / derivedRows).toFixed(2),
  })}\n`,
);
db.close();
