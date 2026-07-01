#!/usr/bin/env python3
"""refactor-reward v4 — cross-type method dup (text-approx of scip_callee_type).

v3 residue: 635de156 'collapse node/edge mirrors into generic GraphRef<K,I>'.
fn-dup and type-def both mislabel it (+6, +22) because parameterization ADDS
boilerplate. The real reward is that the SAME METHOD NAMES used to live on two
mirror types and now live on one generic. That's the scip_callee_type signal
(method -> receiver). Approximated in text: count method-names implemented
under >=2 distinct impl-targets within the touched set; collapse-into-generic
drops it. If this cracks the residue, lexical structural ~= 100% on genuine
hard-collapse commits, and the semantic bridge is only needed for richer
state-shapes, not for reward.
"""
import re, subprocess, sys
from collections import defaultdict, Counter

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/Users/chrishafley/projects/sprefa"
def git(a): return subprocess.run(["git","-C",ROOT]+a,capture_output=True,text=True).stdout
def git_ok(a):
    r=subprocess.run(["git","-C",ROOT]+a,capture_output=True,text=True)
    return r.stdout if r.returncode==0 else None

HARD_DOWN = re.compile(r"\b(collapse|consolidat|merge|dedup|deduplic)\b", re.I)
TYP   = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait)\s+\w+|^impl\b", re.M)
FN_NM = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?(?:const\s+)?fn ([a-z_][a-z0-9_]*)\s*[<(]", re.M)
IMPL_FOR = re.compile(r"^\s*impl\b.*?\bfor\s+([A-Za-z_][\w:]*)")
IMPL_INH = re.compile(r"^\s*impl(?:<[^>]*>)?\s+([A-Za-z_][\w:]*)\s*(?:<[^>]*>)?\s*\{")
FNLINE   = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?(?:const\s+)?fn ([a-z_][a-z0-9_]*)\s*[<(]")
TOPCLOSE = re.compile(r"^\}")

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
    """return (within_file_fn_dup_excess, type_def_count, cross_type_method_dup_excess)."""
    blob = git_ok(["show", f"{rev}:{path}"])
    if blob is None: return 0,0,0
    # within-file fn dup
    fn_dup = sum(v-1 for v in Counter(FN_NM.findall(blob)).values() if v>=2)
    # type defs
    type_defs = len(TYP.findall(blob))
    # cross-type method dup: methods (name -> set of impl targets) at top level
    method_on = defaultdict(set)
    target = None
    for line in blob.splitlines():
        if TOPCLOSE.match(line):                # top-level close ends current impl
            target = None; continue
        m = IMPL_FOR.match(line) or None
        if not m:
            m2 = IMPL_INH.match(line)
            if m2: m = m2
        if m:
            target = m.group(1).split("::")[-1]
            continue
        if target:
            mf = FNLINE.match(line)
            if mf: method_on[mf.group(1)].add(target)
    cross = sum(len(s)-1 for s in method_on.values() if len(s)>=2)
    return fn_dup, type_defs, cross

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
        b=tuple(sum(x) for x in zip(*(counts(parent,f) for f in fs)))
        a=tuple(sum(x) for x in zip(*(counts(sha,f)     for f in fs)))
        fn_d, typ_d, cross_d = a[0]-b[0], a[1]-b[1], a[2]-b[2]
        comb = (fn_d<=0) or (typ_d<=0) or (cross_d<=0)
        rows.append(dict(sha=sha[:8], loc=loc_d, fn=fn_d, typ=typ_d, cross=cross_d,
                         p_loc=loc_d<0, p_fn=fn_d<=0, p_typ=typ_d<=0, p_cross=cross_d<=0,
                         comb=comb, subj=subj))

    n=len(rows)
    def rate(k):
        c=sum(r[k] for r in rows); return f"{c}/{n} = {100*c/n:.0f}%"
    # genuine = exclude the merge-conflict commit
    genuine=[r for r in rows if "merge conflict" not in r['subj'].lower()]
    g=len(genuine)
    def grate(k):
        c=sum(r[k] for r in genuine); return f"{c}/{g} = {100*c/g:.0f}%"

    print(f"== hard-collapse: {n} commits (genuine refactors: {g}) ==")
    print(f"  LOC               (net LOC↓):              {rate('p_loc')}")
    print(f"  fn-dup            (within-file fn-dup↓):   {rate('p_fn')}")
    print(f"  type-def          (type-def count↓):       {rate('p_typ')}")
    print(f"  cross-type-method (method on >=2 types↓):  {rate('p_cross')}")
    print(f"  COMBINED lexical  (any structural↓):       {rate('comb')}")
    print(f"  COMBINED on GENUINE refactors only:        {grate('comb')}")
    print()
    print(f"{'sha':8} {'loc':>6} {'fnD':>5} {'typD':>5} {'crsD':>5}  LOC fn typ crs COMB  subject")
    for r in sorted(rows,key=lambda x:x['loc']):
        print(f"{r['sha']:8} {r['loc']:>+6} {r['fn']:>+5} {r['typ']:>+5} {r['cross']:>+5}  "
              f"{'P' if r['p_loc'] else 'F':3}{'P' if r['p_fn'] else 'F':3}{'P' if r['p_typ'] else 'F':3}"
              f"{'P' if r['p_cross'] else 'F':3} {'PASS' if r['comb'] else 'FAIL':4}  {r['subj'][:54]}")
    print()
    miss=[r for r in rows if not r['comb']]
    print(f"== still fails COMBINED: {len(miss)} ==")
    for r in miss:
        print(f"  {r['sha']} loc={r['loc']:+d} fn={r['fn']:+d} typ={r['typ']:+d} crs={r['cross']:+d}  {r['subj'][:66]}")
    if not miss: print("  (none — lexical structural reward solved for this class)")

if __name__=="__main__": main()
