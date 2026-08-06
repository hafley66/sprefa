import { concatMap, firstValueFrom, of, toArray } from "rxjs";
import { ScratchStore } from "../../../tsv2/runtime/scratchStore.ts";
import { BootRunner } from "../../../tsv2/runtime/2_boot.ts";
import { TickFold } from "../../../tsv2/runtime/tickLoop.ts";
import type { IArrivalBatch, IGenProgram } from "../../../tsv2/runtime/types.ts";

const modulePath = process.argv[2]!;
const side = Number(process.argv[3] ?? 45);
const edges: [number, number][] = [];
for (let row = 0; row < side; row += 1) {
  for (let col = 0; col < side; col += 1) {
    const node = row * side + col;
    if (col + 1 < side) edges.push([node, node + 1]);
    if (row + 1 < side) edges.push([node, node + side]);
  }
}
const loaded = (await import(modulePath)) as { program: IGenProgram };
const program = loaded.program;
const seam = { ...ScratchStore.open(":memory:"), unreadRels: new Set(["edge", "reachable"]) };
await firstValueFrom(ScratchStore.boot(seam, program.ddl));
await firstValueFrom(BootRunner.run(seam, program.boot));

const feed = async (batch: IArrivalBatch) => {
  const started = performance.now();
  await firstValueFrom(TickFold.run(program, seam, [batch], 1_000_000).pipe(toArray()));
  return Math.round(performance.now() - started);
};
const rowCount = async () =>
  Number((await firstValueFrom(seam.runner.execute(seam.db, `SELECT count(*) AS n FROM "reachable"`))).rows[0]!.n);

const build = await feed(edges.map(([s, t]) => ({ rel: "edge", sign: "add" as const, row: [s, t] })));
const head = await rowCount();
const jump: IArrivalBatch = [{ rel: "edge", sign: "add", row: [0, side * side - 2] }];
const insertOne = await feed(jump);
const afterInsert = await rowCount();
const deleteOne = await feed([{ rel: "edge", sign: "del", row: [0, side * side - 2] }]);
const afterDelete = await rowCount();
const drain = await feed([]);
const structural = await feed([{ rel: "edge", sign: "del", row: [0, 1] }]);
const afterStructural = await rowCount();
const batchAdd: IArrivalBatch = [];
for (let index = 0; index < 100; index += 1) {
  const from = (index * 7919) % (side * side);
  const to = (index * 104729 + 13) % (side * side);
  if (from !== to) batchAdd.push({ rel: "edge", sign: "add", row: [Math.min(from, to), Math.max(from, to)] });
}
const insertBatch = await feed(batchAdd);
const afterBatch = await rowCount();
const deleteBatch = await feed(batchAdd.map((row) => ({ ...row, sign: "del" as const })));
const afterUnbatch = await rowCount();
console.log(`build_ms=${build} head=${head}`);
console.log(`insert_one_ms=${insertOne} head=${afterInsert}`);
console.log(`delete_one_ms=${deleteOne} head=${afterDelete}`);
console.log(`drain_ms=${drain}`);
console.log(`delete_structural_ms=${structural} head=${afterStructural}`);
console.log(`insert_batch100_ms=${insertBatch} head=${afterBatch}`);
console.log(`delete_batch100_ms=${deleteBatch} head=${afterUnbatch}`);
seam.db.close();
