#!/usr/bin/env python3
"""The gate's non-diff legs: semantics, laziness, cardinality, query plans.

usage: 7_assert.py <emitted.jsonl> <emitted.scale.jsonl> <generated.ts>
"""

import json
import re
import sqlite3
import sys

FAILURES = []


def check(name, actual, want):
    if actual == want:
        print(f"PASS  {name}: {actual}")
    else:
        FAILURES.append(f"{name}: got {actual!r}, want {want!r}")
        print(f"FAIL  {name}: got {actual!r}, want {want!r}")


def read_run(path):
    ticks, final = [], {}
    with open(path, "r", encoding="utf-8") as handle:
        for line in handle:
            row = json.loads(line)
            if "final" in row:
                final = row["final"]
            else:
                ticks.append(row)
    return ticks, final


def rows(final, rel):
    return final.get(rel, [])


def count(final, rel):
    return len(rows(final, rel))


def demand_paths(ticks, rel):
    seen = set()
    for tick in ticks:
        delta = tick["deltas"].get(rel)
        if delta:
            for row in delta["add"]:
                seen.add(row[2])
    return seen


def semantics(ticks, final):
    check(
        "violation_run",
        sorted(rows(final, "violation_run")),
        [
            ["src/block.ts", 20, 22, 3],
            ["src/fake-waiver.ts", 2, 4, 3],
            ["src/gapped.ts", 5, 7, 3],
            ["src/violation.ts", 10, 12, 3],
        ],
    )
    check("violation_total", rows(final, "violation_total"), [[4]])

    # One node spanning three lines is a three-line run with nothing to coalesce.
    check("block_is_one_node", count(final, "comment_node") and
          [row for row in rows(final, "comment_node") if row[0] == "src/block.ts"],
          [["src/block.ts", 20, 22, "block"]])

    # A gap splits one node list into two runs, and min/1 pairs each start with
    # its NEAREST end rather than the file's last.
    check(
        "gapped_runs",
        sorted(row for row in rows(final, "run_extent") if row[0] == "src/gapped.ts"),
        [["src/gapped.ts", 1, 2], ["src/gapped.ts", 5, 7]],
    )

    # The diff intersection: a long run the staged diff never touched is not one.
    check(
        "untouched_long_run_is_not_a_violation",
        (
            [row for row in rows(final, "long_run") if row[0] == "src/untouched.ts"],
            [row for row in rows(final, "touched_run") if row[0] == "src/untouched.ts"],
        ),
        ([["src/untouched.ts", 30, 33, 4]], []),
    )

    # The grammar gate: src/fake-waiver.ts has a marker hit on line 1 and no
    # comment node covering it, so nothing is waived.
    check("fake_marker_hit", rows(final, "marker_hit").count(["src/fake-waiver.ts", 1]), 1)
    check(
        "fake_marker_waives_nothing",
        [row for row in rows(final, "waiver_in_comment") if row[0] == "src/fake-waiver.ts"],
        [],
    )
    check("waived_run", rows(final, "waived_run"), [["src/waived.ts", 5, 7]])

    # Tick-visible: the violation exists until the marker lands, then retracts.
    tick4 = ticks[3]["deltas"].get("violation_run", {"add": [], "del": []})
    tick5 = ticks[4]["deltas"].get("violation_run", {"add": [], "del": []})
    check("waiver_admits_at_tick4", ["src/waived.ts", 5, 7, 3] in tick4["add"], True)
    check("waiver_retracts_at_tick5", tick5["del"], [["src/waived.ts", 5, 7, 3]])

    # Laziness: the exempt path never raises a demand row, so no extractor runs.
    for host in ("comment_fact", "added_line_span", "waiver_marker"):
        paths = demand_paths(ticks, f"__host_demand_{host}")
        check(f"exempt_raises_no_{host}_demand", "tests/exempt.test.ts" in paths, False)
        check(f"{host}_demands", len(paths), 7)


def cardinality(final, scale_final):
    """The linearity claim, as a COUNT test rather than end-state equality.

    The scale schedule keeps every comment node and multiplies the added lines
    by 20. Boundary and pairing cardinalities must be IDENTICAL; only
    added_line may grow.
    """
    base_added = count(final, "added_line")
    scale_added = count(scale_final, "added_line")
    check("added_line grew", (base_added, scale_added), (24, 480))
    for rel in ("comment_node", "run_start", "run_end", "run_end_candidate",
                "run_extent", "long_run", "touched_run", "violation_run"):
        check(f"{rel} flat under 20x added lines", count(scale_final, rel), count(final, rel))
    # The pairing join is 2 boundary rows per run, never a line row: 8 runs over
    # 7 files, so start x end within a file stays at this exact number.
    check("run_end_candidate cardinality", count(final, "run_end_candidate"), 9)
    check("run_start cardinality", count(final, "run_start"), 8)


def query_plans(generated):
    """EXPLAIN QUERY PLAN over the emitted DDL: every join that could scan a
    line table must be an index SEARCH instead."""
    source = open(generated, "r", encoding="utf-8").read()
    ddl = re.findall(r'`(CREATE (?:TEMP )?TABLE [^`]*)`', source)
    ddl += re.findall(r'`(CREATE INDEX [^`]*)`', source)
    statements = {}
    for rel, sql in re.findall(r'\{ rel: "([a-z_]+)", sql: `(INSERT[^`]*)`', source):
        statements.setdefault(rel, sql)

    connection = sqlite3.connect(":memory:")
    for statement in ddl:
        connection.execute(statement.replace("CREATE TEMP TABLE", "CREATE TABLE"))

    # The inner side of every range join must be an index SEARCH with the range
    # pushed into the key, and there must be exactly one SCAN (the driver).
    wanted = {
        "run_end_candidate":
            "SCAN b0 | SEARCH b1 USING PRIMARY KEY (file_path=? AND end_line>?)",
        "touched_run":
            "SCAN b0 | SEARCH b1 USING PRIMARY KEY "
            "(file_path=? AND line_number>? AND line_number<?)",
        "waived_run":
            "SCAN b0 | SEARCH b1 USING PRIMARY KEY "
            "(file_path=? AND marker_line>? AND marker_line<?)",
        "waiver_in_comment":
            "SCAN b1 | SEARCH b0 USING PRIMARY KEY "
            "(file_path=? AND marker_line>? AND marker_line<?)",
    }
    for rel, want_plan in wanted.items():
        sql = statements.get(rel)
        if sql is None:
            FAILURES.append(f"{rel}: no INSERT statement found in the emitted module")
            print(f"FAIL  {rel}: no INSERT statement found")
            continue
        plan = " | ".join(str(row[3]) for row in connection.execute(f"EXPLAIN QUERY PLAN {sql}"))
        check(f"{rel} query plan", plan, want_plan)
    connection.close()


def main():
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    ticks, final = read_run(sys.argv[1])
    _, scale_final = read_run(sys.argv[2])
    semantics(ticks, final)
    cardinality(final, scale_final)
    query_plans(sys.argv[3])
    if FAILURES:
        print(f"comment rail golden: {len(FAILURES)} assertion(s) failed", file=sys.stderr)
        return 1
    print("COMMENT_RAIL_ASSERTIONS HOLD")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
