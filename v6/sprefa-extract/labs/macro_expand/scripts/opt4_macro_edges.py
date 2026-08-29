#!/usr/bin/env python3
# Count scip_fn_edge-related call occurrences that sit inside a macro
# invocation span. A "call occurrence" is a scip_occurrence with role
# read (not def/import) whose symbol appears as the callee of some
# scip_fn_edge row.
import json, sys, collections

LOG = sys.argv[1] if len(sys.argv) > 1 else "opt4.scip.jsonl"
SPANS = sys.argv[2] if len(sys.argv) > 2 else "opt1.macro_spans.tsv"

macro_spans = collections.defaultdict(list)
for line in open(SPANS):
    f, s, e = line.rstrip("\n").split("\t")
    macro_spans[f].append((int(s), int(e)))

fn_edges = set()
occ = []
for line in open(LOG):
    r = json.loads(line)
    rec = r.get("record")
    if rec == "scip_fn_edge":
        fn_edges.add(r["callee"])
    elif rec == "scip_occurrence":
        occ.append(r)

call_occ = [o for o in occ if o["read_access"] and not o["definition"] and not o["import"] and o["symbol"] in fn_edges]
inside = 0
outside = 0
inside_rows = []
for o in call_occ:
    f = o["path"]
    s, e = o["start"], o["end"]
    hit = any(a <= s and e <= b for (a, b) in macro_spans.get(f, ()))
    if hit:
        inside += 1
        if len(inside_rows) < 10:
            inside_rows.append((f, s, e, o["symbol"], o.get("text") or ""))
    else:
        outside += 1
print("fn_edge callee symbols:", len(fn_edges))
print("call occurrences:", len(call_occ))
print("inside macro invocation spans:", inside)
print("outside:", outside)
for r in inside_rows:
    print("  ", r)
