#!/usr/bin/env python3
"""Run extract --resolve and --family diet_scip over a corpus, one group at
a time, splitting on rc=124 until every call fits the 10s law. Writes raw
JSONL plus a per-run tsv log (group, rc, ms, lines)."""
import json
import os
import subprocess
import sys
import time

BIN = os.path.join(os.path.dirname(__file__), "..", "..", "v6", "sprefa-extract",
                    "target", "release", "extract")
BIN = os.path.abspath(BIN)


def group_by_depth(files, root, depth):
    groups = {}
    for f in files:
        rel = os.path.relpath(f, root)
        parts = rel.split(os.sep)
        key = os.sep.join(parts[:depth]) if len(parts) > depth else rel
        groups.setdefault(key, []).append(f)
    return groups


def run_one(mode, files, out_jsonl, runs_log, timeout_s=10, project_root=None, scip_index=None):
    if mode == "resolve":
        argv = [BIN, "--resolve", "--family", "call,type"] + files
    elif mode == "diet_scip":
        argv = [BIN, "--family", "diet_scip"] + files
    elif mode == "scip_override":
        argv = [BIN, "--resolve", "--family", "call", "--project-root", project_root,
                "--scip-index", scip_index] + files
    else:
        raise ValueError(mode)
    t0 = time.time()
    p = subprocess.run(["timeout", str(timeout_s)] + argv, capture_output=True, text=True)
    ms = int((time.time() - t0) * 1000)
    return p, ms


def run_group(mode, key, files, out_f, runs_log, timeout_s=10, depth_left=6,
              project_root=None, scip_index=None):
    p, ms = run_one(mode, files, out_f, runs_log, timeout_s, project_root, scip_index)
    if p.returncode == 0:
        lines = p.stdout.splitlines()
        for line in lines:
            out_f.write(line + "\n")
        runs_log.write(f"{key}\t{mode}\t{len(files)}\t{p.returncode}\t{ms}\t{len(lines)}\n")
        return
    if len(files) == 1 or depth_left <= 0:
        runs_log.write(f"{key}\t{mode}\t{len(files)}\t{p.returncode}\t{ms}\tSPLIT_FLOOR\n")
        return
    mid = len(files) // 2
    run_group(mode, key + "/a", files[:mid], out_f, runs_log, timeout_s, depth_left - 1, project_root, scip_index)
    run_group(mode, key + "/b", files[mid:], out_f, runs_log, timeout_s, depth_left - 1, project_root, scip_index)


def main():
    lang = sys.argv[1]
    root = sys.argv[2]
    files_list = sys.argv[3]
    out_dir = sys.argv[4]
    depth = int(sys.argv[5]) if len(sys.argv) > 5 else 3
    modes = sys.argv[6].split(",") if len(sys.argv) > 6 else ["resolve", "diet_scip"]
    project_root = sys.argv[7] if len(sys.argv) > 7 else None
    scip_index = sys.argv[8] if len(sys.argv) > 8 else None

    with open(files_list) as fh:
        files = [line.strip() for line in fh if line.strip()]

    groups = group_by_depth(files, root, depth)
    print(f"{lang}: {len(files)} files, {len(groups)} groups at depth {depth}", file=sys.stderr)

    for mode in modes:
        out_path = os.path.join(out_dir, f"{lang}.parse.{mode}.raw.jsonl")
        runs_path = os.path.join(out_dir, f"{lang}.parse.{mode}.runs.tsv")
        with open(out_path, "w") as out_f, open(runs_path, "w") as runs_log:
            for i, (key, group_files) in enumerate(sorted(groups.items())):
                run_group(mode, key, group_files, out_f, runs_log, project_root=project_root, scip_index=scip_index)
                if i % 20 == 0:
                    print(f"{lang} {mode}: {i}/{len(groups)} groups", file=sys.stderr)
        print(f"{lang} {mode} done -> {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
