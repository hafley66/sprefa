#!/usr/bin/env python3
"""Both workspaces that build soopy must resolve its transitives identically.

`ignore` decides worktree membership (soopy `_4_worktree.rs`), so two
resolutions of it are two answers to "which files exist" from one library.

Reads the two Cargo.lock files directly. `cargo tree` would answer the same
question but REWRITES the lockfile it reads, which turns a read-only rail into
a mutation.
"""

import re
import sys
from pathlib import Path

ROOT = "soopy"
LOCKS = ("sprefa-extract/Cargo.lock", "sprefa-engine-rs/Cargo.lock")


def packages(lock_text):
    """name -> {version -> [dependency specs]}, one entry per resolved package."""
    by_name = {}
    for block in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "(.+)"$', block, re.M)
        version = re.search(r'^version = "(.+)"$', block, re.M)
        if not name or not version:
            continue
        deps = re.search(r"^dependencies = \[\n(.*?)^\]$", block, re.M | re.S)
        specs = re.findall(r'"(.+?)"', deps.group(1)) if deps else []
        by_name.setdefault(name.group(1), {})[version.group(1)] = specs
    return by_name


def closure(by_name):
    """Every (name, version) reachable from soopy, soopy included."""
    seen = set()
    pending = [(ROOT, version) for version in by_name.get(ROOT, {})]
    while pending:
        name, version = pending.pop()
        if (name, version) in seen:
            continue
        seen.add((name, version))
        for spec in by_name.get(name, {}).get(version, []):
            dep_name, _, dep_version = spec.partition(" ")
            versions = by_name.get(dep_name, {})
            for candidate in [dep_version] if dep_version else versions:
                if candidate in versions:
                    pending.append((dep_name, candidate))
    return sorted(seen)


def main():
    v6 = Path(__file__).resolve().parent.parent
    closures = []
    for lock in LOCKS:
        path = v6 / lock
        if not path.exists():
            print(f"FAIL: {path} is missing")
            return 1
        found = closure(packages(path.read_text()))
        if not found:
            print(f"FAIL: {lock} resolves no `{ROOT}` package")
            return 1
        closures.append(found)

    extract, engine = closures
    if extract != engine:
        for entry in sorted(set(extract) ^ set(engine)):
            side = LOCKS[0] if entry in set(extract) else LOCKS[1]
            print(f"  {entry[0]} {entry[1]}  ({side} only)")
        print(f"FAIL: {LOCKS[0]} and {LOCKS[1]} resolve {ROOT} differently")
        print(f"fix: cd v6/sprefa-extract && cargo update -p <crate> --precise <version>")
        return 1
    print(f"PASS: one {ROOT} closure, {len(extract)} crates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
