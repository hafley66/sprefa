#!/usr/bin/env python3
"""Render violation_run rows in the bash tool's exact hook contract.

Exit 0 clean, 2 with findings, matching claude-research/bin/comment-prod --hook
line for line so the two rails are byte-comparable on stderr.
"""

import json
import sys


def rows_from(path):
    with open(path, "r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if isinstance(payload, dict):
        return payload.get("rows", [])
    return payload


def cell(row, name, index):
    if isinstance(row, dict):
        return row[name]
    return row[index]


def main():
    if len(sys.argv) != 3:
        print("usage: comment_budget_report.py <violation_run.json> <max_run>", file=sys.stderr)
        return 1
    rows = rows_from(sys.argv[1])
    max_run = sys.argv[2]
    if not rows:
        return 0

    findings = sorted(
        (
            str(cell(row, "file_path", 0)),
            int(cell(row, "start_line", 1)),
            int(cell(row, "end_line", 2)),
            int(cell(row, "comment_line_count", 3)),
        )
        for row in rows
    )
    print(
        f"COMMENT BUDGET VIOLATION (max {max_run} consecutive comment lines in new code):",
        file=sys.stderr,
    )
    for file_path, start_line, end_line, count in findings:
        print(f"{file_path}:{start_line}-{end_line} ({count} comment lines)", file=sys.stderr)
    print("Repo law: comments state only constraints the code cannot show.", file=sys.stderr)
    print(
        f"Fix: delete the narrative, keep at most {max_run} lines, or carry "
        "'@comment-ok: <reason>' if a scanner-backed waiver truly applies.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
