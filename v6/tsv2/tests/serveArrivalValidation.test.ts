/**
 * serveArrivalValidation.test.ts — the HTTP arrival boundary checks row SHAPE,
 * not only row LENGTH.
 *
 * THE DEFECT (review finding 2, plans/2026-07-30-ts-lowering-review.md).
 * `batchProblem` checked three things: rel is an arrival target, sign is
 * add/del, and `arrival.row.length` equals the declared width. It never checked
 * that the parsed body was an object, that `batch` was an array, that an
 * arrival was an object, that `row` was an array, or that a row's elements were
 * `IRowValue`. Five measured payloads, all reaching the engine:
 *
 *   row: "ab" on a 2-column rel   500, and the SERVER DIED (finding 1's trigger)
 *   row: [{a:1},[2,3]]            200, tick log {"deltas":{}}, and the row is in
 *                                 the table as [null,"[2,3]"] -- a NULL in a
 *                                 TEXT NOT NULL column, which then pollutes
 *                                 every later read
 *   batch: 5                      500 "batch is not iterable"
 *   body {{{                      500 (raw JSON.parse text leaked to the client)
 *   batch: [null]                 500 "Cannot read properties of null"
 *
 * The second is the worst in kind: the tick log is the cross-target grading
 * contract, and a path that STORES a row while printing an empty delta line
 * breaks that contract in silence.
 *
 * WHERE THE CHECK BELONGS, and why: the HAND-WRITTEN BOUNDARY, not emitted
 * per-rel code.
 *   1. It is the only trust boundary. The other two arrival producers (bind
 *      timers, host responses) build rows in typed code inside this process;
 *      only http accepts arbitrary bytes.
 *   2. It is the only place that can name the mistake. The emitted
 *      `validateArrivals` sees one value with an index; the boundary holds
 *      `relColumns` and `relColumnTypes` and can answer 400 with the rel name,
 *      the column name and the offending type.
 *   3. Emitted validation runs INSIDE the tick, after writes have begun. A
 *      rejection there is a partially-applied tick and a 500, never a clean 400.
 *   4. The emitter is prolog and `v6/prolog/compile/*` belongs to another lane
 *      at this sha.
 * The emitted `validateArrivals` stays what it is: a per-value COERCION pass
 * (bool to 0/1, -0 to 0) with the bool/float refusals it already had. This file
 * mirrors those two refusals at the boundary so they answer 400 instead of 500,
 * and adds the structural and int checks it never had. Recorded for the emitter
 * lane: text/int/ref columns still fall through `validateArrivals` with a bare
 * `return value`, which is fine now that nothing untrusted reaches it.
 *
 * WHAT THE BOUNDARY DELIBERATELY DOES NOT CHECK. A `ref` column takes any JSON
 * value, because under struct-as-rows a struct arrives whole as an object.
 * Whether that object matches the declared struct SHAPE is the type graph's
 * question; the engine already answers it by name
 * ("type_arrival_shape_mismatch: not_an_object(plot, null)"), one layer down.
 *
 * RED FIRST, verbatim, at 4d438db2 before the fix (`node --test
 * --experimental-transform-types tests/serveArrivalValidation.test.ts`). This
 * was run against an earlier 2-column form of PROGRAM, before the `ref` column
 * was added; the probe bodies are otherwise the same:
 *
 *   ✖ a row that is not an array is a 400 naming what is wrong
 *     AssertionError: a row that is not an array is a client error:
 *     {"error":"arrival.row.map is not a function"}
 *     500 !== 400
 *   ✖ row elements that are not scalars is a 400 naming what is wrong
 *     AssertionError: row elements that are not scalars is a client error:
 *     {"ticks":[{"tick":1,"line":"{\"tick\":1,\"deltas\":{}}"}]}
 *     200 !== 400
 *   ✖ a batch that is not an array is a 400 naming what is wrong
 *     AssertionError: a batch that is not an array is a client error:
 *     {"error":"batch is not iterable"}
 *     500 !== 400
 *   ✖ a body that is not JSON is a 400 naming what is wrong
 *     AssertionError: a body that is not JSON is a client error:
 *     {"error":"Expected property name or '}' in JSON at position 1 (line 1 column 2)"}
 *     500 !== 400
 *   ✖ a null arrival is a 400 naming what is wrong
 *     AssertionError: a null arrival is a client error:
 *     {"error":"Cannot read properties of null (reading 'rel')"}
 *     500 !== 400
 *   ✖ a value of the wrong declared type is a 400 naming what is wrong
 *     AssertionError: a value of the wrong declared type is a client error:
 *     {"ticks":[{"tick":1,"line":"{\"tick\":1,\"deltas\":{\"echoed\":{\"add\":
 *     [[\"x\",\"seven\"]],\"del\":[]},\"note\":{\"add\":[[\"x\",\"seven\"]],
 *     \"del\":[]}}}"}]}
 *     200 !== 400
 *   ✖ a rejected row is never stored, and never printed as an empty delta
 *     AssertionError: {"ticks":[{"tick":1,"line":"{\"tick\":1,\"deltas\":{}}"}]}
 *     200 !== 400
 *   # fail 7 (of 8; only "well-formed arrivals are untouched" was already green)
 *
 * The sixth is worth reading twice: "seven" went into an INTEGER column,
 * derived THROUGH A RULE into `echoed`, and came back out of the tick log as a
 * string, with a 200.
 *
 * SABOTAGE RECEIPT, run after the fix and reverted. `columnProblem` made to
 * return null unconditionally (every other check kept). Five of ten flip,
 * verbatim:
 *
 *   ✖ an object in a text column is a 400 naming what is wrong
 *     AssertionError: an object in a text column is a client error:
 *     {"ticks":[{"tick":1,"line":"{\"tick\":1,\"deltas\":{\"plot\":{\"add\":
 *     [[1,2]],\"del\":[]}}}"}]}
 *   ✖ an array in a text column is a 400 naming what is wrong
 *     ... same shape, 200 and an empty `note` delta
 *   ✖ a null in a struct column is a 400 naming what is wrong
 *     AssertionError: a null in a struct column is a client error:
 *     {"error":"type_arrival_shape_mismatch: not_an_object(plot, null)"}
 *   ✖ a value of the wrong declared type is a 400 naming what is wrong
 *     ... 200, with the tick log carrying note add [["x","seven",{...}]] and
 *     echoed add [["x","seven"]]
 *   ✖ a rejected row is never stored, and never printed as an empty delta
 *     AssertionError: {"ticks":[{"tick":1,"line":"{\"tick\":1,\"deltas\":
 *     {\"plot\":{\"add\":[[1,2]],\"del\":[]}}}"}]}
 *
 * Two of those are worth reading closely. The last is the one that proves the
 * difference is not cosmetic: it asserts `GET /idb/note` holds zero rows, and
 * without the per-value check the bad row is stored. The third shows what the
 * engine does WITHOUT this boundary and is the reason the boundary exists: it
 * refuses the null, correctly, but as an in-tick fault and therefore a 500 that
 * blames the server for the client's malformed body.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { post_program, request, start_served } from "./serveHelpers.ts";

/**
 * Three columns, three kinds of column: a `text`, an `int`, and a `ref` to a
 * struct. `note` is the arrival target; `echoed` proves a bad row that got in
 * would PROPAGATE through a rule, not just sit in one table.
 *
 * The `ref` column is here on purpose. Under struct-as-rows a struct value
 * arrives whole as a JSON object, so a boundary check that demanded a scalar
 * everywhere would reject legitimate traffic. That is not hypothetical: the
 * first draft of `columnProblem` did exactly that and golden-flex went red on
 * its own `tree.site` (receipt in serve/4_http.ts). Both directions are
 * asserted below -- an object into `at` is a 200, an object into `name` is a
 * 400.
 */
