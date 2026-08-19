/**
 * hostDecode.test.ts — the sh host's JSON-object decode is a NAMED PROJECTION
 * (golden plan phase 2, the seam that lets `sprefa-extract`'s heterogeneous
 * JSONL feed a rel without a `decode/2` lowering).
 *
 * Graded through a REAL host: the program's template is a hermetic `printf`
 * that reproduces the extractor's own interleaved record shapes, so what is
 * measured is the shipped decode path and not a unit-test copy of it. The tick
 * log is then diffed byte-for-byte against the oracle fed the very rows the
 * server pushed (the runtime bridge's total-replay grading).
 *
 * SABOTAGE RECEIPT (run 2026-07-29, reverted): deleting the
 * `carriesEveryColumn` filter in serve/1_hosts.ts (`decodeObjectItems`) makes
 * the two `record=node` lines land as rows with an empty `callee`, and this
 * test fails with 4 picked rows instead of 2 -- plus the oracle diff, since
 * those rows are in the tick log too. Restoring the OLD positional fallback
 * (`Object.values(item)[index]`) is worse and also caught here: the node lines
 * then land carrying their `kind` value ("function"/"lambda") in the callee
 * column, which is failure class 36's cross-contamination in a new costume.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { firstValueFrom } from "rxjs";

import { ProcessAdapters } from "../serve/1_hosts.ts";
import { log_of_ticks, oracle_log, post_arrivals, post_program, request, schedule_from_ticks, start_served, tick_events } from "./serveHelpers.ts";

const JSON_PROJECTION_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-json-projection.dl6", import.meta.url));

async function wait_until(predicate: () => boolean | Promise<boolean>, what: string, timeout_ms = 10_000): Promise<void> {
  const deadline = Date.now() + timeout_ms;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("sh host: a JSON object stream projects by NAME, and a record missing a declared column is no row", async () => {
  const source = readFileSync(JSON_PROJECTION_DL6, "utf8");
  const served = await start_served();
  try {
    const loaded = await post_program(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const plans = JSON.parse(loaded.body) as { readonly arrival_targets: readonly string[] };

    await post_arrivals(served.port, [{ rel: "seed", sign: "add", row: ["alpha"] }]);
    await wait_until(async () => {
      const reply = await request(served.port, "/idb/picked", "GET");
      return (JSON.parse(reply.body) as { rows: unknown[] }).rows.length > 0;
    }, "the host's projected rows to land in picked");

    const picked = JSON.parse((await request(served.port, "/idb/picked", "GET")).body) as {
      rows: readonly (readonly (string | number)[])[];
    };
    // FOUR JSON lines out, TWO rows in: the two `record=node` lines carry no
    // `callee` at all, so they are not rows of this rel.
    assert.equal(picked.rows.length, 2, `expected the two site records only, got ${JSON.stringify(picked.rows)}`);
    assert.deepEqual(
      picked.rows.map((row) => row[1]).sort(),
      ["alpha_one", "alpha_two"],
      "each row carries the site record's own callee, never a neighbouring field",
    );

    const outcomes = tick_events(served.events);
    const replayed = schedule_from_ticks(outcomes, plans.arrival_targets);
    assert.equal(log_of_ticks(outcomes), oracle_log(source, replayed));
  } finally {
    await served.stop();
  }
});

/**
 * A GRID host: N answers, each one line carrying exactly the declared column
 * count as whitespace-separated fields. This is the shipped `files_at`
 * shape verbatim (`printf '%s %s\n' "$entry" "$oid"` per tracked path), reduced
 * to a hermetic `seq` so the cardinality is the demand row.
 */
const GRID_HOST_DL6 = `
rel want(count: text).
rel listing(count: text, path: text, digest: text).

sh files(count: text) -> (path: text, digest: text) =
  \`seq 1 {count} | while IFS= read -r idx; do printf '%s %s\\n' "file$idx.txt" "oid$idx"; done\`.

listing(count, path, digest) <- want(count), files(count, path, digest).
`;

/**
 * D1: the line-per-column reading is a GUESS, and it used to win whenever the
 * line count happened to equal the column count. `files_at` declares two
 * output columns, so a two-file answer -- and only a two-file answer -- was
 * folded into ONE row whose `path` was the whole first line and whose `digest`
 * was the whole second. Correct at one file, correct at three, silently wrong
 * at two, on a shipped host feeding the flagship receipt.
 *
 * RED-FIRST RECEIPT (run at a4629623, before the parseWhitespace fix):
 *
 *   AssertionError [ERR_ASSERTION]: 2 files must be 2 rows, not one row of two
 *   lines: [["2","file1.txt oid1","file2.txt oid2"]]
 *     + actual - expected
 *     + 1
 *     - 2
 *
 * The mangled row is printed by the assertion itself: `path` held
 * "file1.txt oid1", the entire first line, and `digest` held the second.
 * Cardinalities 0, 1 and 3 were already green, which is the whole reason this
 * shipped -- the receipt pins all four so a future heuristic cannot trade one
 * for another.
 *
 * SABOTAGE RECEIPT (run after the fix, reverted): restoring the old
 * `lines.length === outputs.length` precedence in `parseWhitespace` turns this
 * red again at count=2 only, exactly as above.
 *
 * NOTE ON GRADING, stated rather than hidden: replay grading (`scheduleFromTicks`
 * feeding the oracle the rows the server pushed) is BLIND to this defect by
 * construction -- the mangled row is replayed faithfully and both engines then
 * agree on it. The world-side decode is graded by the row assertions below and
 * nowhere else.
 */
