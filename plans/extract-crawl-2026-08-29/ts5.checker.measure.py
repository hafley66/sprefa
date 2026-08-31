#!/usr/bin/env python3
"""ts5 corpus, syntax leg vs checker tier, scored against both call oracles.

The normal form and the scoring are the Python twins of
`v6/sprefa-extract/tests/bench/mod.rs` (`normal_form`, `score`, `buckets`);
that file stays the reference and is NOT edited by this measurement. Nothing
here writes RATCHET.tsv.

    ts5.checker.measure.py <extract-binary> <corpus-root> <out-dir>
"""
import json
import os
import subprocess
import sys
import time

BENCH = os.path.join(os.path.dirname(__file__), "..", "extract-bench-2026-08-29")
ORACLES = ["ts5.oracle.call.tsv", "ts.codeql2.call.tsv"]


def corpus_files(root):
    """`src/**` minus `src/lib`, `.ts` only, sorted: bench/mod.rs `wants`."""
    found = []
    for dirpath, dirnames, filenames in os.walk(os.path.join(root, "src")):
        dirnames[:] = [d for d in dirnames if not d.startswith(".") and d != "node_modules"]
        for name in filenames:
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, root)
            parts = rel.split(os.sep)
            if parts[0] != "src" or (len(parts) >= 2 and parts[1] == "lib"):
                continue
            if rel.endswith(".ts"):
                found.append(path)
    return sorted(found)


def rel(root, path):
    return path[len(root):].lstrip("/") if path.startswith(root) else path


def call_rows(raw_path, root):
    """Every `resolved_edge` as `src_path\tsrc_name\tdst_path\tdst_name`."""
    rows = set()
    origins = {}
    with open(raw_path) as handle:
        for line in handle:
            line = line.strip()
            if not line or '"resolved_edge"' not in line:
                continue
            fact = json.loads(line)
            if fact.get("record") != "resolved_edge":
                continue
            row = "\t".join([
                rel(root, fact["caller_path"]), fact.get("caller_name") or "",
                rel(root, fact["callee_path"]), fact.get("callee_name") or "",
            ])
            rows.add(row)
            origins.setdefault(row, set()).add(fact.get("resolution_origin") or "")
    return rows, origins


def score(ours, oracle):
    overlap = ours & oracle
    judged = {tuple(row.split("\t")[:2]) for row in oracle}
    contradicted = unjudged = 0
    for row in ours - overlap:
        if tuple(row.split("\t")[:2]) in judged:
            contradicted += 1
        else:
            unjudged += 1
    return {
        "ours": len(ours),
        "oracle": len(oracle),
        "overlap": len(overlap),
        "recall": 100.0 * len(overlap) / len(oracle) if oracle else 0.0,
        "precision": 100.0 * len(overlap) / len(ours) if ours else 0.0,
        "matched": len(overlap),
        "contradicted": contradicted,
        "unjudged": unjudged,
    }


def run(binary, root, files, out_dir, checker):
    """One extract process, timed, its OWN peak RSS from wait4's rusage.

    The checker leg's `node` runs in its own process group, so its peak is not
    in this figure; the report carries it as a separate row.
    """
    leg = "checker" if checker else "syntax"
    raw = os.path.join(out_dir, f"ts5.{leg}.jsonl")
    args = [binary, "--resolve", "--family", "call,type", "--project-root", root]
    if checker:
        args.append("--ts-checker")
    args.extend(files)
    started = time.monotonic()
    with open(raw, "wb") as sink, open(os.path.join(out_dir, f"ts5.{leg}.stderr"), "wb") as err:
        child = subprocess.Popen(["nice", "-n", "15"] + args, stdout=sink, stderr=err)
        _, status, usage = os.wait4(child.pid, 0)
    wall = time.monotonic() - started
    child.returncode = os.waitstatus_to_exitcode(status)
    return {
        "leg": leg,
        "rc": child.returncode,
        "raw": raw,
        "wall_s": round(wall, 2),
        "rss_peak_mb": round(usage.ru_maxrss / (1024 * 1024), 1),
    }


def main():
    binary, root, out_dir = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]
    os.makedirs(out_dir, exist_ok=True)
    files = corpus_files(root)
    print(f"corpus: {len(files)} files under {root}", flush=True)

    oracles = {}
    for name in ORACLES:
        with open(os.path.join(BENCH, name)) as handle:
            oracles[name] = {line.rstrip("\n") for line in handle if line.strip()}

    report = {"files": len(files), "legs": []}
    for checker in (False, True):
        run_stats = run(binary, root, files, out_dir, checker)
        rows, origins = call_rows(run_stats["raw"], root)
        by_origin = {}
        for row in rows:
            for origin in origins[row]:
                by_origin[origin] = by_origin.get(origin, 0) + 1
        run_stats["origins"] = by_origin
        run_stats["scores"] = {name: score(rows, oracle) for name, oracle in oracles.items()}
        report["legs"].append(run_stats)
        print(json.dumps(run_stats, indent=2), flush=True)

    with open(os.path.join(out_dir, "ts5.checker.report.json"), "w") as handle:
        json.dump(report, handle, indent=2)


main()
