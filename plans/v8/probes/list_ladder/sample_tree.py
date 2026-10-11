import re,sys
lines=open(sys.argv[1]).read().split("\n")
target=sys.argv[2]; maxdepth=int(sys.argv[3]); mincount=int(sys.argv[4])
start=next(i for i,l in enumerate(lines) if l.startswith("Call graph:"))
end=next(i for i,l in enumerate(lines) if "Total number in stack" in l)
pat=re.compile(r"^([\s+!:|]*)(\d+) (.+?)  \(in ([^)]+)\)")
def short(f):
    names=re.findall(r"\d+([A-Za-z_][A-Za-z0-9_]*)", f)
    s="::".join(n for n in names if not re.match(r"^(Cs|Nt|B[0-9a-z]|Ns|Nv|Mt|Ms|INt|RN|Cs)",n))
    return (s or f)[:120]
rows=[]
for l in lines[start+1:end]:
    m=pat.match(l)
    if m: rows.append((len(m.group(1)),int(m.group(2)),m.group(3),m.group(4)))
i=next(i for i,r in enumerate(rows) if target in r[2])
d0=rows[i][0]
print(rows[i][1], short(rows[i][2]))
for d,n,f,lib in rows[i+1:]:
    if d<=d0: break
    rel=(d-d0)//2
    if rel<=maxdepth and n>=mincount:
        print("  "*rel+str(n), short(f), "" if lib=="dl8" else "["+lib+"]")
