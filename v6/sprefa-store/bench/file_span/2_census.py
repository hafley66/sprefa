#!/usr/bin/env python3
"""Count real extractor references per distinct file-relative span."""

from __future__ import annotations

import argparse
import collections
import json
import subprocess
from pathlib import Path

SPAN_FIELDS = ("span", "from", "to", "call", "arg", "owner")
EXTENSIONS = {
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".rs",
    ".go",
    ".kt",
    ".kts",
    ".pl",
    ".pro",
    ".prolog",
    ".datalog",
    ".horn",
}


def tracked_sources(root, limit):
    paths = subprocess.check_output(
        ["git", "ls-files"], cwd=root, text=True
    ).splitlines()
    selected = [path for path in paths if Path(path).suffix in EXTENSIONS]
    return selected[:limit] if limit else selected


def spans_in_fact(fact):
    for field in SPAN_FIELDS:
        value = fact.get(field)
        if isinstance(value, dict) and "start" in value and "end" in value:
            yield (value["start"], value["end"])


def main(args):
    root = Path(args.root).resolve()
    extract = Path(args.extract).resolve()
    per_span = collections.Counter()
    records = collections.Counter()
    files_with_facts = 0
    failures = []
    for relative in tracked_sources(root, args.limit):
        completed = subprocess.run(
            [extract, root / relative],
            text=True,
            capture_output=True,
        )
        if completed.returncode:
            failures.append((relative, completed.stderr.strip()))
            continue
        local = collections.Counter()
        for line in completed.stdout.splitlines():
            fact = json.loads(line)
            records[fact["record"]] += 1
            for span in spans_in_fact(fact):
                local[span] += 1
        if local:
            files_with_facts += 1
        per_span.update(local.values())

    distinct = sum(per_span.values())
    references = sum(multiplicity * count for multiplicity, count in per_span.items())
    at_least = {
        str(threshold): sum(count for multiplicity, count in per_span.items() if multiplicity >= threshold)
        for threshold in (1, 2, 3, 4, 8, 16)
    }
    result = {
        "tracked_source_files": len(tracked_sources(root, args.limit)),
        "files_with_facts": files_with_facts,
        "failed_files": failures,
        "record_counts": dict(sorted(records.items())),
        "distinct_spans": distinct,
        "span_references": references,
        "references_per_distinct_span": references / distinct if distinct else 0,
        "multiplicity_histogram": {
            str(key): value for key, value in sorted(per_span.items())
        },
        "distinct_spans_at_least": at_least,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default="../..")
    parser.add_argument("--extract", default="../sprefa-extract/target/release/extract")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--output", default="bench/file_span/census-results.json")
    main(parser.parse_args())
