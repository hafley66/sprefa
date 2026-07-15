#!/usr/bin/env python3
"""Run the prebuilt call-graph probe on repo-local generated fixtures."""

import argparse
import json
import os
import subprocess
from pathlib import Path

from generate_fixture import ALLOWED_SIZES, DEFAULT_SEED, generate

REPO = Path(__file__).resolve().parents[2]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--harness", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=REPO / "target/reactivity/probe")
    parser.add_argument("--sizes", nargs="+", type=int, default=list(ALLOWED_SIZES))
    args = parser.parse_args()

    harness = args.harness.resolve(strict=True)
    output = Path(os.path.abspath(args.output))
    if REPO not in output.parents:
        parser.error("output must be inside the sprefa repository")
    if output.exists():
        parser.error(f"output exists; remove it to rerun: {output}")
    output.mkdir(parents=True)

    results = []
    for size in args.sizes:
        if size not in ALLOWED_SIZES:
            parser.error(f"size must be one of {ALLOWED_SIZES}")
        case = output / f"files-{size}"
        root = case / "root"
        manifest = case / "manifest.json"
        case.mkdir()
        generate(root, manifest, size, DEFAULT_SEED)
        database = case / "probe.sqlite"
        changed = root / "corpus/file_0000.rs"
        env = os.environ.copy()
        env.update({"CARGO_BUILD_JOBS": "2", "DL_RAYON_THREADS": "2"})
        completed = subprocess.run(
            [str(harness), str(root), str(database), str(changed)],
            cwd=REPO,
            env=env,
            text=True,
            capture_output=True,
            check=True,
            timeout=120,
        )
        result = json.loads(completed.stdout)
        (case / "result.json").write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n"
        )
        results.append(result)

    print(json.dumps(results, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
