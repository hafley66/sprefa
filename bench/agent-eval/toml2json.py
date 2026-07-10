#!/usr/bin/env python3
"""toml2json.py <file.toml> — dumps a TOML file as JSON on stdout. Used by
run.sh to read experiment.toml with jq (bash has no TOML parser)."""
import json
import sys


def load_toml(path):
    try:
        import tomllib
        with open(path, "rb") as f:
            return tomllib.load(f)
    except ModuleNotFoundError:  # pragma: no cover - py<3.11 fallback
        import tomli
        with open(path, "rb") as f:
            return tomli.load(f)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    json.dump(load_toml(sys.argv[1]), sys.stdout)
