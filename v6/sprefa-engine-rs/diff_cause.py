"""Rewrite the cause column of every `diff` row into a structural category.

The raw first-differing LINE is unique per fixture, so a ledger keyed on it
shows 171 problems where there are three. Each diff row becomes
`<category> first-tick=<n>`, and grade.sh's per-verdict `uniq -c` then prints
the histogram.

Usage: diff_cause.py <corpus-dir> <output-dir> <verdicts.tsv>
"""

import json
import os
import sys


def read_lines(path):
    if not os.path.exists(path):
        return []
    with open(path) as handle:
        return [line for line in handle.read().splitlines() if line.strip()]


def categories(oracle, rust):
    found = set()
    if len(rust) < len(oracle):
        found.add("missing-tick")
    if len(rust) > len(oracle):
        found.add("extra-tick")
    first_tick = None
    for index in range(min(len(oracle), len(rust))):
        if oracle[index] == rust[index]:
            continue
        if first_tick is None:
            first_tick = index + 1
        expected = json.loads(oracle[index])
        actual = json.loads(rust[index])
        # Equal as JSON, different as bytes: the number rendering, not the row.
        if expected == actual:
            found.add("number-text")
            continue
        expected_deltas = expected.get("deltas", {})
        actual_deltas = actual.get("deltas", {})
        for rel in expected_deltas:
            if rel not in actual_deltas:
                found.add("missing-rel")
            elif expected_deltas[rel] != actual_deltas[rel]:
                found.add("wrong-row")
        for rel in actual_deltas:
            if rel not in expected_deltas:
                found.add("extra-rel")
    if first_tick is None:
        first_tick = min(len(oracle), len(rust)) + 1
    return found, first_tick


def cause(corpus, output, name):
    oracle = read_lines(os.path.join(corpus, name + ".oracle.jsonl"))
    rust = read_lines(os.path.join(output, name + ".out"))
    found, first_tick = categories(oracle, rust)
    if not found:
        return "identical after normalization"
    category = sorted(found)[0] if len(found) == 1 else "mixed(" + "+".join(sorted(found)) + ")"
    return "{} first-tick={}".format(category, first_tick)


def main():
    corpus, output, verdicts = sys.argv[1], sys.argv[2], sys.argv[3]
    with open(verdicts) as handle:
        rows = [line.split("\t") for line in handle.read().splitlines() if line.strip()]
    with open(verdicts, "w") as handle:
        for row in rows:
            name, verdict = row[0], row[1]
            reason = row[2] if len(row) > 2 else ""
            if verdict == "diff":
                reason = cause(corpus, output, name)
            handle.write("{}\t{}\t{}\n".format(name, verdict, reason))


main()
