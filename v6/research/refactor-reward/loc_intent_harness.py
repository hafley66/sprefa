#!/usr/bin/env python3
"""refactor-reward harness — batch pass/fail over sprefa's own refactor history.

Each refactor commit is one experiment:
  intent   = claim parsed from the commit subject (DOWN=consolidate, UP=split)
  reward   = measured net LOC delta across the .rs files the commit touched
  pass/fail = does reward sign match intent sign?

Output: per-commit table + per-class accuracy + the mismatches (the noise the
deterministic metric catches that the human label doesn't). Pure git, no dl,
so it runs in one shot over the whole history.

This is the policy-table backbone: which move-classes are reliably
reward-positive, measured not asserted.
"""
import re, subprocess, sys

ROOT = "/Users/chrishafley/projects/sprefa"

def git(args):
    return subprocess.run(["git", "-C", ROOT] + args,
                          capture_output=True, text=True, check=True).stdout

# verbs that signal a structural refactor worth measuring
SELECT = re.compile(
    r"\b(refactor|collapse|consolidat|dedup|inline|merge|split|extract|"
    r"break out|reuse|shar|delegate|rout(?:e|ing)|lift|intern|simplif|"
    r"factor out|parameteriz|prune|trim)\b", re.I)
# direction of expected net-LOC movement
DOWN = re.compile(
    r"\b(collapse|consolidat|dedup|inline|merge|reuse|shar|delegate|"
    r"rout(?:e|ing)|intern|simplif|prune|trim|drop|remov)\b", re.I)
UP = re.compile(
    r"\b(split|extract|break out|lift|parameteriz|introduc|expand)\b", re.I)

def numstat(sha):
    out = git(["show", "--no-renames", "--numstat", "--format=", sha])
    a = d = files = 0
    for line in out.splitlines():
        p = line.split("\t")
        if len(p) != 3 or p[0] == "-" or not p[2].endswith(".rs"):
            continue
        a += int(p[0]); d += int(p[1]); files += 1
    return a, d, files

def main():
    log = git(["log", "--no-merges", "--format=%H%x09%s", "-1500"])
    rows = []
    for line in log.splitlines():
        sha, subj = line.split("\t", 1)
        if not SELECT.search(subj):
            continue
        a, d, files = numstat(sha)
        if files == 0:
            continue
        net = a - d
        cls = "DOWN" if DOWN.search(subj) else ("UP" if UP.search(subj) else "?")
        if cls == "?":
            continue
        expect = -1 if cls == "DOWN" else 1
        # pass if measured sign matches expected (zero counts as fail)
        passed = (1 if net > 0 else -1 if net < 0 else 0) == expect
        rows.append((sha[:8], cls, a, d, net, files, passed, subj))

    down = [r for r in rows if r[1] == "DOWN"]
    up = [r for r in rows if r[1] == "UP"]
    def acc(rs): return f"{sum(r[6] for r in rs)}/{len(rs)}" if rs else "n/a"

    print(f"== refactor-reward pass/fail over sprefa history ==")
    print(f"selected/classified experiments: {len(rows)}  "
          f"(DOWN={len(down)} UP={len(up)})")
    print(f"accuracy  DOWN (consolidate→expect LOC↓): {acc(down)}")
    print(f"accuracy  UP   (split/extract→expect LOC↑): {acc(up)}")
    print(f"accuracy  ALL:  {acc(rows)}")
    print()
    print(f"{'sha':8} {'cls':4} {'+':>5} {'-':>5} {'net':>6} {'rs':>3} {'pf':3}  subject")
    for r in sorted(rows, key=lambda x: x[4]):
        print(f"{r[0]:8} {r[1]:4} {r[2]:>5} {r[3]:>5} {r[4]:>6} {r[5]:>3} "
              f"{'PASS' if r[6] else 'FAIL':4}  {r[7][:72]}")
    print()
    print("== mismatches (label said one direction, LOC measured the other) ==")
    for r in rows:
        if not r[6]:
            print(f"  {r[0]} {r[1]:4} net={r[4]:+d}  {r[7][:80]}")

if __name__ == "__main__":
    main()
