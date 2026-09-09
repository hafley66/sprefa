import json, subprocess, sys, glob
from collections import defaultdict, deque
entry, sink = sys.argv[1], sys.argv[2]
files = sorted(glob.glob("*.rs"))
def facts(args):
    out = subprocess.run(["extract", *args], capture_output=True, text=True).stdout
    return [json.loads(l) for l in out.splitlines() if l]
fn_at = {}          # (file, name) -> span
cfg_test = defaultdict(list)   # file -> spans guarded by cfg(test)
sites = defaultdict(list)      # file -> [(start, callee)]
nest = defaultdict(list)       # file -> [call spans inside loops]
for f in files:
    for r in facts(["--family", "call,df", f]):
        if r["record"] == "node" and r["family"] == "call" and r["kind"] in ("function", "method"):
            fn_at[(f, r["name"])] = r["span"]
        elif r["record"] == "site":
            sites[f].append((r["span"]["start"], r["callee"]))
        elif r["record"] == "cfg_scope" and "test" in r["cfg"]:
            cfg_test[f].append(r["span"])
        elif r["record"] == "df_nest":
            nest[f].append(r["call"]["start"])
def in_test(f, off): return any(s["start"] <= off < s["end"] for s in cfg_test[f])
def owner(f, off):
    best = None
    for (ff, name), sp in fn_at.items():
        if ff == f and sp["start"] <= off < sp["end"] and (best is None or sp["start"] > fn_at[best]["start"]): best = (ff, name)
    return best
edges = defaultdict(list)      # (file, fn) -> [((file, fn), in_loop)]
for r in facts(["--resolve", "--family", "call", *files]):
    if r["record"] != "resolved_edge": continue
    src = (r["caller_path"], r["caller_name"]); dst = (r["callee_path"], r["callee_name"])
    if dst not in fn_at or in_test(*src[:1], r["caller_site_start"]): continue
    edges[src].append((dst, r["caller_site_start"] in set(nest[r["caller_path"]])))
sinks = {o for f in files for (off, callee) in sites[f] if callee == sink and (o := owner(f, off))}
starts = [k for k in fn_at if k[1] == entry]
paths = []
def walk(node, path, looped):
    if node in sinks and looped: paths.append(path); return
    for dst, in_loop in edges[node]:
        if dst in {p[0] for p in path}: continue
        walk(dst, path + [(dst, in_loop)], looped or in_loop)
for s in starts: walk(s, [(s, False)], False)
seen = set()
for p in paths:
    key = tuple(n for n, _ in p)
    if key in seen: continue
    seen.add(key)
    print(" -> ".join(f"{n[1]}{' [LOOP]' if l else ''}" for n, l in p) + f"  -> {sink}()")
print(f"{len(seen)} distinct loop-reaching paths from {entry} to {sink}", file=sys.stderr)
