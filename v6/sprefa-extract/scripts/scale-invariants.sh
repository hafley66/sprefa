#!/usr/bin/env bash
# Per-file df invariants over external-scale Rust corpora. Usage: scale-invariants.sh <extract-binary> <repo-dir> [repo-dir ...]

set -u

if [ "$#" -lt 2 ]; then
  echo "usage: $0 <extract-binary> <repo-dir> [repo-dir ...]" >&2
  exit 2
fi

EXTRACT="$1"
shift
REPOS=("$@")

if [ ! -x "$EXTRACT" ]; then
  echo "extract binary not executable: $EXTRACT" >&2
  exit 2
fi

python3 - "$EXTRACT" "${REPOS[@]}" <<'PY'
import json
import subprocess
import sys
import os

extract = sys.argv[1]
repos = sys.argv[2:]

# invariant counters: {repo: {inv: count}}
totals = {}
per_file = {}   # {repo: {"files": n, "facts": n}}
first_violation = {}  # {repo: {inv: (path, detail)}}

INV_NAMES = {
    1: "span start<=end<=bytes",
    2: "zero-width df node",
    3: "dup (span,kind) node",
    4: "edge endpoint unresolved",
    5: "process exit != 0",
}

def analyze_file(path, nbytes, proc, inv, fv):
    """Accumulate df invariants for one file's JSONL into inv/fv; returns fact count."""
    facts = 0
    if proc.returncode != 0:
        inv[5] += 1
        fv.setdefault(5, (path, f"rc={proc.returncode}"))
    nodes = {}   # (start,end,kind) -> count of node facts
    edges = []   # (from start,end,kind, to start,end,kind)
    span_ok = True

    def chk(span):
        nonlocal span_ok
        if not span_ok:
            return
        s, e = span["start"], span["end"]
        if s > e or e > nbytes:
            inv[1] += 1
            span_ok = False
            fv.setdefault(1, (path, f"span=({s},{e}) bytes={nbytes}"))

    for line in proc.stdout.splitlines():
        if not line.strip():
            continue
        try:
            f = json.loads(line)
        except json.JSONDecodeError:
            continue
        if f.get("family") != "df":
            continue
        facts += 1
        rec = f.get("record")
        if rec == "node":
            chk(f["span"])
            if f["span"]["start"] == f["span"]["end"]:
                inv[2] += 1
                fv.setdefault(2, (path, f"kind={f['kind']} span={f['span']}"))
            key = (f["span"]["start"], f["span"]["end"], f.get("kind"))
            nodes[key] = nodes.get(key, 0) + 1
        elif rec == "edge":
            chk(f["from"])
            chk(f["to"])
            edges.append((
                f["from"]["start"], f["from"]["end"], f.get("from_kind"),
                f["to"]["start"], f["to"]["end"], f.get("to_kind"),
            ))
        elif rec == "param":
            chk(f["span"])
        elif rec == "arg":
            chk(f["call"])
            chk(f["arg"])
    for key, count in nodes.items():
        if count > 1:
            inv[3] += 1
            fv.setdefault(3, (path, f"key={key} x{count}"))
    for (fs, fe, fk, ts, te, tk) in edges:
        if (fs, fe, fk) not in nodes:
            inv[4] += 1
            fv.setdefault(4, (path, f"from=({fs},{fe},{fk})"))
        if (ts, te, tk) not in nodes:
            inv[4] += 1
            fv.setdefault(4, (path, f"to=({ts},{te},{tk})"))
    return facts

for repo in repos:
    files = []
    for root, dirs, names in os.walk(repo):
        # skip .git and vendor/target trees that are not first-party Rust
        dirs[:] = [d for d in dirs if d not in (".git", "target", "node_modules")]
        for n in names:
            if n.endswith(".rs"):
                files.append(os.path.join(root, n))
    files.sort()

    inv = {k: 0 for k in INV_NAMES}
    facts = 0
    files_scanned = 0
    fv = {}

    for path in files:
        files_scanned += 1
        try:
            with open(path, "rb") as fh:
                content = fh.read()
        except OSError:
            continue
        nbytes = len(content)
        proc = subprocess.run(
            [extract, "--family", "df", path],
            capture_output=True, text=True,
        )
        facts += analyze_file(path, nbytes, proc, inv, fv)

    totals[repo] = inv
    per_file[repo] = {"files": files_scanned, "facts": facts}
    first_violation[repo] = fv

# ---- report ----
def short(r):
    return os.path.basename(r.rstrip("/"))

print("REPO                        files      facts   " + "  ".join(
    f"I{i:>2}={INV_NAMES[i].split()[0]}" for i in INV_NAMES))
print("-" * 100)
grand_files = 0
grand_facts = 0
grand = {k: 0 for k in INV_NAMES}
for repo in repos:
    pf = per_file[repo]
    inv = totals[repo]
    grand_files += pf["files"]
    grand_facts += pf["facts"]
    for k in INV_NAMES:
        grand[k] += inv[k]
    row = f"{short(repo):30s} {pf['files']:7d} {pf['facts']:9d}   " + "  ".join(
        f"{inv[i]:5d}" for i in INV_NAMES)
    print(row)
print("-" * 100)
row = f"{'TOTAL':30s} {grand_files:7d} {grand_facts:9d}   " + "  ".join(
    f"{grand[i]:5d}" for i in INV_NAMES)
print(row)
print()

for repo in repos:
    if any(first_violation[repo].values()):
        print(f"first violation per invariant — {short(repo)}:")
        for k in INV_NAMES:
            if k in first_violation[repo]:
                path, detail = first_violation[repo][k]
                print(f"  I{k}: {detail}  [{path}]")

ok = all(grand[k] == 0 for k in INV_NAMES)
print()
print("RESULT: " + ("PASS" if ok else "VIOLATIONS"))
sys.exit(0 if ok else 1)
PY
