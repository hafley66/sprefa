#!/usr/bin/env python3
"""refactor-reward v3 — add type-level structural metric; combined reward.

v2: within-file fn-name dup-excess beats LOC 73% vs 36% on hard-collapse, but
missed 2 cross-file/type collapses (generic GraphRef, store merge). Those are
TYPE-LEVEL de-duplications. A type-def-count metric should see them.

Signals per commit (parent vs commit, over touched .rs files):
  loc_d      net LOC
  fn_dup_d   within-file fn-name dup-excess Δ        (v2 structural)
  type_def_d (struct|enum|trait|impl) line count Δ   (NEW type-level)
Combined structural = pass if EITHER structural signal shows de-dup (<= 0).
"""
import re, subprocess
from collections import Counter

ROOT = "/Users/chrishafley/projects/sprefa"
def git(a): return subprocess.run(["git","-C",ROOT]+a,capture_output=True,text=True).stdout
def git_ok(a):
    r=subprocess.run(["git","-C",ROOT]+a,capture_output=True,text=True)
    return r.stdout if r.returncode==0 else None

HARD_DOWN = re.compile(r"\b(collapse|consolidat|merge|dedup|deduplic)\b", re.I)
TYP = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait)\s+\w+|^impl\b", re.M)

def files_of(sha):
    fs=[]
    for line in git(["show","--no-renames","--numstat","--format=",sha]).splitlines():
        p=line.split("\t")
        if len(p)==3 and p[0]!="-" and p[2].endswith(".rs"): fs.append(p[2])
    return fs

def netloc(a,b):
    ad=dl=0
    for line in git(["diff","--no-renames","--numstat",a,b]).splitlines():
        p=line.split("\t")
        if len(p)==3 and p[0]!="-" and p[2].endswith(".rs"): ad+=int(p[0]); dl+=int(p[1])
    return ad-dl

def counts(rev, path):
    blob = git_ok(["show", f"{rev}:{path}"])
    if blob is None: return 0,0
    fn_names = re.findall(r"^\s*(?:pub\s+)?(?:async\s+)?(?:const\s+)?fn ([a-z_][a-z0-9_]*)\s*[<(]", blob, re.M)
    fn_dup = sum(v-1 for v in Counter(fn_names).values() if v>=2)
    type_defs = len(TYP.findall(blob))
    return fn_dup, type_defs

def main():
    log = git(["log","--no-merges","--format=%H%x09%s","-1500"])
    rows=[]
    for line in log.splitlines():
        sha,subj=line.split("\t",1)
        if not HARD_DOWN.search(subj): continue
        fs=files_of(sha)
        if not fs: continue
        parent=f"{sha}^"
        loc_d = netloc(parent,sha)
        fb=tb=0; fa=ta=0
        for f in fs:
            x,y=counts(parent,f); fb+=x; tb+=y
            x,y=counts(sha,f);     fa+=x; ta+=y
        fn_dup_d   = fa-fb
        type_def_d = ta-tb
        rows.append(dict(sha=sha[:8], n=len(fs), loc_d=loc_d,
                         fn_dup_d=fn_dup_d, type_def_d=type_def_d,
                         loc=loc_d<0, fn=fn_dup_d<=0, typ=type_def_d<=0,
                         comb=(fn_dup_d<=0) or (type_def_d<=0),
                         subj=subj))

    n=len(rows)
    def rate(key): 
        c=sum(r[key] for r in rows)
        return f"{c}/{n} = {100*c/n:.0f}%"
    print(f"== hard-collapse: {n} commits, 4 reward signals ==")
    print(f"  LOC               (net LOC↓):                {rate('loc')}")
    print(f"  fn-dup            (within-file fn-dup↓):     {rate('fn')}")
    print(f"  type-def          (struct/enum/trait/impl↓): {rate('typ')}")
    print(f"  COMBINED struct   (fn-dup OR type-def↓):     {rate('comb')}")
    print()
    print(f"{'sha':8} {'loc_d':>7} {'fn_dup_d':>9} {'type_def_d':>11}  "
          f"{'LOC':4}{'fnD':4}{'typ':4}{'COMB':5}  subject")
    for r in sorted(rows, key=lambda x:x['loc_d']):
        print(f"{r['sha']:8} {r['loc_d']:>+7} {r['fn_dup_d']:>+9} {r['type_def_d']:>+11}  "
              f"{'P' if r['loc'] else 'F':4}{'P' if r['fn'] else 'F':4}{'P' if r['typ'] else 'F':4}"
              f"{'PASS' if r['comb'] else 'FAIL':5}  {r['subj'][:58]}")
    print()
    print("== still fails COMBINED (the unsolved residue) ==")
    miss=[r for r in rows if not r['comb']]
    for r in miss:
        print(f"  {r['sha']} loc={r['loc_d']:+d} fn={r['fn_dup_d']:+d} typ={r['type_def_d']:+d}  {r['subj'][:70]}")
    if not miss: print("  (none)")

if __name__=="__main__": main()
