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
import {
  bootHostRunnerFixture,
  bridgeHostsFixture,
  disposeHostFixture,
  effectCacheDump,
  makeCountingHost,
  waitUntil,
} from "./2_helpers_hosts.ts";
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
    // M8-alpha's file(path, content_hash) spine (2 cols): the committed row needs a
    // real witness value so sg?'s salted probe (content_hash is salt_0) has something
    // to key its digest on -- "hash_v1" stands in for a real sha-256 hex here since
    // this test never touches the file's actual bytes.
    await fixture.rt.commit(edbBatch({ file: [{ path: "fixtures/corpus/bad.ts", content_hash: "hash_v1" }] }));

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

    // second identical commit (same file, same path, SAME witness): the demand row is
    // unchanged (the pre-check sees it already exists), so HostRunner never even sees
    // a new __req_sg insert -- the cache proves the effect fired exactly once
    // regardless.
    await fixture.rt.commit(edbBatch({ file: [{ path: "fixtures/corpus/bad.ts", content_hash: "hash_v1" }] }));
    assert.equal((await effectCacheDump(fixture.dbPath)).length, 1);
    assert.deepEqual(await rowsOf(fixture.rt, "__resp_sg"), snapshot[2]);
  } finally {
    await disposeHostFixture(fixture);
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// M8-beta: supersession + ABA, over a counting fake host (deterministic, no
// subprocess -- see tests/2_helpers_hosts.ts's makeCountingHost). `counter(path,
// value)` declares `{path}` as its only template ref (inputCols = ["path"]), so
// `counter?(path, content_hash, value)` passes ONE extra arg beyond the host's 2
// declared columns -- exactly the salt-arg law's shape (salt_0 = content_hash),
// same as sg-rail.dl's real `sg?` probe. The `sh` template text is never actually
// run: bootHostRunnerFixture is handed the counting fake HostDef under the SAME
// name, which HostRunner dispatches to instead.
// ─────────────────────────────────────────────────────────────────────────────

const COUNTER_PROGRAM_DL = `
rel probe_hit(path: text, value: int).

sh counter(path: text, value: int) = \`echo {path}\`.

probe_hit(path, value) <- file(path, content_hash), counter?(path, content_hash, value).
`;

function bridgeCounterProgram(): BridgeOk {
  const bridgeResult = bridge(COUNTER_PROGRAM_DL, builtinRelsForTests());
  assert.equal(
    bridgeResult.kind,
    "ok",
    bridgeResult.kind === "err" ? bridgeResult.diags.map((diag) => diag.message).join("; ") : undefined,
  );
  return bridgeResult as BridgeOk;
}

test("fire-once per FULL digest: same (path, content_hash) committed twice -> one cache row, one run", async () => {
  const bridgeOk = bridgeCounterProgram();
  const counting = makeCountingHost("counter");
  const fixture = await bootHostRunnerFixture(bridgeOk, [counting.host]);
  try {
    await fixture.rt.commit(edbBatch({ file: [{ path: "a.ts", content_hash: "hash_v1" }] }));
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_counter");
      return respRows.length > 0 ? respRows : undefined;
    });
    assert.equal(counting.runCount, 1);
    assert.equal((await effectCacheDump(fixture.dbPath)).length, 1);

    // re-commit the IDENTICAL (path, content_hash): applyEdbTxn's pre-check sees the
    // file row already exists, so __req_counter never even sees a new insert -- same
    // fire-once law as the sg? golden test above, now over the fake host.
    await fixture.rt.commit(edbBatch({ file: [{ path: "a.ts", content_hash: "hash_v1" }] }));
    assert.equal(counting.runCount, 1);
    assert.equal((await effectCacheDump(fixture.dbPath)).length, 1);
  } finally {
    await disposeHostFixture(fixture);
  }
});

