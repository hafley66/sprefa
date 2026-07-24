/**
 * tests/4_hosts.test.ts - M4 gate: the `?` probe machinery end to end (demand row ->
 * digest-cached effect -> response commit) plus the sg builtin/sh parity law. Numbered
 * 4_ because it exercises both the runtime (3_runtime.ts) and the hosts (1_hosts.ts).
 *
 * One golden (test 1) proves the whole path -- fire once per distinct request row, the
 * cache proves it, downstream rels update. The rest are unit tests reaching what the
 * golden can't isolate: parity (test 2), the {col}/$col template split (test 3), the
 * error path's stream-survives law (test 4), and the extract-as-a-host exposure (test
 * 5, task 4.4).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

import { bridge } from "../src/0_ast_bridge.ts";
import { builtinExtract, builtinSg, shHost } from "../src/1_hosts.ts";
import type { BridgeOk, HostDecl } from "../tasks.d.ts";
import { builtinRelsForTests } from "./0_helpers.ts";
import { bridgeHostsFixture, bootHostRunnerFixture, disposeHostFixture, effectCacheDump, waitUntil } from "./2_helpers_hosts.ts";
import { edbBatch, rowsOf } from "./1_helpers_db.ts";

const GOLDEN_PATH = path.join(import.meta.dirname, "..", "fixtures", "golden", "hosts.sg-timeline.json");

async function drain<T>(iterable: AsyncIterable<T>): Promise<T[]> {
  const items: T[] = [];
  for await (const item of iterable) items.push(item);
  return items;
}

test("sg? fires exactly once per distinct request row (the cache proves it)", async () => {
  const bridgeOk = bridgeHostsFixture("sg-rail.dl");
  const fixture = await bootHostRunnerFixture(bridgeOk, [builtinSg]);
  try {
    await fixture.rt.commit(edbBatch({ file: [{ path: "fixtures/corpus/bad.ts" }] }));

    // bounded poll: the effect runs on a real child process, asynchronously, after
    // commit()'s promise (correlated to the EDB+derived tick only) has already
    // resolved.
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_sg");
      return respRows.length > 0 ? respRows : undefined;
    });

    const snapshot = [
      await rowsOf(fixture.rt, "__req_sg"),
      await effectCacheDump(fixture.dbPath),
      await rowsOf(fixture.rt, "__resp_sg"),
      await rowsOf(fixture.rt, "console_hit"),
    ];

    if (process.env.REGEN_GOLDEN === "1") {
      fs.mkdirSync(path.dirname(GOLDEN_PATH), { recursive: true });
      fs.writeFileSync(GOLDEN_PATH, `${JSON.stringify(snapshot, null, 2)}\n`);
    }
    const expected: unknown = JSON.parse(fs.readFileSync(GOLDEN_PATH, "utf8"));
    assert.deepEqual(snapshot, expected);

    // second identical commit (same file, same path): the demand row is unchanged
    // (the pre-check sees it already exists), so HostRunner never even sees a new
    // __req_sg insert -- the cache proves the effect fired exactly once regardless.
    await fixture.rt.commit(edbBatch({ file: [{ path: "fixtures/corpus/bad.ts" }] }));
    assert.equal((await effectCacheDump(fixture.dbPath)).length, 1);
    assert.deepEqual(await rowsOf(fixture.rt, "__resp_sg"), snapshot[2]);
  } finally {
    await disposeHostFixture(fixture);
  }
});

test("parity: builtinSg vs sh sg decl produce byte-equal response rows", async () => {
  // sg's own `--json` output nests its byte offsets at `range.byteOffset.start/end`
  // (verified earlier against this worktree's corpus fixture) -- not a flat
  // {pattern,path,start,end,text} shape. shHost's generic parser only ever targets
  // OUTPUT-only columns (request columns are always merged in verbatim, never
  // re-parsed from a tool's own output -- see 1_hosts.ts's shHost doc comment), so the
  // sh decl reshapes sg's nested JSON into the flat output-only shape {start,end,text}
  // with a `jq` tail on the template -- an ordinary shell line, not a change to the
  // generic executor. With that, no renaming step is even needed: both HostDefs
  // produce identical rows outright.
  const shSgDecl: HostDecl = {
    name: "sg_sh",
    columns: [
      { name: "pattern", ty: "text" },
      { name: "path", ty: "text" },
      { name: "start", ty: "int" },
      { name: "end", ty: "int" },
      { name: "text", ty: "text" },
    ],
    template:
      "sg run --pattern '{pattern}' --json $path | " +
      "jq '[.[] | {start: .range.byteOffset.start, end: .range.byteOffset.end, text: .text}]'",
    inputCols: ["pattern", "path"],
  };
  const shSg = shHost(shSgDecl);
  const request = { pattern: "console.log($$$ARGS)", path: "fixtures/corpus/bad.ts" };

  const shRows = await drain(shSg.run(request));
  const builtinRows = await drain(builtinSg.run(request));

  assert.equal(shRows.length, 1);
  assert.deepEqual(shRows, builtinRows);
});

test("template fill: {col} splices raw, $col goes to env", async () => {
  const decl: HostDecl = {
    name: "greet",
    columns: [
      { name: "greeting", ty: "text" },
      { name: "subject", ty: "text" },
      { name: "echoed", ty: "text" },
    ],
    template: 'echo "{greeting}-$subject"',
    inputCols: ["greeting", "subject"],
  };
  const host = shHost(decl);
  const rows = await drain(host.run({ greeting: "hello", subject: "world" }));
  // "hello" arrives via the raw {greeting} splice (baked into the command string
  // before the shell ever runs); "world" arrives via the $subject env var the shell
  // itself expands. Both paths land in the same echoed output.
  assert.deepEqual(rows, [{ greeting: "hello", subject: "world", echoed: "hello-world" }]);
});

test("error effect: cache state 'error', no resp rows, stream lives", async () => {
  const dlText = `
rel bad_result(path: text, out: text).

sh failing(path: text, out: text) = \`echo "{path}"; exit 1\`.

bad_result(path, out) <- file(path), failing?(path, out).
`;
  const bridgeResult = bridge(dlText, builtinRelsForTests());
  assert.equal(
    bridgeResult.kind,
    "ok",
    bridgeResult.kind === "err" ? bridgeResult.diags.map((diag) => diag.message).join("; ") : undefined,
  );
  const bridgeOk = bridgeResult as BridgeOk;
  const failingDecl = bridgeOk.hosts.find((host) => host.name === "failing");
  assert.ok(failingDecl, "expected a 'failing' host decl from the bridge");

  const fixture = await bootHostRunnerFixture(bridgeOk, [shHost(failingDecl!)]);
  try {
    await fixture.rt.commit(edbBatch({ file: [{ path: "a.ts" }] }));
    await waitUntil(async () => {
      const cache = await effectCacheDump(fixture.dbPath);
      return cache.some((entry) => entry.state === "error") ? cache : undefined;
    });

    const cacheAfterFirst = await effectCacheDump(fixture.dbPath);
    assert.equal(cacheAfterFirst.length, 1);
    assert.equal(cacheAfterFirst[0]?.state, "error");
    assert.deepEqual(await fixture.rt.rows("__resp_failing"), []);
    assert.deepEqual(await fixture.rt.rows("bad_result"), []);

    // a SECOND, distinct request (a different path -> a different digest): the
    // pipeline is still alive after the first effect's error.
    await fixture.rt.commit(edbBatch({ file: [{ path: "b.ts" }] }));
    await waitUntil(async () => {
      const cache = await effectCacheDump(fixture.dbPath);
      return cache.length === 2 && cache.every((entry) => entry.state === "error") ? cache : undefined;
    });

    const cacheAfterSecond = await effectCacheDump(fixture.dbPath);
    assert.equal(cacheAfterSecond.length, 2);
    assert.ok(cacheAfterSecond.every((entry) => entry.state === "error"));
    assert.notEqual(cacheAfterSecond[0]?.digest, cacheAfterSecond[1]?.digest);
  } finally {
    await disposeHostFixture(fixture);
  }
});

test("extract host exposure: one demand row streams the file's records as rows", async () => {
  const rows = await drain(builtinExtract.run({ path: "fixtures/corpus/bad.ts" }));
  assert.equal(rows.length, 79);
  for (const row of rows) {
    assert.equal(row.path, "fixtures/corpus/bad.ts");
    assert.doesNotThrow(() => JSON.parse(String(row.record_json)));
  }
});
