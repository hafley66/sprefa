// Summarize raw child runs from run.sh; no engine execution is performed here.
import {readFileSync,writeFileSync} from "node:fs";
import {join} from "node:path";
import assert from "node:assert/strict";
const out=process.argv[2];
const runs=readFileSync(join(out,"repeat-runs.tsv"),"utf8").trim().split("\n").slice(1).map(line=>line.split("\t"));
const groups=new Map(); const statuses=[];
let header;
for (const [phase,iteration,rotation,exit] of runs) {
  const dir=`${phase}-${iteration}`;
  statuses.push(`| ${phase} | ${iteration} | ${rotation} | ${exit} | [raw report](${dir}/REPORT.md) |`);
  if (phase !== "measured") continue;
  const csv=readFileSync(join(out,dir,"results.csv"),"utf8").trim().split("\n");
  header=csv.shift();
  for (const line of csv) {
    const fields=line.split(",");
    if (fields[5]==="WALL" || fields[5]==="") continue;
    assert.equal(fields.length,11);
    const key=fields.slice(0,4).join(",");
    const group=groups.get(key)??[]; group.push({iteration,fields}); groups.set(key,group);
  }
}
const median=values=>{const sorted=values.toSorted((a,b)=>a-b);const middle=Math.floor(sorted.length/2);return sorted.length%2?sorted[middle]:(sorted[middle-1]+sorted[middle])/2;};
const expected=runs.filter(([phase])=>phase==="measured").length;
const csv=[header??"engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb,host_peak_mb,sqlite_hw_mb,db_mb"];
const ranges=["engine,nodes,samples,setup_min_ms,setup_median_ms,setup_max_ms,retract_min_ms,retract_median_ms,retract_max_ms,rss_min_mb,rss_median_mb,rss_max_mb"];
const table=[];
for (const [key,samples] of [...groups].sort(([a],[b])=>a.localeCompare(b,undefined,{numeric:true}))) {
  const fixed=key.split(","); const aggregate=[...fixed];
  for (let col=4;col<11;col++) {
    const values=samples.map(({fields})=>Number(fields[col]));
    aggregate.push(values.every(Number.isFinite)?String(median(values)):"N/A");
  }
  const metrics=[4,5,7].flatMap(col=>{const values=samples.map(({fields})=>Number(fields[col]));return [Math.min(...values),median(values),Math.max(...values)];});
  ranges.push([fixed[0],fixed[1],samples.length,...metrics].join(","));
  if (samples.length===expected && expected>=5) csv.push(aggregate.join(","));
  table.push(`| ${fixed[0]} | ${fixed[1]} | ${samples.length}/${expected} | ${metrics[4].toFixed(3)} | ${metrics[3].toFixed(3)}–${metrics[5].toFixed(3)} | ${metrics[7].toFixed(1)} |`);
}
writeFileSync(join(out,"results.csv"),csv.join("\n")+"\n");
writeFileSync(join(out,"repeat-ranges.csv"),ranges.join("\n")+"\n");
writeFileSync(join(out,"REPORT.md"),`# Repeated shared root-retraction comparison

The warmup run is retained and excluded from all summaries. Each measured run
uses fresh engine processes and a fresh task-local PostgreSQL cluster. Engine
order rotates by three positions per run. Tables report medians and full ranges,
with no confidence or significance claim. Only groups present in all measured
runs, with at least five samples, enter the median charts.

Deletion, maintenance or recomputation/materialization, and count are timed.
Exact input and result validation is outside the clocks. PostgreSQL includes
durable commit and client round trips. SQLite runtime stores are in memory;
specialized cascade stores use their existing on-disk configuration. RSS and
memory-cap scopes differ by adapter and are recorded in the raw reports.
OS available-memory samples and engine order are retained in each raw directory.
The sqlite-signed-delta-v2 name currently runs full recursive recomputation with
implicit zero-indegree roots and only the current deletion excluded. Its single
deletion measurements pass, but repeated-deletion sequence checks fail.

![Median retraction](retract_ms.png)
![Median RSS](rss_mb.png)

| Engine | Nodes | Samples | Median retract ms | Min–max ms | Median RSS MB |
|---|---:|---:|---:|---:|---:|
${table.join("\n")}

[Machine-readable ranges](repeat-ranges.csv), [median chart input](results.csv).

## Raw run status

| Phase | Iteration | Rotation | Exit | Receipts |
|---|---:|---:|---:|---|
${statuses.join("\n")}
`);
