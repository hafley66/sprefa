#!/usr/bin/env python3
"""ARCH marker host projection; policy stays outside the extractor."""

import json
import re
import sys


MARKER = re.compile(r'ARCH\s*\{')
URL = re.compile(r'"url":"([^"]*)"')


def main(path):
    with open(path, encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not MARKER.search(line):
                continue
            match = URL.search(line)
            if match is None:
                continue
            url = match.group(1)
            parent, _, last = url.rpartition("/")
            print(json.dumps({
                "line": line_number,
                "url": url,
                "parent": parent,
                "last": last if parent else url,
            }, separators=(",", ":")))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: comment_arch_marker.py PATH")
    main(sys.argv[1])
