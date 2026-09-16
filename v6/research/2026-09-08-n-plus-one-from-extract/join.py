import json, re, sys
LINEAR = {"filter","find","findIndex","some","every","includes","indexOf","map","reduce","forEach","flatMap","sort","has_linear"}
for f in ["before","after"]:
    src = open(f"navTree.{f}.ts").read().encode()
    rows = [json.loads(l) for l in open(f"{f}.jsonl")]
    sites = {r["span"]["start"]: r for r in rows if r["record"] == "site"}
    loops = {(r["span"]["start"], r["span"]["end"]): r for r in rows if r["record"] == "df_loop"}
    line = lambda off: src[:off].count(b"\n") + 1
    hits = []
    for r in rows:
        if r["record"] != "df_nest": continue
        c = (r["call"]["start"], r["call"]["end"]); s = sites.get(c[0])
        if not s: continue
        callee = s.get("callee_path") or s["callee"]; text = src[c[0]:c[1]].decode()
        loop = loops[(r["loop"]["start"], r["loop"]["end"])]
        m = re.match(r"([\w.$\[\]()]+)\.(\w+)\(", text)
        recv = m.group(1) if m else None
        method = callee.split(".")[-1]
        invariant = recv is not None and recv != loop["var"] and recv != loop["collection"] and not (loop["var"] and recv.startswith(loop["var"] + "."))
        if method in LINEAR and invariant:
            hits.append((line(c[0]), loop["collection"], recv, method, text[:70].replace("\n"," ")))
    print(f"== {f}: {len(hits)} loop-invariant linear scans inside loops")
    for h in hits: print("  L%d  loop over %-24s  scans %-12s .%-8s %s" % h)
