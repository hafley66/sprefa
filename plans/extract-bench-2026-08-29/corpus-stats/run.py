#!/usr/bin/env python3
"""corpus-stats: run `extract --resolve` over a TSV of repos, one STATS.tsv row per repo+lang+arm.

Usage:
  run.py --lang <go|ts|rust|python> --repos REPOS.tsv [--arm diet|checker] [--stats STATS.tsv]

REPOS.tsv is tab-separated with a header: repo, url, lang, pinned_sha.
Rows are idempotent: a rerun replaces the (repo, lang, arm) row, never duplicates.
"""
import argparse
import collections
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPREFA = HERE.parents[2]
EXTRACT = SPREFA / "v6" / "sprefa-extract" / "target" / "release" / "extract"
CORPORA = Path.home() / "corpora"
CAP_S = 15 * 60
STATS_HEADER = (
    "repo\tlang\tarm\tsha\tfiles\tloc\trows_call\trows_type\trows_module"
    "\tunresolved\twall_s\tpeak_rss_mb\tovercap\textract_describe\n"
)

# File-set rule per language: extensions to keep, directory names to prune.
LANG_RULES = {
    "go": {
        "exts": {".go"},
        "prune": {"vendor", "testdata", "examples"},
        "skip_name": {"_test.go", "_test"},
    },
    "ts": {
        "exts": {".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"},
        "prune": {"node_modules", "dist", "build", "coverage"},
        "skip_name": set(),
    },
    "rust": {
        # #606 rule: every tracked .rs whose path carries a /src/ component.
        "exts": {".rs"},
        "prune": {"target"},
        "skip_name": set(),
        "require_component": "src",
    },
    "python": {
        "exts": {".py"},
        "prune": {"venv", ".venv", "build", "dist", "__pycache__"},
        "skip_name": set(),
    },
}


def git(*args, cwd=None):
    out = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True, check=True
    )
    return out.stdout.strip()


def clone_or_reuse(name, url):
    dest = CORPORA / name
    if not (dest / ".git").exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        print(f"clone {name} (shallow)...")
        subprocess.run(
            ["git", "clone", "--depth", "1", url, str(dest)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
    return dest, git("rev-parse", "HEAD", cwd=dest)


def select_files(root, lang):
    rule = LANG_RULES[lang]
    files = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [
            d
            for d in dirnames
            if d not in rule["prune"] and not d.startswith(".")
        ]
        for fn in filenames:
            if fn in rule["skip_name"]:
                continue
            ext = os.path.splitext(fn)[1]
            if ext not in rule["exts"]:
                continue
            rel = Path(dirpath, fn).relative_to(root)
            if rule.get("require_component") and f"/{rule['require_component']}/" not in f"/{rel}":
                continue
            files.append(str(rel))
    return sorted(files)


def run_extract(root, files, arm, lang):
    """Run extract --resolve under /usr/bin/time -l, capped at CAP_S. Returns (counter, wall_s, rss_mb, overcap, rc)."""
    cmd = [
        "/usr/bin/time",
        "-l",
        "nice",
        "-n",
        "15",
        str(EXTRACT),
        "--resolve",
        "--family",
        "call,type",
    ]
    if arm == "checker":
        cmd += ["--rust-checker", "--project-root", str(root)]
    cmd += [str(root / f) for f in files]

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    overcap = False
    t0 = time.monotonic()
    try:
        stdout, stderr = proc.communicate(timeout=CAP_S)
    except subprocess.TimeoutExpired:
        os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        stdout, stderr = proc.communicate()
        overcap = True
    wall = time.monotonic() - t0
    if overcap:
        wall = float(CAP_S)

    counts = collections.Counter()
    for line in stdout.decode("utf-8", "replace").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            counts[json.loads(line)["record"]] += 1
        except (json.JSONDecodeError, KeyError):
            continue

    stderr_text = stderr.decode("utf-8", "replace")
    rss_mb = ""
    import re

    m = re.search(r"(\d+)\s+maximum resident set size", stderr_text)
    if m:
        rss_mb = f"{int(m.group(1)) / (1024 * 1024):.1f}"
    return counts, wall, rss_mb, overcap, proc.returncode, stderr_text


def upsert_stats(stats_path, row_cells):
    """Replace the row matching (repo, lang, arm), else append."""
    key = (row_cells[0], row_cells[1], row_cells[2])
    rows = []
    if stats_path.exists():
        lines = stats_path.read_text().splitlines()
        header = lines[0] if (lines and lines[0].startswith("repo\t")) else None
        body = lines[1:] if header else lines
        rows = [ln for ln in body if ln.split("\t")[:3] != list(key)]
    rows.append("\t".join(row_cells))
    with open(stats_path, "w") as f:
        f.write(STATS_HEADER)
        f.write("\n".join(rows) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--lang", required=True, choices=["go", "ts", "rust", "python"])
    ap.add_argument("--repos", required=True)
    ap.add_argument("--arm", choices=["diet", "checker"], default="diet")
    ap.add_argument("--stats", default=str(HERE / "STATS.tsv"))
    args = ap.parse_args()

    if args.arm == "checker" and args.lang != "rust":
        ap.error("--arm checker only defined for --lang rust")

    describe = git("describe", "--always", cwd=SPREFA)
    if not EXTRACT.exists():
        raise SystemExit(f"extract binary missing: {EXTRACT}")

    with open(args.repos) as f:
        header = f.readline().rstrip("\n").split("\t")
        if header[:4] != ["repo", "url", "lang", "pinned_sha"]:
            raise SystemExit(f"{args.repos}: unexpected header {header}")
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            name, url, lang, _pinned = line.split("\t")[:4]
            if lang != args.lang:
                continue
            root, sha = clone_or_reuse(name, url)
            files = select_files(root, lang)
            if not files:
                print(f"{name}: no {lang} files matched, skipping")
                continue
            loc = sum(
                1
                for f_ in files
                for _ in open(root / f_, "rb")
            )
            counts, wall, rss_mb, overcap, rc, stderr_text = run_extract(
                root, files, args.arm, lang
            )
            row_cells = [
                name,
                lang,
                args.arm,
                sha,
                str(len(files)),
                str(loc),
                str(counts.get("resolved_edge", 0)),
                str(counts.get("resolved_type_edge", 0)),
                str(counts.get("resolved_import", 0)),
                str(counts.get("unresolved", 0)),
                f"{wall:.1f}" if not overcap else "CAP",
                rss_mb,
                "1" if overcap else "0",
                describe,
            ]
            upsert_stats(Path(args.stats), row_cells)
            print(
                f"{name}: files={len(files)} loc={loc} call={counts.get('resolved_edge', 0)}"
                f" type={counts.get('resolved_type_edge', 0)} module={counts.get('resolved_import', 0)}"
                f" unresolved={counts.get('unresolved', 0)} wall={row_cells[10]}s rss={rss_mb or '?'}MB"
                f" overcap={overcap} rc={rc}"
            )
            if rc not in (0, None) and not overcap:
                print(f"  stderr tail: {stderr_text[-300:]}")


if __name__ == "__main__":
    main()
