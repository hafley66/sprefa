#!/usr/bin/env python3
"""zone.py -- the marker/span helper the staged-writes lab's `sh` hosts call.

Policy-free, exactly like scripts/comment_node.py: it projects a file into JSONL
rows and it splices a file when told to. Every decision about WHAT to write is
made by the .dl6 rules; this script only knows how to read a marker pair and how
to put bytes at an offset.

The marker dialect is the one hafley-tsp's ReplaceFile.tsx uses and the one v5's
`gen(:zone, ...)` resolves (src/engine/gen.rs find_zone): a `BEGIN: <name>` line
through the next `END:` line, any comment prefix, markers stay, content strictly
between them is the owned region.

Subcommands
  body PATH       one row per line currently inside each zone
                  {zone, slot, line_text}
  pair PATH       one row per zone: the marker pair's line numbers and byte
                  offsets of the owned region  {zone, begin_line, end_line,
                  start, end}
  fns PATH        the GENERATOR under test: one row per `fn name(` in the file,
                  the body the zone is supposed to hold
                  {slot, line_text}
  put PATH ZONE ORDINAL TEXT
                  write ONE line into a zone at ORDINAL (grow the zone as
                  needed). Prints {wrote: 1}. This is the per-line apply arm.
  splice PATH START END TEXT
                  replace bytes [START, END) with TEXT. Prints {wrote: 1}.
                  This is the span-addressed apply arm.
  ordinals PATH   two rows whose column is literally named `ordinal`
  append PATH TEXT
                  append one line. Prints {wrote: 1}. Deliberately NOT
                  idempotent -- the crash receipt needs a write that shows its
                  own replay.
"""

import json
import re
import sys

BEGIN = re.compile(r"BEGIN:\s*([A-Za-z0-9_.\-]+)")
END = re.compile(r"END:")
FN = re.compile(r"^\s*(?:pub\s+)?fn\s+([A-Za-z0-9_]+)")


def emit(row):
    print(json.dumps(row, separators=(",", ":")))


def read_lines(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read().split("\n")


def zones(lines):
    """(name, begin_index, end_index) per marker pair, 0-based line indices."""
    found = []
    open_at = None
    name = None
    for index, text in enumerate(lines):
        if open_at is None:
            match = BEGIN.search(text)
            if match:
                open_at, name = index, match.group(1)
        elif END.search(text):
            found.append((name, open_at, index))
            open_at, name = None, None
    return found


def line_offsets(path):
    with open(path, "rb") as handle:
        data = handle.read()
    starts = [0]
    for index, byte in enumerate(data):
        if byte == 0x0A:
            starts.append(index + 1)
    return data, starts


def cmd_body(path):
    lines = read_lines(path)
    for name, begin, end in zones(lines):
        for ordinal, text in enumerate(lines[begin + 1:end]):
            emit({"zone": name, "slot": ordinal, "line_text": text})


def cmd_pair(path):
    lines = read_lines(path)
    data, starts = line_offsets(path)
    for name, begin, end in zones(lines):
        # the owned region is [start of line begin+1, start of line end)
        start = starts[begin + 1] if begin + 1 < len(starts) else len(data)
        stop = starts[end] if end < len(starts) else len(data)
        emit({
            "zone": name,
            "begin_line": begin + 1,
            "end_line": end + 1,
            "start": start,
            "end": stop,
        })


def cmd_fns(path):
    ordinal = 0
    for text in read_lines(path):
        match = FN.match(text)
        if match:
            emit({"slot": ordinal, "line_text": "// fn " + match.group(1)})
            ordinal += 1


def cmd_put(path, zone, ordinal, text):
    ordinal = int(ordinal)
    lines = read_lines(path)
    for name, begin, end in zones(lines):
        if name != zone:
            continue
        body = lines[begin + 1:end]
        while len(body) <= ordinal:
            body.append("")
        body[ordinal] = text
        lines[begin + 1:end] = body
        with open(path, "w", encoding="utf-8") as handle:
            handle.write("\n".join(lines))
        emit({"wrote": 1})
        return 0
    print(f"zone {zone} not found in {path}", file=sys.stderr)
    return 1


def cmd_splice(path, start, end, text):
    start, end = int(start), int(end)
    with open(path, "rb") as handle:
        data = handle.read()
    payload = (text + "\n").encode("utf-8") if text else b""
    with open(path, "wb") as handle:
        handle.write(data[:start] + payload + data[end:])
    emit({"wrote": 1})
    return 0


def cmd_ordinals(_path):
    """Two rows carrying an OUTPUT column literally named `ordinal`, with values
    nothing else in the system could produce. The receipt reads them back."""
    emit({"ordinal": 7, "payload": "seven"})
    emit({"ordinal": 8, "payload": "eight"})


def cmd_append(path, text):
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(text + "\n")
    emit({"wrote": 1})
    return 0


def main(argv):
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    verb, path = argv[1], argv[2]
    rest = argv[3:]
    if verb == "body":
        cmd_body(path)
        return 0
    if verb == "pair":
        cmd_pair(path)
        return 0
    if verb == "fns":
        cmd_fns(path)
        return 0
    if verb == "put":
        return cmd_put(path, *rest)
    if verb == "splice":
        return cmd_splice(path, *rest)
    if verb == "ordinals":
        cmd_ordinals(path)
        return 0
    if verb == "append":
        return cmd_append(path, *rest)
    print(f"unknown verb {verb}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