test("sh host: a grid answer is one row per line at every cardinality, 0 through 3", async () => {
  const served = await start_served();
  try {
    const loaded = await post_program(served.port, GRID_HOST_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await post_arrivals(served.port, [
      { rel: "want", sign: "add", row: ["0"] },
      { rel: "want", sign: "add", row: ["1"] },
      { rel: "want", sign: "add", row: ["2"] },
      { rel: "want", sign: "add", row: ["3"] },
    ]);

    // Every demand has been ANSWERED once its own `effect` event is out,
    // including the zero-row one: waiting on `listing` alone could never
    // observe the count=0 host at all (no rows is its correct answer, and the
    // trace surface is where that answer is visible, by the design note in
    // 1_hosts.ts `decodeObjectItems`).
    const answers = (): readonly number[] =>
      served.events.flatMap((event) => (event.kind === "effect" && event.done.host === "files" ? [event.done.response_rows] : []));
    await wait_until(() => answers().length >= 4, "all four sh host demands to be answered");
    assert.deepEqual(
      answers().slice().sort((left, right) => left - right),
      [0, 1, 2, 3],
      `the decoded row count per demand, straight off the trace: ${JSON.stringify(answers())}`,
    );

    const listing = JSON.parse((await request(served.port, "/idb/listing", "GET")).body) as {
      rows: readonly (readonly string[])[];
    };
    const rows_for = (count: string): readonly (readonly string[])[] =>
      listing.rows.filter((row) => row[0] === count).slice().sort((left, right) => left[1]!.localeCompare(right[1]!));

    assert.deepEqual(rows_for("0"), [], "an empty answer is no rows");
    assert.deepEqual(rows_for("1"), [["1", "file1.txt", "oid1"]], "1 file must be 1 row");
    assert.deepEqual(
      rows_for("2"),
      [
        ["2", "file1.txt", "oid1"],
        ["2", "file2.txt", "oid2"],
      ],
      `2 files must be 2 rows, not one row of two lines: ${JSON.stringify(rows_for("2"))}`,
    );
    assert.deepEqual(
      rows_for("3"),
      [
        ["3", "file1.txt", "oid1"],
        ["3", "file2.txt", "oid2"],
        ["3", "file3.txt", "oid3"],
      ],
      "3 files must be 3 rows",
    );
    assert.equal(listing.rows.length, 6, "0 + 1 + 2 + 3 files");
  } finally {
    await served.stop();
  }
});

/**
 * The other side of the same guess, and why the fix is a PRECEDENCE change and
 * not a deletion: a host whose values carry internal whitespace prints one
 * VALUE per line (ghcacher's `printf '%s\n%s\n%s'`), and splitting such a line
 * into words shreds it. That reading survives; it just no longer outranks the
 * grid reading when both are available.
 */
const LINE_PER_COLUMN_DL6 = `
rel ask(tag: text).
rel meta(tag: text, title: text, author: text).

sh describe(tag: text) -> (title: text, author: text) =
  \`printf '%s\\n%s\\n' 'the {tag} report' 'ada lovelace'\`.

meta(tag, title, author) <- ask(tag), describe(tag, title, author).
`;

test("sh host: one value per line still wins when the lines are not a grid", async () => {
  const served = await start_served();
  try {
    const loaded = await post_program(served.port, LINE_PER_COLUMN_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);
    await post_arrivals(served.port, [{ rel: "ask", sign: "add", row: ["annual"] }]);
    await wait_until(async () => {
      const reply = await request(served.port, "/idb/meta", "GET");
      return (JSON.parse(reply.body) as { rows: unknown[] }).rows.length > 0;
    }, "the two-line answer to land in meta");

    const meta = JSON.parse((await request(served.port, "/idb/meta", "GET")).body) as {
      rows: readonly (readonly string[])[];
    };
    assert.deepEqual(meta.rows, [["annual", "the annual report", "ada lovelace"]]);
  } finally {
    await served.stop();
  }
});

test("the process adapter registry names extract and shell", () => {
  assert.ok(ProcessAdapters.has("sprefa_extract"));
  assert.ok(ProcessAdapters.has("shell"));
});
