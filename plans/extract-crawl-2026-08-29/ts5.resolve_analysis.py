#!/usr/bin/env python3
"""How many ambiguous sites become unique once the named import plus the barrel
closure from --deps is used to pick among the same-name defs."""
import json, collections

S = "/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/crawl-ts5"
P = "/Users/chrishafley/projects/TypeScript-5.9/"

edges = collections.defaultdict(dict)      # src -> module -> dst
reexport = collections.defaultdict(list)   # barrel file -> [dst]
for l in open(S + "/src.deps.jsonl"):
    o = json.loads(l)
    if o["record"] == "file_edge":
        if o["kind"] == "reexport":
            reexport[o["src_path"]].append(o["dst_path"])

mod_of = {}   # (src, module string) -> dst
sites, defs, imports = [], collections.defaultdict(list), collections.defaultdict(dict)
for l in open(S + "/src.call.jsonl"):
    o = json.loads(l)
    r = o["record"]
    if r == "site":
        sites.append((o["path"], o["span"]["start"], o["callee"]))
    elif r == "node":
        defs[o["name"]].append((o["path"], o["kind"]))
    elif r == "specifier" and o["kind"] == "named":
        imports[o["path"]][o["name"]] = o["module"]

# module string -> file, rebuilt from the deps graph by matching the NodeNext rewrite
import os
allfiles = set(l.strip() for l in open(S + "/files_src.txt"))


def resolve(src, mod):
    if not mod.startswith("."):
        return None
    base = os.path.normpath(os.path.join(os.path.dirname(src), mod))
    for cand in (base[:-3] + ".ts" if base.endswith(".js") else None, base + ".ts",
                 base, os.path.join(base, "index.ts")):
        if cand and cand in allfiles:
            return cand
    return None


def barrel_closure(f, seen=None):
    seen = seen or set()
    if f in seen:
        return set()
    seen.add(f)
    out = {f}
    for d in reexport.get(f, ()):
        out |= barrel_closure(d, seen)
    return out


matched = set()
for l in open(S + "/src.resolve.jsonl"):
    o = json.loads(l)
    if o["record"] == "resolved_edge":
        matched.add((o["caller_path"].replace(P, ""), o["caller_site_start"]))

win = collections.Counter()
examples = collections.defaultdict(list)
for path, st, callee in sites:
    if (path, st) in matched:
        continue
    cands = defs.get(callee)
    if not cands or len(set(c[0] for c in cands)) < 2:
        continue
    mod = imports.get(path, {}).get(callee)
    if mod is None:
        win["ambiguous, name not imported here"] += 1
        continue
    tgt = resolve(path, mod)
    if tgt is None:
        win["ambiguous, module outside the universe"] += 1
        continue
    scope = barrel_closure(tgt)
    hits = sorted({c[0] for c in cands if c[0] in scope})
    if len(hits) == 1:
        win["RECOVERABLE: import narrows to one def"] += 1
        if len(examples["r"]) < 12:
            examples["r"].append((path, st, callee, mod, hits[0]))
    elif len(hits) == 0:
        win["import target holds no def of the name"] += 1
    else:
        win["still ambiguous inside the barrel closure"] += 1
        if len(examples["s"]) < 8:
            examples["s"].append((path, st, callee, mod, hits))

tot = sum(win.values())
print("=== ambiguous sites, re-judged with the import + barrel closure")
for k, v in win.most_common():
    print(f"  {v:6d} {v/tot*100:5.1f}%  {k}")
print("  total", tot)
print("\nrecoverable examples:")
for e in examples["r"]:
    print("   ", e)
print("\nstill-ambiguous examples:")
for e in examples["s"]:
    print("   ", e)
