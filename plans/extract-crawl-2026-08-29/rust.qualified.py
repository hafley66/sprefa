#!/usr/bin/env python3
"""Kink 4's corpus metric: resolved edges whose callee file disagrees with the
module path the call site names.

Inputs, produced by the battery driver in the lane scratch:
  sites.jsonl          `record=site` rows, one per call site, plus a `path` key
  resolve_edges.jsonl  the union of the per-crate `extract --resolve` runs

An edge is judged only when its site carries a MODULE-shaped `callee_path`
(every qualifier segment lowercase; `crate`/`self`/`super` excluded, as in the
report's section 9) AND some corpus file whose module path ends with that
qualifier defines the callee. Those two conditions make the expected file
computable from the corpus alone. Anything else is not counted either way.

Usage: rust.qualified.py <scratch-dir> <corpus-root>
"""
import json, os, sys, collections

SCRATCH = sys.argv[1] if len(sys.argv) > 1 else "."
CORPUS = (sys.argv[2] if len(sys.argv) > 2 else ".").rstrip("/") + "/"


def module_segments(path):
    stem = path[:-3] if path.endswith(".rs") else path
    segs = [s.replace("-", "_") for s in stem.split("/") if s and s != "." and s != "src"]
    if segs and segs[-1] in ("mod", "lib", "main"):
        segs.pop()
    return segs


def qualifier(callee_path):
    segs = callee_path.split("::")[:-1]
    if not segs:
        return None
    if segs[0] in ("crate", "self", "super"):
        return None
    if any(s[:1].isupper() for s in segs):
        return None
    return segs


# ------------------------------------------------------------------ load sites
# (caller file, site start) -> callee_path, the join key resolved_edge carries.
site_path = {}
for line in open(os.path.join(SCRATCH, "sites.jsonl")):
    o = json.loads(line)
    if o.get("record") != "site" or not o.get("callee_path"):
        continue
    site_path[(o["path"], o["span"]["start"])] = (o["callee_path"], o["callee"])

# --------------------------------------------------------- corpus module index
# callee name -> the set of files whose module path could host it.
defs_by_name = collections.defaultdict(set)
for line in open(os.path.join(SCRATCH, "defs.jsonl")):
    d = json.loads(line)
    if d.get("name"):
        defs_by_name[d["name"]].add(d["path"])

judged = wrong = 0
examples = []
for line in open(os.path.join(SCRATCH, "resolve_edges.jsonl")):
    o = json.loads(line)
    if o["record"] != "resolved_edge":
        continue
    caller = o["caller_path"].replace(CORPUS, "")
    key = (caller, o["caller_site_start"])
    if key not in site_path:
        continue
    written, callee = site_path[key]
    segs = qualifier(written)
    if segs is None:
        continue
    hosts = {p for p in defs_by_name.get(callee, ()) if module_segments(p)[-len(segs):] == segs}
    if not hosts:
        continue
    judged += 1
    got = o["callee_path"].replace(CORPUS, "")
    if got not in hosts:
        wrong += 1
        if len(examples) < 10:
            examples.append({"site": f"{caller}:{o['caller_site_start']}",
                             "written": written, "bound": got,
                             "expected": sorted(hosts)})

report = {"judged": judged, "wrong_file": wrong, "examples": examples}
json.dump(report, open(os.path.join(SCRATCH, "qualified.json"), "w"), indent=1)
print("judged edges     ", judged)
print("wrong-file edges ", wrong)
for ex in examples:
    print(" ", ex["site"], ex["written"], "->", ex["bound"], "expected", ex["expected"])
