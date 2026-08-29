#!/usr/bin/env python3
"""Run extract over a corpus. DEFAULT: one process for the whole corpus.

Every index in sprefa-extract is per-process, so a file partition drops every
fact whose target file sits in another process (docs/failure-modes.md entry
96). The old default here grouped by directory depth and split on rc=124,
which is why the committed *.chunked.tsv files carry zero cross-crate rust
rows and zero cross-top-2-dir go rows. Those numbers are floors, not
measurements.

--chunk restores the split path. It exists only to reproduce the pre-2026-08-29
numbers and to bisect a corpus that a single process cannot finish; anything
it produces is a floor and must be labelled as one.

Writes raw JSONL plus a per-run tsv log (group, mode, files, rc, ms, lines).
Under the default that log has exactly ONE line, so `wc -l` on it is the
process count and is the receipt that no split happened.
"""
import argparse
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


def argv_for(mode, files, project_root=None, scip_index=None):
    if mode == "resolve":
        return [BIN, "--resolve", "--family", "call,type"] + files
    if mode == "diet_scip":
        return [BIN, "--family", "diet_scip"] + files
    if mode == "scip_override":
        return [BIN, "--resolve", "--family", "call", "--project-root", project_root,
                "--scip-index", scip_index] + files
    if mode == "deps":
        return [BIN, "--deps", "--project-root", project_root] + files
    raise ValueError(mode)


def run_one(mode, files, timeout_s, project_root=None, scip_index=None):
    argv = argv_for(mode, files, project_root, scip_index)
    t0 = time.time()
    p = subprocess.run(["timeout", str(timeout_s)] + argv, capture_output=True, text=True)
    return p, int((time.time() - t0) * 1000)


def run_whole(mode, files, out_f, runs_log, timeout_s, project_root=None, scip_index=None):
    p, ms = run_one(mode, files, timeout_s, project_root, scip_index)
    lines = p.stdout.splitlines() if p.returncode == 0 else []
    for line in lines:
        out_f.write(line + "\n")
    runs_log.write(f"WHOLE\t{mode}\t{len(files)}\t{p.returncode}\t{ms}\t{len(lines)}\n")
    if p.returncode != 0:
        # A non-zero rc here is a defect to investigate, never a cue to split:
        # splitting is what produced the floors entry 96 records.
        print(f"{mode}: rc={p.returncode} after {ms} ms, stderr tail:\n"
              f"{p.stderr[-2000:]}", file=sys.stderr)
    return p.returncode


def run_group(mode, key, files, out_f, runs_log, timeout_s, depth_left=6,
              project_root=None, scip_index=None):
    p, ms = run_one(mode, files, timeout_s, project_root, scip_index)
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
    run_group(mode, key + "/a", files[:mid], out_f, runs_log, timeout_s, depth_left - 1,
              project_root, scip_index)
    run_group(mode, key + "/b", files[mid:], out_f, runs_log, timeout_s, depth_left - 1,
              project_root, scip_index)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("lang")
    ap.add_argument("root")
    ap.add_argument("files_list")
    ap.add_argument("out_dir")
    ap.add_argument("--modes", default="resolve,diet_scip")
    ap.add_argument("--timeout", type=int, default=60)
    ap.add_argument("--project-root")
    ap.add_argument("--scip-index")
    ap.add_argument("--chunk", action="store_true",
                    help="split the corpus by directory depth and bisect on timeout; "
                         "the output is a FLOOR, see the module docstring")
    ap.add_argument("--depth", type=int, default=3, help="grouping depth, --chunk only")
    ap.add_argument("--suffix", default=None,
                    help="tag written into the output filenames; defaults to "
                         "'chunked' under --chunk and 'single' otherwise")
    args = ap.parse_args()

    with open(args.files_list) as fh:
        files = [line.strip() for line in fh if line.strip()]
    files = [f if os.path.isabs(f) else os.path.join(args.root, f) for f in files]

    suffix = args.suffix or ("chunked" if args.chunk else "single")
    os.makedirs(args.out_dir, exist_ok=True)

    for mode in args.modes.split(","):
        out_path = os.path.join(args.out_dir, f"{args.lang}.parse.{mode}.{suffix}.raw.jsonl")
        runs_path = os.path.join(args.out_dir, f"{args.lang}.{mode}.{suffix}.runs.tsv")
        with open(out_path, "w") as out_f, open(runs_path, "w") as runs_log:
            if args.chunk:
                groups = group_by_depth(files, args.root, args.depth)
                print(f"{args.lang} {mode}: FLOOR run, {len(files)} files in "
                      f"{len(groups)} groups at depth {args.depth}", file=sys.stderr)
                for i, (key, group_files) in enumerate(sorted(groups.items())):
                    run_group(mode, key, group_files, out_f, runs_log, args.timeout,
                              project_root=args.project_root, scip_index=args.scip_index)
                    if i % 20 == 0:
                        print(f"{args.lang} {mode}: {i}/{len(groups)} groups", file=sys.stderr)
            else:
                print(f"{args.lang} {mode}: ONE process, {len(files)} files", file=sys.stderr)
                run_whole(mode, files, out_f, runs_log, args.timeout,
                          project_root=args.project_root, scip_index=args.scip_index)
        with open(runs_path) as fh:
            n_procs = sum(1 for line in fh if line.strip())
        print(f"{args.lang} {mode} done -> {out_path} ({n_procs} process(es))",
              file=sys.stderr)


if __name__ == "__main__":
    main()
