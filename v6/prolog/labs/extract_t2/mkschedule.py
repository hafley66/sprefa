#!/usr/bin/env python3
"""mkschedule.py 'REL:VALUE[,VALUE...]' ... > schedule.json

Build a ONE-TICK arrival schedule whose rows may carry whole JSON documents.

A VALUE is either

    @path/to/doc.json   the file's contents, as a json column value
    anything else       a text literal
    an integer literal  an int column value

Every arrival lands in ONE tick, so a program joining several documents sees
all of them at its first derivation.

    mkschedule.py 'spec:@corpus/openapi-petstore.json'
    mkschedule.py 'spec:pet-contracts,@corpus/xrepo/pet-contracts/openapi.json' \
                  'manifest:pet-contracts,@corpus/xrepo/pet-contracts/package.json'

THE JSON COLUMN CONTRACT, and why it is not a choice. The value written for a
json column is a JSON STRING holding the CANONICAL document text -- sorted keys,
no whitespace -- which is exactly what compile/sweep.pl:arrival_value_json/4
writes for a json column, and the reason that file gives applies unchanged:
json1's json() minifies but PRESERVES key order, so nothing downstream will
canonicalize on our behalf and the bytes have to be right on the way in or the
two doors cannot agree.
"""
import json
import sys


def parse_value(raw: str):
    if raw.startswith("@"):
        with open(raw[1:], encoding="utf-8") as handle:
            document = json.load(handle)
        return json.dumps(document, sort_keys=True, separators=(",", ":"))
    try:
        return int(raw)
    except ValueError:
        return raw


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    arrivals = []
    for spec in argv[1:]:
        rel, _, values = spec.partition(":")
        if not rel or not values:
            print(f"mkschedule: bad spec {spec!r}", file=sys.stderr)
            return 2
        arrivals.append(
            {"rel": rel, "sign": "add", "row": [parse_value(v) for v in values.split(",")]}
        )
    json.dump([arrivals], sys.stdout, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
