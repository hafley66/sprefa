#!/usr/bin/env python3
"""Per-file battery: one `extract` process per file. argv: LIST OUT [FAMILY]"""
import os, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor

EX = "/Users/chrishafley/projects/sprefa/.boop-worktrees/crawl/extract-typescript-5/v6/sprefa-extract/target/release/extract"
CORPUS = "/Users/chrishafley/projects/TypeScript-5.9"
FAM = sys.argv[3] if len(sys.argv) > 3 else None


def run_one(rel):
    full = os.path.join(CORPUS, rel)
    try:
        nbytes = os.path.getsize(full)
    except OSError:
        nbytes = -1
    cmd = [EX]
    if FAM:
        cmd += ["--family", FAM]
    cmd.append(full)
    t0 = time.monotonic()
    try:
        p = subprocess.run(cmd, capture_output=True, timeout=10)
        rc, out, err = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        rc, out, err = 124, e.stdout or b"", e.stderr or b""
    ms = int((time.monotonic() - t0) * 1000)
    lines = out.count(b"\n")
    skip = out.count(b'"size_skip"')
    first_err = err.split(b"\n")[0].decode("utf8", "replace").replace("\t", " ")[:200]
    return f"{rel}\t{rc}\t{ms}\t{nbytes}\t{lines}\t{skip}\t{first_err}"


files = [l.strip() for l in open(sys.argv[1]) if l.strip()]
with open(sys.argv[2], "w") as fh:
    fh.write("path\trc\tms\tbytes\tlines\tsize_skip\terr\n")
    with ThreadPoolExecutor(max_workers=8) as ex:
        for i, row in enumerate(ex.map(run_one, files)):
            fh.write(row + "\n")
            if i % 500 == 0:
                fh.flush()
                print(f"{i}/{len(files)}", flush=True)
print(f"DONE {len(files)}", flush=True)
