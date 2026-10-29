import json, re, sys, os, collections
rows=[json.loads(l) for l in open(sys.argv[1])]
depth={r['path']:r['depth'] for r in rows if r['record']=='stratum'}
cyc={p:r['scc'] for r in rows if r['record']=='stratum_cycle' for p in r['paths']}
EXEMPT={'lib.rs','mod.rs','main.rs'}
PREFIX=re.compile(r'^_(\d+)([a-z]?)_(.+)$')
# key of every node (file or dir) = depth for files, max descendant depth for dirs
key={}
for p,d in depth.items():
    if p.startswith('src/bin/'): continue
    key[p]=d
    parts=p.split('/')
    for i in range(2,len(parts)):
        dp='/'.join(parts[:i])
        key[dp]=max(key.get(dp,-1),d)
kids=collections.defaultdict(list)
for p in key:
    parent,name=p.rsplit('/',1)
    if name in EXEMPT: continue
    kids[parent].append(p)
rename={}
for parent,ps in kids.items():
    ranks=sorted({key[p] for p in ps})
    dense={k:i for i,k in enumerate(ranks)}
    groups=collections.defaultdict(list)
    for p in ps: groups[dense[key[p]]].append(p)
    for r,g in groups.items():
        g.sort(key=lambda p:(PREFIX.match(p.rsplit('/',1)[1]) and int(PREFIX.match(p.rsplit('/',1)[1]).group(1)) or 0, p))
        for i,p in enumerate(g):
            name=p.rsplit('/',1)[1]
            m=PREFIX.match(name); stem=m.group(3) if m else name
            letter='' if i==0 else chr(ord('a')+i-1)
            rename[p]=f'_{r}{letter}_{stem}'
# full new path = every segment renamed
def newpath(p):
    parts=p.split('/'); out=[parts[0]]
    for i in range(1,len(parts)):
        seg='/'.join(parts[:i+1]); out.append(rename.get(seg,parts[i]))
    return '/'.join(out)
moves=[(p,newpath(p)) for p in sorted(depth) if not p.startswith('src/bin/') and newpath(p)!=p]
dirs=[(p,rename[p]) for p in sorted(rename) if p not in depth and rename[p]!=p.rsplit('/',1)[1]]
print(f"files {len([p for p in depth if not p.startswith('src/bin/')])} moved {len(moves)} dirs renamed {len(dirs)}")
for p,n in dirs: print('DIR ',p,'->',n, 'key',key[p])
mode=sys.argv[2] if len(sys.argv)>2 else 'all'
for p,n in moves:
    if mode=='all' or mode in p: print(f"{depth[p]:>2} {'C'+str(cyc[p]) if p in cyc else '  ':>4} {p} -> {n}")
with open(sys.argv[1]+'.tsv','w') as f:
    for p,n in moves: f.write(f"{p}\t{n}\n")
