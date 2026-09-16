#!/usr/bin/env python3
"""Co-change validation: does git-history co-occurrence of file pairs
recover the known refactor splits and the 10 consensus clusters from
the god-object decomposition study?

Ground truth (2 sources):
  1. refactor_log.md: 3 iters all inside engine.rs (single-file, so file-grain
     co-change can't see them; they're a negative control — co-change should
     NOT flag these as cross-file pairs, and we report that).
  2. god-object study: 10 unanimous consensus clusters of Engine methods +
     the dl field-coverage partition (db_only=44 / stateful=48 / pure_fn=18).
     The study's own conclusion: "Name clustering, topological layering, or
     co-change are the only signals that could further cut the [stateful] core."
     This script tests the co-change hypothesis directly.

Method:
  - git log --max-count=N --name-only --format=%H on src
  - group files by commit, emit co-occurring file pairs with counts
  - rank pairs by co-change frequency
  - check: do top pairs correspond to the known structural partitions?
"""
import subprocess, collections, sys, os

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(ROOT)))  # sprefa/
SRC = "src"

def git_log_files(max_count, rev="HEAD"):
    """Yield (commit, [files]) for each commit touching src."""
    out = subprocess.check_output(
        ["git", "-C", REPO, "log", f"--max-count={max_count}",
         "--name-only", "--format=COMMIT %H", rev, "--", SRC],
        text=True,
    )
    commit = None
    files = []
    for line in out.splitlines():
        if line.startswith("COMMIT "):
            if commit and files:
                yield commit, files
            commit = line.split()[1]
            files = []
        elif line.strip():
            files.append(line.strip())
    if commit and files:
        yield commit, files

def cochange_pairs(max_count):
    """Return Counter of frozenset({a, b}) -> co-change count."""
    pair_counts = collections.Counter()
    file_counts = collections.Counter()
    n_commits = 0
    for commit, files in git_log_files(max_count):
        n_commits += 1
        uniq = sorted(set(files))
        for f in uniq:
            file_counts[f] += 1
        for i in range(len(uniq)):
            for j in range(i+1, len(uniq)):
                pair_counts[frozenset({uniq[i], uniq[j]})] += 1
    return pair_counts, file_counts, n_commits

def main():
    for depth in [50, 200, 500, 1000]:
        pairs, files, n_commits = cochange_pairs(depth)
        if n_commits == 0:
            print(f"=== depth={depth}: no commits touching {SRC} ===")
            continue
        print(f"=== depth={depth}: {n_commits} commits, {len(files)} files, {len(pairs)} pairs ===")

        # Top 20 file pairs by co-change count
        top = pairs.most_common(20)
        print(f"\nTop 20 co-change pairs (of {len(pairs)}):")
        for pair, n in top:
            a, b = sorted(pair)
            # relative rate: how often do they co-change vs individual activity
            fa = files[a]; fb = files[b]
            jaccard = n / (fa + fb - n) if (fa + fb - n) else 0
            print(f"  {n:3d}/{n_commits}  J={jaccard:.2f}  {a}  <->  {b}")

        # Check against known structural partitions
        print("\nKnown partitions vs co-change:")
        known_pairs = [
            ("src/engine.rs", "src/db.rs",         "Engine <-> Db (the 44 db_only extraction target)"),
            ("src/engine.rs", "src/typegraph.rs",  "Engine <-> TypeGraph (type_edge producer)"),
            ("src/engine.rs", "src/scip_import.rs","Engine <-> SCIP importer"),
            ("src/engine.rs", "src/lsp.rs",        "Engine <-> LSP"),
            ("src/engine.rs", "src/parse.rs",      "Engine <-> Parse"),
            ("src/engine.rs", "src/modgraph.rs",   "Engine <-> ModGraph"),
            ("src/engine.rs", "src/propose.rs",    "Engine <-> Propose (clone kernels)"),
            ("src/engine.rs", "src/daemon.rs",     "Engine <-> Daemon"),
            ("src/engine.rs", "src/rule.rs",       "Engine <-> Rule eval"),
            ("src/typegraph.rs", "src/parse.rs",   "TypeGraph <-> Parse"),
            ("src/typegraph.rs", "src/scip_import.rs", "TypeGraph <-> SCIP"),
        ]
        for a, b, label in known_pairs:
            key = frozenset({a, b})
            n = pairs.get(key, 0)
            fa = files.get(a, 0); fb = files.get(b, 0)
            j = n / (fa + fb - n) if (fa + fb - n) else 0
            flag = "  <-- HIT" if n >= 3 else ""
            print(f"  {n:3d}/{n_commits}  J={j:.2f}  {label}{flag}")

        # Hidden deps: high co-change, NOT engine.rs (to find non-hub pairs)
        print("\nTop 10 non-engine co-change pairs:")
        non_eng = [(p, n) for p, n in pairs.most_common(200)
                   if "engine.rs" not in p]
        for pair, n in non_eng[:10]:
            a, b = sorted(pair)
            fa = files[a]; fb = files[b]
            j = n / (fa + fb - n) if (fa + fb - n) else 0
            print(f"  {n:3d}/{n_commits}  J={j:.2f}  {a}  <->  {b}")

        print()

if __name__ == "__main__":
    main()