test("witness change supersedes: v1 -> v2 in one commit retracts the old resp rows, keeps only the new cache row", async () => {
  const bridgeOk = bridgeCounterProgram();
  const counting = makeCountingHost("counter");
  const fixture = await bootHostRunnerFixture(bridgeOk, [counting.host]);
  try {
    await fixture.rt.commit(edbBatch({ file: [{ path: "a.ts", content_hash: "hash_v1" }] }));
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_counter");
      return respRows.length > 0 ? respRows : undefined;
    });
    assert.deepEqual(await rowsOf(fixture.rt, "__resp_counter"), [{ path: "a.ts", salt_0: "hash_v1", value: 1 }]);
    assert.equal((await effectCacheDump(fixture.dbPath)).length, 1);

    // ONE commit retracting v1's file row and inserting v2's -- exactly the shape
    // ingestFile produces for a same-path content edit (4_ingest.ts's diff, never two
    // separate commits). This is the honest re-fix of curl-session.sh's console_hit
    // finding: the new witness's __req_counter row lands, HostRunner fires again, and
    // the PRIOR witness's __resp_counter rows retract in the SAME commit as the new
    // insert.
    await fixture.rt.commit(
      edbBatch(
        { file: [{ path: "a.ts", content_hash: "hash_v2" }] },
        { file: [{ path: "a.ts", content_hash: "hash_v1" }] },
      ),
    );
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_counter");
      return respRows.some((row) => row.salt_0 === "hash_v2") ? respRows : undefined;
    });

    assert.equal(counting.runCount, 2);
    assert.deepEqual(await rowsOf(fixture.rt, "__resp_counter"), [{ path: "a.ts", salt_0: "hash_v2", value: 2 }]);

    // the orchestrator's latest-wins interpretation (IdentityWitnessLaw, tasks.d.ts;
    // flagged in 1_hosts.ts's runEffectOnce, not owner-ruled): effect_cache tracks only
    // the LIVE witness per identity, so the v1 cache row is gone too, not just its rows.
    const cache = await effectCacheDump(fixture.dbPath);
    assert.equal(cache.length, 1);
    assert.equal(cache[0]?.state, "done");
  } finally {
    await disposeHostFixture(fixture);
  }
});

test("A->B->A: flipping back to a prior witness re-fires (its old cache row was deleted, not reused)", async () => {
  const bridgeOk = bridgeCounterProgram();
  const counting = makeCountingHost("counter");
  const fixture = await bootHostRunnerFixture(bridgeOk, [counting.host]);
  try {
    await fixture.rt.commit(edbBatch({ file: [{ path: "a.ts", content_hash: "hash_v1" }] })); // A
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_counter");
      return respRows.length > 0 ? respRows : undefined;
    });

    await fixture.rt.commit(
      // A -> B
      edbBatch(
        { file: [{ path: "a.ts", content_hash: "hash_v2" }] },
        { file: [{ path: "a.ts", content_hash: "hash_v1" }] },
      ),
    );
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_counter");
      return respRows.some((row) => row.salt_0 === "hash_v2") ? respRows : undefined;
    });
    assert.equal(counting.runCount, 2);

    await fixture.rt.commit(
      // B -> A: hash_v1's PRIOR cache row was DELETEd by the latest-wins interpretation
      // when B superseded it, so this is a cache MISS again, not a hit served against
      // rows that were retracted a moment ago -- fire-once holds only while a witness
      // is the CURRENT one for its identity group.
      edbBatch(
        { file: [{ path: "a.ts", content_hash: "hash_v1" }] },
        { file: [{ path: "a.ts", content_hash: "hash_v2" }] },
      ),
    );
    await waitUntil(async () => {
      const respRows = await fixture.rt.rows("__resp_counter");
      return respRows.some((row) => row.value === 3) ? respRows : undefined;
    });

    assert.equal(counting.runCount, 3); // A, B, A again -- 3 real runs, not 2.
    assert.deepEqual(await rowsOf(fixture.rt, "__resp_counter"), [{ path: "a.ts", salt_0: "hash_v1", value: 3 }]);
    const cache = await effectCacheDump(fixture.dbPath);
    assert.equal(cache.length, 1); // only the live (3rd) witness's cache row survives
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

// ─────────────────────────────────────────────────────────────────────────────
// F7 regression (docs/failure-modes.md class 36): a `sh` host with MORE THAN ONE
// output-only column, whose template prints one VALUE per LINE (fixtures/
// ghcacher.dl:52-57's `printf '%s\n%s\n%s' "$C" "$E" "$B"` shape) used to be
// misread by parseWhitespaceColumns as ONE ROW PER LINE (whitespace-split WITHIN
// each line) instead of one row with one column value PER LINE. For a body value
// containing internal spaces (any real JSON payload), that shredded a single
// response into several malformed rows, and one of them ran Number() over a
// non-numeric token (the OTHER line's own value, landed in the wrong column) and
// minted a bare NaN that reached SQL text unquoted -- `SQLITE_ERROR: no such
// column: NaN` out of `execute$` (3_runtime.ts).
// ─────────────────────────────────────────────────────────────────────────────

test("multi-line sh output (one value per line, >1 output column) parses to ONE row, not one row per line", async () => {
  const decl: HostDecl = {
    name: "triple",
    columns: [
      { name: "id", ty: "text" },
      { name: "status", ty: "int" },
      { name: "tag", ty: "text" },
      { name: "body", ty: "text" },
    ],
    template: `printf '200\\n"etag-value"\\nsome body text with several spaced words'`,
    inputCols: ["id"],
  };
  const host = shHost(decl);
  const rows = await drain(host.run({ id: "x" }));
  // Pre-fix this was 3 rows (one per line), the middle one holding `status: NaN`
  // (Number() applied to the tag line's own text, which landed in the status slot).
  assert.equal(rows.length, 1);
  assert.deepEqual(rows[0], {
    id: "x",
    status: 200,
    tag: `"etag-value"`,
    body: "some body text with several spaced words",
  });
});

