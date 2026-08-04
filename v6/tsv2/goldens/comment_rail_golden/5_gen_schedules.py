#!/usr/bin/env python3
"""Writes the golden's two hermetic schedules.

1_schedule.json  the graded fixture: 12 staged files, 5 ticks.
5_schedule.scale.json  the same comment nodes with SCALE_FACTOR x the added
lines. run_end_candidate's cardinality must be identical across the two; that
equality is the linearity claim (added lines never enter the pairing join).
The per-line prose rows are fixed by the comment content, so they too are
identical across the two schedules.
"""

import json
import sys

SCALE_FACTOR = 20

# (path, blob, exempt): the graded fixture set.
FILES = [
    ("src/violation.ts", "blob-violation", 0),
    ("src/block.ts", "blob-block", 0),
    ("src/block3.ts", "blob-block3", 0),
    ("src/waived.ts", "blob-waived", 0),
    ("src/fake-waiver.ts", "blob-fake", 0),
    ("src/untouched.ts", "blob-untouched", 0),
    ("src/pair.ts", "blob-pair", 0),
    ("src/gapped.ts", "blob-gapped", 0),
    ("src/shebang2.ts", "blob-shebang2", 0),
    ("src/shebang3.ts", "blob-shebang3", 0),
    ("src/divider.ts", "blob-divider", 0),
    ("tests/exempt.test.ts", "blob-exempt", 1),
]

# NODES[path] = (line, end_line, kind) comment spans.
NODES = {
    "src/violation.ts": [(10, 10, "line"), (11, 11, "line"), (12, 12, "line")],
    "src/block.ts": [(20, 22, "block")],
    "src/block3.ts": [(10, 14, "block")],
    "src/waived.ts": [(5, 5, "line"), (6, 6, "line"), (7, 7, "line")],
    "src/fake-waiver.ts": [(2, 2, "line"), (3, 3, "line"), (4, 4, "line")],
    "src/untouched.ts": [(30, 30, "line"), (31, 31, "line"), (32, 32, "line"), (33, 33, "line")],
    "src/pair.ts": [(3, 3, "line"), (4, 4, "line")],
    "src/gapped.ts": [(1, 1, "line"), (2, 2, "line"), (5, 5, "line"), (6, 6, "line"), (7, 7, "line")],
    "src/shebang2.ts": [(1, 1, "line"), (2, 2, "line"), (3, 3, "line")],
    "src/shebang3.ts": [(1, 1, "line"), (2, 2, "line"), (3, 3, "line"), (4, 4, "line")],
    "src/divider.ts": [(10, 10, "line"), (11, 11, "line"), (12, 12, "line"), (13, 13, "line"), (14, 14, "line")],
}

# LINES[path] = (line, prose_flag, prose_seq), one per physical comment line.
# prose_seq advances only on prose lines; non-prose rows carry the previous
# prose seq (0 before any prose line).
LINES = {
    "src/violation.ts": [(10, 1, 1), (11, 1, 2), (12, 1, 3)],
    "src/block.ts": [(20, 0, 0), (21, 1, 1), (22, 0, 1)],
    "src/block3.ts": [(10, 0, 0), (11, 1, 1), (12, 1, 2), (13, 1, 3), (14, 0, 3)],
    "src/waived.ts": [(5, 1, 1), (6, 1, 2), (7, 1, 3)],
    "src/fake-waiver.ts": [(2, 1, 1), (3, 1, 2), (4, 1, 3)],
    "src/untouched.ts": [(30, 1, 1), (31, 1, 2), (32, 1, 3), (33, 1, 4)],
    "src/pair.ts": [(3, 1, 1), (4, 1, 2)],
    "src/gapped.ts": [(1, 1, 1), (2, 1, 2), (5, 1, 3), (6, 1, 4), (7, 1, 5)],
    "src/shebang2.ts": [(1, 0, 0), (2, 1, 1), (3, 1, 2)],
    "src/shebang3.ts": [(1, 0, 0), (2, 1, 1), (3, 1, 2), (4, 1, 3)],
    "src/divider.ts": [(10, 1, 1), (11, 1, 2), (12, 0, 2), (13, 1, 3), (14, 1, 4)],
}

ADDED = {
    "src/violation.ts": [10, 11, 12],
    "src/block.ts": [20, 21, 22],
    "src/block3.ts": [10, 11, 12, 13, 14],
    "src/waived.ts": [5, 6, 7],
    "src/fake-waiver.ts": [1, 2, 3, 4],
    "src/untouched.ts": [50],
    "src/pair.ts": [3, 4],
    "src/gapped.ts": [1, 2, 3, 4, 5, 6, 7, 8],
    "src/shebang2.ts": [1, 2, 3],
    "src/shebang3.ts": [1, 2, 3, 4],
    "src/divider.ts": [10, 11, 12, 13, 14],
}

MARKERS = {
    "src/waived.ts": [6],
    "src/fake-waiver.ts": [1],
}


def add(rel, row):
    return {"rel": rel, "sign": "add", "row": row}


def per_file(host, table, project):
    batch = []
    for path, blob, exempt in FILES:
        if exempt:
            continue
        witness = f"witness|{host}|path:text={path}|digest:text={blob}"
        for ordinal, item in enumerate(table.get(path, [])):
            batch.append(add(f"__host_response_{host}", [witness, ordinal, path, blob] + project(item)))
    return batch


def inflated_added():
    """Every original added line, plus SCALE_FACTOR-1 lines well past any node."""
    grown = {}
    for path, lines in ADDED.items():
        extra = [1000 + index for index in range((SCALE_FACTOR - 1) * len(lines))]
        grown[path] = lines + extra
    return grown


def schedule(added_table):
    staged_witness = "witness|staged_file_list|index_digest:text=stage1"
    return [
        [add("max_run", [2]), add("staged_probe", ["stage1"])],
        [
            add("__host_response_staged_file_list", [staged_witness, ordinal, "stage1", path, blob, exempt])
            for ordinal, (path, blob, exempt) in enumerate(FILES)
        ],
        per_file("added_line_span", added_table, lambda line: [line]),
        per_file("comment_fact", NODES, lambda node: [node[0], node[1], node[2]])
        + per_file("comment_line_fact", LINES, lambda line: [line[0], line[1], line[2]]),
        per_file("waiver_marker", MARKERS, lambda line: [line]),
    ]


def main():
    if len(sys.argv) != 3:
        print("usage: 5_gen_schedules.py <schedule.json> <schedule.scale.json>", file=sys.stderr)
        return 2
    with open(sys.argv[1], "w", encoding="utf-8") as handle:
        json.dump(schedule(ADDED), handle, indent=2)
        handle.write("\n")
    with open(sys.argv[2], "w", encoding="utf-8") as handle:
        json.dump(schedule(inflated_added()), handle, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
