#!/usr/bin/env python3
"""Every `path:line` in a markdown file must resolve. Exit 1 when one does not.

A citation rots silently: the file is renamed, the block moves, the line count
shrinks, and the doc keeps asserting the old address. This is the rail that
catches it. Only `path:line` and `path:line-line` forms are checked; a bare
filename with no line is not an address and is skipped.
"""
import os
import re
import sys

CITE = re.compile(r'`([A-Za-z0-9_./-]+\.[A-Za-z0-9]+):(\d+)(?:-(\d+))?`')
PREFIXES = ("", "v6/prolog/", "v6/", "v6/prolog/compile/", ".github/",
            "v6/sprefa-extract/src/", "v6/prolog/conformance/")


def resolve(path):
    for prefix in PREFIXES:
        candidate = prefix + path
        if os.path.exists(candidate):
            return candidate
    return None


def check(doc):
    broken = []
    checked = 0
    for path, first, last in CITE.findall(open(doc, encoding="utf-8").read()):
        target = resolve(path)
        if target is None:
            broken.append((f"{path}:{first}", "no such file"))
            continue
        with open(target, encoding="utf-8", errors="ignore") as handle:
            lines = sum(1 for _ in handle)
        highest = int(last or first)
        if highest > lines:
            broken.append((f"{path}:{first}-{last or first}",
                           f"file has {lines} lines"))
            continue
        checked += 1
    return checked, broken


def main(argv):
    docs = argv[1:] or ["CLAUDE.md"]
    total_broken = 0
    for doc in docs:
        checked, broken = check(doc)
        for citation, why in broken:
            print(f"{doc}: BROKEN {citation} -> {why}")
        total_broken += len(broken)
        print(f"{doc}: {checked} citations resolved, {len(broken)} broken")
    return 1 if total_broken else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