const PROGRAM = [
  "rel plot(row: int, col: int).",
  "rel note(name: text, size: int, at: plot).",
  "rel echoed(name: text, size: int).",
  "",
  "echoed(Name, Size) <- note(Name, Size, _At).",
  "",
].join("\n");

/** One well-formed struct value for the `ref` column. */
const AT = { row: 1, col: 2 };

interface Probe {
  readonly what: string;
  readonly body: string;
  /** Every fragment the 400 must name. */
  readonly names: readonly string[];
}

const MALFORMED: readonly Probe[] = [
  {
    what: "a row that is not an array",
    body: '{"batch":[{"rel":"note","sign":"add","row":"ab"}]}',
    names: ["note", "row"],
  },
  {
    what: "an object in a text column",
    body: `{"batch":[{"rel":"note","sign":"add","row":[{"a":1},7,${JSON.stringify(AT)}]}]}`,
    names: ["note", "name"],
  },
  {
    what: "an array in a text column",
    body: `{"batch":[{"rel":"note","sign":"add","row":[[2,3],7,${JSON.stringify(AT)}]}]}`,
    names: ["note", "name"],
  },
  {
    what: "a null in a struct column",
    body: '{"batch":[{"rel":"note","sign":"add","row":["x",7,null]}]}',
    names: ["note", "at", "null"],
  },
  {
    what: "a batch that is not an array",
    body: '{"batch":5}',
    names: ["batch"],
  },
  {
    what: "a body that is not JSON",
    body: "{{{",
    names: ["json"],
  },
  {
    what: "a null arrival",
    body: '{"batch":[null]}',
    names: ["arrival"],
  },
  {
    what: "a value of the wrong declared type",
    body: `{"batch":[{"rel":"note","sign":"add","row":["x","seven",${JSON.stringify(AT)}]}]}`,
    names: ["note", "size", "int"],
  },
];

