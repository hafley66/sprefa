#!/usr/bin/env python3
"""parity_project.py -- project the v6 leg's JSONL onto the v5 query's exact
tab-separated column order, so a byte diff is the grade.

`comment` -> path, line, col, end_line, end_col, text, kind
             (v5 `? comment_node(path, line, col, end_line, end_col, text, kind)`)

`arch`    -> path, line, url
             (v5 `? arch_node(path, line, url)`), with THE GRAMMAR WITNESS
             applied here exactly as arch-rail.dl6's rule applies it: a marker
             hit survives only where the comment stream puts a comment node on
             the same (path, line). Doing it here and not in the shell is what
             keeps the rig's v6 leg the same computation the served program
             performs.
"""
import json
import sys


def rows(path):
    with open(path) as handle:
        for raw in handle:
            raw = raw.strip()
            if raw:
                yield json.loads(raw)


def emit(fields):
    print("\t".join(str(field) for field in fields))


def main():
    mode = sys.argv[1]
    if mode == "comment":
        for row in rows(sys.argv[2]):
            emit([row["path"], row["line"], row["col"], row["end_line"],
                  row["end_col"], row["comment_text"], row["kind"]])
        return 0
    if mode == "arch":
        witness = {(row["path"], row["line"]) for row in rows(sys.argv[3])}
        for row in rows(sys.argv[2]):
            if (row["path"], row["line"]) in witness:
                emit([row["path"], row["line"], row["url"]])
        return 0
    print("usage: parity_project.py comment|arch ...", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
