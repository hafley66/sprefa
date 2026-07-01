#!/usr/bin/env python3
"""refactor-reward v2 — LOC vs STRUCTURAL reward on consolidation commits.

Hypothesis (from v1): raw net-LOC mislabels consolidation refactors because
collapses often grow boilerplate (generics, trait plumbing) while removing
duplication. A STRUCTURAL reward — redundant fn-name copies within a file —
should track the actual de-duplication and beat LOC on the consolidation class.

For each hard-collapse commit (collapse|consolidat|merge|dedup only), measure
at parent vs commit:
  loc_d   = net LOC delta of touched .rs files                 (crude reward)
  dup_d   = delta of redundant fn-name copies within-file      (structural reward)
pass_loc = loc_d < 0 ; pass_dup = dup_d <= 0
Report per-class pass-rate. If dup beats loc on collapses, structural metrics
are the better reward signal — the sprefa justification.
"""
import re, subprocess
from collections import Counter

ROOT = "/Users/chrishafley/projects/sprefa"
def git(a):
    return subprocess.run(["git","-C",ROOT]+a,capture_output=True,text=True).stdout
def git_ok(a):
    r = subprocess.run(["git","-C",ROOT]+a,capture_output=True,text=True)
    return r.stdout if r.returncode==0 else None

HARD_DOWN = re.compile(r"\b(collapse|consolidat|merge|dedup|deduplic)\b", re.I)
FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?(?:const\s+)?fn ([a-z_][a-z0-9_]*)\s*[<(]", re.M)

def files_of(sha):
    out = git(["show","--no-renames","--numstat","--format=",sha])
    fs=[]
    for line in out.splitlines():
        p=line.split("\t")
        if len(p)==3 and p[0]!="-" and p[2].endswith(".rs"): fs.append(p[2])
    return fs

def numstat(sha):
    a=d=0
    for line in git(["show","--no-renames","--numstat","--format=",sha]).splitlines():
        p=line.split("\t")
        if len(p)==3 and p[0]!="-" and p[2].endswith(".rs"):
            a+=int(p[0]); d+=int(p[1])
    return a-d

def dup_excess(rev, path):
    blob = git_ok(["show", f"{rev}:{path}"])
    if blob is None: return 0
    c = Counter(FN.findall(blob))
    return sum(v-1 for v in c.values() if v>=2)   # redundant copies

def main():
    log = git(["log","--no-merges","--format=%H%x09%s","-1500"])
    rows=[]
    for line in log.splitlines():
        sha,subj=line.split("\t",1)
        if not HARD_DOWN.search(subj): continue
        fs=files_of(sha)
        if not fs: continue
        parent=f"{sha}^"
        loc_d = numstat(sha)           # wrong: need parent vs commit. compute below
        # loc delta: net(add-del) of the commit itself IS parent->commit
        loc_d = numstat_between(parent,sha)
        dup_before=sum(dup_excess(parent,f) for f in fs)
        dup_after =sum(dup_excess(sha,f)     for f in fs)
        dup_d = dup_after - dup_before
        rows.append((sha[:8],len(fs),loc_d,dup_before,dup_after,dup_d,
                     loc_d<0, dup_d<=0, subj))

    n=len(rows)
    loc_pass=sum(r[6] for r in rows)
    dup_pass=sum(r[7] for r in rows)
    print(f"== hard-collapse commits: {n} ==")
    print(f"LOC reward    pass (net LOC↓):      {loc_pass}/{n} = {100*loc_pass/n:.0f}%")
    print(f"STRUCT reward pass (dup-excess↓):   {dup_pass}/{n} = {100*dup_pass/n:.0f}%")
    print()
    print(f"{'sha':8} {'rs':>3} {'loc_d':>7} {'dup-':>4} {'dup+':>4} {'dup_d':>6} "
          f"{'LOC':4} {'DUP':4}  subject")
    for r in sorted(rows,key=lambda x:x[5]):
        print(f"{r[0]:8} {r[1]:>3} {r[2]:>7} {r[3]:>4} {r[4]:>4} {r[5]:>6} "
              f"{'PASS' if r[6] else 'FAIL':4} {'PASS' if r[7] else 'FAIL':4}  {r[8][:64]}")
    print()
    print("== where structural beats LOC (FAIL loc, PASS dup) ==")
    for r in rows:
        if not r[6] and r[7]:
            print(f"  {r[0]} loc_d={r[2]:+d} dup_d={r[5]:+d}  {r[8][:72]}")
    print("\n== where BOTH fail (neither metric sees reward) ==")
    for r in rows:
        if not r[6] and not r[7]:
            print(f"  {r[0]} loc_d={r[2]:+d} dup_d={r[5]:+d}  {r[8][:72]}")

def numstat_between(a,b):
    a_,d_=0,0
    for line in git(["diff","--no-renames","--numstat",a,b]).splitlines():
        p=line.split("\t")
        if len(p)==3 and p[0]!="-" and p[2].endswith(".rs"):
            a_+=int(p[0]); d_+=int(p[1])
    return a_-d_

if __name__=="__main__": main()