const GHCACHER_SHAPE_DL = `
rel watch(ep: text).
watch("test/repo").

rel etag(ep: text, tag: text).
etag(ep, "") <- watch(ep).
etag(ep, tag) <- resp(ep, 200, tag, _).

rel poll(ep: text, prev: text).
poll(ep, prev) <- watch(ep), etag(ep, prev).

sh fetch(ep: text, prev: text, status: int, tag: text, body: text) =
  \`STATUS=200
   TAG="etag-stub-abc123"
   PREV_UNUSED="$prev"
   FILLER=$(printf '%.0sx' $(seq 1 4000))
   BODY=$(printf '{"full_name": "%s", "stargazers_count": 44821, "description": "a quoted description with several words", "filler": "%s"}' "$ep" "$FILLER")
   printf '%s\\n%s\\n%s' "$STATUS" "$TAG" "$BODY"\`.

rel resp(ep: text, status: int, tag: text, body: text).
resp(ep, status, tag, body) <- poll(ep, prev), fetch?(ep, prev, status, tag, body).

sh extract_stars(body: text, n: text) =
  \`printf '%s' "$body" | jq -r '.stargazers_count // empty'\`.
rel stars(ep: text, n: text).
stars(ep, n) <- resp(ep, 200, _, body), extract_stars?(body, n).

sh extract_full_name(body: text, name: text) =
  \`printf '%s' "$body" | jq -r '.full_name // empty'\`.
rel full_name(ep: text, name: text).
full_name(ep, name) <- resp(ep, 200, _, body), extract_full_name?(body, name).

rel change_log(ep: text, kind: text, val: text).
change_log(ep, "stars", n) <- stars(ep, n).
change_log(ep, "full_name", v) <- full_name(ep, v).
`;

function bridgeGhcacherShapeProgram(): BridgeOk {
  const bridgeResult = bridge(GHCACHER_SHAPE_DL, builtinRelsForTests());
  assert.equal(
    bridgeResult.kind,
    "ok",
    bridgeResult.kind === "err" ? bridgeResult.diags.map((diag) => diag.message).join("; ") : undefined,
  );
  return bridgeResult as BridgeOk;
}

test("F7 regression end to end: ghcacher's fetch/resp/stars/full_name/change_log chain lands one real (non-NaN) response and the stream survives past it", async () => {
  const bridgeOk = bridgeGhcacherShapeProgram();
  const hosts = bridgeOk.hosts.map(shHost);
  const fixture = await bootHostRunnerFixture(bridgeOk, hosts);
  try {
    const respRows = await waitUntil(async () => {
      const rows = await fixture.rt.rows("resp");
      return rows.length > 0 ? rows : undefined;
    });

    // Exactly one row, not the 3 malformed ones the unfixed parser produced.
    assert.equal(respRows.length, 1);
    const respRow = respRows[0]!;
    assert.equal(respRow.ep, "test/repo");
    assert.equal(respRow.status, 200);
    assert.equal(respRow.tag, "etag-stub-abc123");
    assert.ok(
      typeof respRow.body === "string" && respRow.body.length > 4000,
      "body should be the multi-KB JSON payload",
    );
    assert.doesNotThrow(() => JSON.parse(String(respRow.body)));

    const starsRows = await waitUntil(async () => {
      const rows = await fixture.rt.rows("stars");
      return rows.length > 0 ? rows : undefined;
    });
    assert.deepEqual(starsRows, [{ ep: "test/repo", n: "44821" }]);

    const fullNameRows = await waitUntil(async () => {
      const rows = await fixture.rt.rows("full_name");
      return rows.length > 0 ? rows : undefined;
    });
    assert.deepEqual(fullNameRows, [{ ep: "test/repo", name: "test/repo" }]);

    // change_log's own rows prove the post-response tick(s) reached the whole
    // derived chain, not just resp -- this is the "post-response tick" receipt.
    const changeLogRows = await rowsOf(fixture.rt, "change_log");
    assert.deepEqual(changeLogRows, [
      { ep: "test/repo", kind: "full_name", val: "test/repo" },
      { ep: "test/repo", kind: "stars", val: "44821" },
    ]);
  } finally {
    await disposeHostFixture(fixture);
  }
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
    assert.notEqual(cacheAfterSecond[0]?.full_digest, cacheAfterSecond[1]?.full_digest);
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
