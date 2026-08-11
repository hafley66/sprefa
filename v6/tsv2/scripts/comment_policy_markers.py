#!/usr/bin/env python3
"""Marker projections used by comment rails after grammar extraction."""

import json
import re
import sys


def emit(row):
    print(json.dumps(row, separators=(",", ":")))


def lines(path):
    with open(path, encoding="utf-8") as handle:
        yield from enumerate(handle, 1)


def readme(path):
    marker = re.compile(r"README\(([a-z-]+)\):\s*(.*)")
    for line, text in lines(path):
        match = marker.search(text)
        if match:
            emit({"line": line, "anchor": match.group(1), "prose": match.group(2).rstrip("\n")})


def readme_row(path):
    marker = re.compile(r"README-ROW\(([a-z-]+)\)")
    for _, text in lines(path):
        match = marker.search(text)
        if match:
            emit({"anchor": match.group(1)})


def junction(path):
    marker = re.compile(r"LANG-JUNCTION\(([a-z0-9-]+)\):\s*(.*)")
    for line, text in lines(path):
        match = marker.search(text)
        if match:
            emit({"line": line, "slug": match.group(1), "meaning": match.group(2).rstrip("\n")})


def registry(path):
    marker = re.compile(r"LANG-REGISTRY\(([a-z0-9-]+)\)")
    for _, text in lines(path):
        match = marker.search(text)
        if match:
            emit({"slug": match.group(1)})


def zone(path):
    marker = re.compile(r"BEGIN: gen ([A-Za-z0-9_-]+)")
    start = None
    name = None
    for line, text in lines(path):
        match = marker.search(text)
        if match:
            start = line
            name = match.group(1)
        elif start is not None and "END:" in text:
            emit({"start": start, "end_line": line, "name": name})
            start = None
            name = None


# No `docs:` marker (user verdict 2026-08-10): the declaration line is the
# join key, reference-docs-rail.dl6 attaches the comment run ending above it.
SURFACE_ROW = re.compile(
    r"^surface\(\s*(?:'([^']*)'|([a-zA-Z_]\w*))\s*/\s*(variadic|\d+)\s*,\s*"
    r"([a-zA-Z_]\w*)\s*,"
)
SURFACE_STATUS = re.compile(r",\s*(live|reserved|refused)\)\.\s*$")
DL6_DECL_ROW = re.compile(r"^(?:rel|sh|bind)\s+([a-z_][a-z0-9_]*)")


def construct(path):
    for line, text in lines(path):
        head = SURFACE_ROW.search(text)
        if not head:
            continue
        status = SURFACE_STATUS.search(text)
        if not status:
            continue
        name = head.group(1) if head.group(1) is not None else head.group(2)
        emit({
            "line": line,
            "name": name,
            "arity": head.group(3),
            "axis": head.group(4),
            "status": status.group(1),
        })


# Presence-only decl test (construct/1 carries the rich row for registry.pl).
def decl_line(path):
    is_prolog = path.endswith(".pl") or path.endswith(".pro") or path.endswith(".prolog")
    for line, text in lines(path):
        match = SURFACE_ROW.search(text) if is_prolog else DL6_DECL_ROW.search(text)
        if match:
            emit({"line": line})


MODES = {
    "readme": readme,
    "readme-row": readme_row,
    "junction": junction,
    "registry": registry,
    "zone": zone,
    "construct": construct,
    "decl-line": decl_line,
}

if len(sys.argv) != 3 or sys.argv[1] not in MODES:
    raise SystemExit("usage: comment_policy_markers.py MODE PATH")
MODES[sys.argv[1]](sys.argv[2])
