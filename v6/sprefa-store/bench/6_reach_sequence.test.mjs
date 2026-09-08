import assert from "node:assert/strict";
import {spawnSync} from "node:child_process";
import {mkdirSync,writeFileSync,existsSync} from "node:fs";
import {dirname,resolve,join} from "node:path";
import {fileURLToPath} from "node:url";
import test from "node:test";
import {sequenceFixtures} from "./0_reach_sequences.mjs";

const bench=dirname(fileURLToPath(import.meta.url));
const root=resolve(bench,"../../..");
const fixtures=sequenceFixtures();

test("seeded fixtures cover legal set updates and DAG boundaries",()=>{
  assert.equal(fixtures.length,39);
  const operations=new Set();
  for (const fixture of fixtures) for (const {arrivals,expected} of fixture.ticks) {
    for (const arrival of arrivals) operations.add(`${arrival.rel}:${arrival.sign}`);
    assert.equal(new Set(expected.edge.map(row=>row.join(","))).size,expected.edge.length);
    if (fixture.dag) assert.ok(expected.edge.every(([p,c])=>p<c),fixture.name);
  }
  assert.deepEqual([...operations].sort(),["edge:add","edge:del","root:add","root:del"]);
});

test("actual shared adapters match BFS after every supported update",{skip:!process.env.BENCH_SEQUENCE_OUT},async t=>{
  const out=resolve(process.env.BENCH_SEQUENCE_OUT);
  assert.equal(existsSync(out),false,"choose a new sequence receipt destination");
  mkdirSync(join(out,"fixtures"),{recursive:true});
  mkdirSync(join(out,"logs"));
  const adapters=[
    ["differential-dataflow",file=>[resolve(bench,"../target/release/examples/dd_reach"),"--sequence",file]],
    ["swi-incr",file=>["swipl","-q","-s",join(bench,"swi_reach.pl"),"--","--sequence",file]],
    ["tsv2-runtime",file=>["node","--experimental-transform-types",join(root,"v6/tsv2/scripts/scale-bench.ts"),"reach-sequence",file]],
    ["sprefa-engine-rs",file=>[join(root,"v6/sprefa-engine-rs/target/release/examples/0_reach_bench"),join(bench,"1_root_reach.program.rs"),file,"--sequence"]],
    ["native-postgres-query",file=>["node",join(bench,"engines/1_postgres_reach.mjs"),"sequence-native",file]],
    ...["sqlite-count","sqlite-count-scc","sqlite-dred-loop","sqlite-dred-cte","sqlite-signed-delta-v2"].map(label=>
      [label,file=>[resolve(bench,"../target/release/examples/perf_report"),"--sequence",label,file]]),
  ].filter(([label])=>!process.env.BENCH_SEQUENCE_FILTER || process.env.BENCH_SEQUENCE_FILTER.split(" ").includes(label));
  const accounting=[];
  for (const fixture of fixtures) {
    const file=join(out,"fixtures",`${fixture.name}.json`);
    writeFileSync(file,JSON.stringify(fixture,null,2)+"\n");
    for (const [engine,command] of adapters) await t.test(`${engine}/${fixture.name}`,()=>{
      const [bin,...args]=command(file);
      const result=spawnSync(bin,args,{encoding:"utf8",timeout:30_000,maxBuffer:8*1024*1024,
        env:{...process.env,LC_ALL:"C",LANG:"C",DL_MEMCAP_MB:"2048",DL_TRACE_SUMMARY:"1"}});
      const log=join(out,"logs",`${engine}-${fixture.name}`);
      writeFileSync(log+".stdout",result.stdout??"");
      writeFileSync(log+".stderr",(result.stderr??"")+`\nEXIT_STATUS|${result.status}\nSIGNAL|${result.signal}\nERROR|${result.error??""}\n`);
      let records=[],failure=null,passed=0,unsupported=null;
      try {
        records=(result.stdout??"").trim().split("\n").filter(Boolean).map(line=>JSON.parse(line));
        for (const record of records) {
          if (record.unsupported) {unsupported=record.unsupported; continue;}
          assert.equal(record.tick,passed,"missing or reordered tick");
          assert.equal(record.carry_pending,false,"unsettled tick");
          assert.deepEqual(record.actual,fixture.ticks[passed].expected,`tick ${passed}`);
          passed++;
        }
        assert.equal(result.status,0,`child failure: ${result.stderr}`);
        if (!unsupported) assert.equal(passed,fixture.ticks.length,"missing final tick");
      } catch (error) {failure=String(error);}
      const record={engine,fixture:fixture.name,status:failure?"fail":unsupported?"unsupported":"pass",passed_ticks:passed,total_ticks:fixture.ticks.length,reason:failure??unsupported??"exact roots, edges, and alive sets match BFS"};
      accounting.push(record);
      writeFileSync(join(out,"accounting.json"),JSON.stringify(accounting,null,2)+"\n");
      if (failure) {
        // This is the first failing prefix. It is a reproduction, not a claim
        // of globally minimal graph size; explicit reductions are recorded separately.
        writeFileSync(log+".failing-prefix.json",JSON.stringify({...fixture,ticks:fixture.ticks.slice(0,passed+1)},null,2)+"\n");
        assert.fail(`${engine}/${fixture.name}: ${failure}`);
      }
    });
  }
});