async function served_with_program(): Promise<Awaited<ReturnType<typeof start_served>>> {
  const served = await start_served();
  const loaded = await post_program(served.port, PROGRAM);
  assert.equal(loaded.statusCode, 200, loaded.body);
  return served;
}

for (const probe of MALFORMED) {
  test(`${probe.what} is a 400 naming what is wrong`, async () => {
    const served = await served_with_program();
    try {
      const answered = await request(served.port, "/edb/events", "POST", probe.body);
      assert.equal(answered.statusCode, 400, `${probe.what} is a client error: ${answered.body}`);
      const lowered = answered.body.toLowerCase();
      for (const name of probe.names) {
        assert.ok(lowered.includes(name.toLowerCase()), `the 400 must name '${name}': ${answered.body}`);
      }
      // A raw JS TypeError message leaking to the client is the old shape.
      assert.doesNotMatch(answered.body, /is not a function|is not iterable|Cannot read properties/);
    } finally {
      await served.stop();
    }
  });
}

function good_body(name: string, size: number): string {
  return `{"batch":[{"rel":"note","sign":"add","row":[${JSON.stringify(name)},${size},${JSON.stringify(AT)}]}]}`;
}

test("a rejected row is never stored, and never printed as an empty delta", async () => {
  const served = await served_with_program();
  try {
    const rejected = await request(
      served.port,
      "/edb/events",
      "POST",
      `{"batch":[{"rel":"note","sign":"add","row":[{"a":1},[2,3],${JSON.stringify(AT)}]}]}`,
    );
    assert.equal(rejected.statusCode, 400, rejected.body);

    // The measured corruption: [null,"[2,3]"] sitting in a TEXT NOT NULL column
    // after a 200 whose tick log said `{"deltas":{}}`.
    const stored = await request(served.port, "/idb/note", "GET");
    assert.equal(stored.statusCode, 200, stored.body);
    assert.equal(JSON.parse(stored.body).rows.length, 0, `a rejected row must not be stored: ${stored.body}`);

    // And the server is unpolluted for the next well-formed arrival.
    const good = await request(served.port, "/edb/events", "POST", good_body("x", 7));
    assert.equal(good.statusCode, 200, good.body);
    assert.deepEqual(JSON.parse((await request(served.port, "/idb/echoed", "GET")).body).rows, [["x", 7]]);
  } finally {
    await served.stop();
  }
});

test("well-formed arrivals are untouched by the new checks", async () => {
  const served = await served_with_program();
  try {
    // A STRUCT VALUE IN A REF COLUMN IS ORDINARY TRAFFIC, not a malformed row.
    assert.equal((await request(served.port, "/edb/events", "POST", good_body("x", 7))).statusCode, 200);
    assert.equal((await request(served.port, "/edb/events", "POST", good_body("y", 8))).statusCode, 200);
    const echoed = await request(served.port, "/idb/echoed", "GET");
    assert.deepEqual(JSON.parse(echoed.body).rows, [["x", 7], ["y", 8]], echoed.body);

    // The three checks that already existed still answer 400, unchanged.
    const wrong_width = await request(served.port, "/edb/events", "POST", '{"batch":[{"rel":"note","sign":"add","row":["x"]}]}');
    assert.equal(wrong_width.statusCode, 400, wrong_width.body);
    assert.match(wrong_width.body, /takes 3 columns, got 1/, wrong_width.body);

    const wrong_rel = await request(served.port, "/edb/events", "POST", '{"batch":[{"rel":"echoed","sign":"add","row":["x",7]}]}');
    assert.equal(wrong_rel.statusCode, 400, wrong_rel.body);
    assert.match(wrong_rel.body, /not an arrival target/, wrong_rel.body);

    const wrong_sign = await request(
      served.port,
      "/edb/events",
      "POST",
      `{"batch":[{"rel":"note","sign":"nope","row":["x",7,${JSON.stringify(AT)}]}]}`,
    );
    assert.equal(wrong_sign.statusCode, 400, wrong_sign.body);
    assert.match(wrong_sign.body, /sign must be add or del/, wrong_sign.body);

    // An empty batch is legal and ticks, as before.
    const empty = await request(served.port, "/edb/events", "POST", '{"batch":[]}');
    assert.equal(empty.statusCode, 200, empty.body);
  } finally {
    await served.stop();
  }
});
