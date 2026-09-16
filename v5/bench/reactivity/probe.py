#!/usr/bin/env python3
"""Run the prebuilt call-graph probe on repo-local generated fixtures.

Each size is measured `--warmup + --repeats` times: a FRESH deterministic
fixture (same seed, so byte-identical content every time) and a fresh
database per iteration, so no state carries between repeats. The first
`--warmup` iterations are discarded; the summary (mean/stdev/min/max per
phase) is computed only over the `--repeats` measured iterations. A single
run is not a measurement — this is why the default is 5 repeats, not 1, and
why the output reports spread, not just a mean.
"""

import argparse
import json
import os
import statistics
import subprocess
from pathlib import Path

from generate_fixture import ALLOWED_SIZES, DEFAULT_SEED, generate

REPO = Path(__file__).resolve().parents[2]
PHASES = ("cold", "warm", "one_file_edit", "clean_rebuild")


def stats(values):
    """mean/stdev/min/max over `values`. stdev is 0.0 for a single sample
    (statistics.stdev requires >=2 points; that is itself a signal the
    caller should pass more repeats, not a reason to crash)."""
    return {
        "mean_ms": statistics.fmean(values),
        "stdev_ms": statistics.stdev(values) if len(values) > 1 else 0.0,
        "min_ms": min(values),
        "max_ms": max(values),
        "n": len(values),
    }


def run_one(harness, root, database, changed, env):
    completed = subprocess.run(
        [str(harness), str(root), str(database), str(changed)],
        cwd=REPO,
        env=env,
        text=True,
        capture_output=True,
        check=True,
        timeout=120,
    )
    return json.loads(completed.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--harness", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=REPO / "target/reactivity/probe")
    parser.add_argument("--sizes", nargs="+", type=int, default=list(ALLOWED_SIZES))
    parser.add_argument("--repeats", type=int, default=5, help="measured iterations per size")
    parser.add_argument("--warmup", type=int, default=1, help="discarded iterations per size, run first")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be >= 1")
    if args.warmup < 0:
        parser.error("--warmup must be >= 0")

    harness = args.harness.resolve(strict=True)
    output = Path(os.path.abspath(args.output))
    if REPO not in output.parents:
        parser.error("output must be inside the sprefa repository")
    if output.exists():
        parser.error(f"output exists; remove it to rerun: {output}")
    output.mkdir(parents=True)

    env = os.environ.copy()
    env.update({"CARGO_BUILD_JOBS": "2", "DL_RAYON_THREADS": "2"})

    summaries = []
    for size in args.sizes:
        if size not in ALLOWED_SIZES:
            parser.error(f"size must be one of {ALLOWED_SIZES}")
        size_dir = output / f"files-{size}"
        size_dir.mkdir()

        raw_iterations = []
        total_iterations = args.warmup + args.repeats
        for iteration in range(total_iterations):
            case = size_dir / f"iter-{iteration:02d}"
            root = case / "root"
            manifest = case / "manifest.json"
            case.mkdir()
            generate(root, manifest, size, DEFAULT_SEED)
            database = case / "probe.sqlite"
            changed = root / "corpus/file_0000.rs"
            result = run_one(harness, root, database, changed, env)
            (case / "result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
            raw_iterations.append({"iteration": iteration, "warmup": iteration < args.warmup, "result": result})

        measured = raw_iterations[args.warmup:]

        # Sanity: a deterministic, byte-identical fixture must reproduce the
        # SAME row count and semantic digest on every measured iteration —
        # if it doesn't, the harness itself is non-deterministic and no
        # timing number drawn from it can be trusted.
        digests_by_phase = {
            phase: {iteration["result"][phase]["semantic_digest"] for iteration in measured}
            for phase in PHASES
        }
        deterministic = all(len(digests) == 1 for digests in digests_by_phase.values())
        equivalent = all(iteration["result"]["equivalent"] for iteration in measured)

        phase_stats = {
            phase: stats([iteration["result"][phase]["wall_ms"] for iteration in measured])
            for phase in PHASES
        }

        summary = {
            "fixture_files": size,
            "warmup": args.warmup,
            "repeats": args.repeats,
            "deterministic_output": deterministic,
            "equivalent_every_iteration": equivalent,
            "phases": phase_stats,
        }
        (size_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
        (size_dir / "raw.json").write_text(json.dumps(raw_iterations, indent=2, sort_keys=True) + "\n")
        summaries.append(summary)

    print(json.dumps(summaries, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
