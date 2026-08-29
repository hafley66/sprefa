#!/usr/bin/env python3
# Minimal protobuf wire walker over the raw .scip index: pull every
# non-def occurrence (range + symbol) per document, convert ranges to
# byte offsets, and count how many sit inside a macro invocation span.
import sys, collections, json

def read_varint(buf, i):
    shift = 0; val = 0
    while True:
        b = buf[i]; i += 1
        val |= (b & 0x7F) << shift
        if not b & 0x80: return val, i
        shift += 7

def fields(buf):
    i = 0; n = len(buf)
    while i < n:
        key, i = read_varint(buf, i)
        fno, wt = key >> 3, key & 7
        if wt == 0:
            v, i = read_varint(buf, i)
            yield fno, wt, v
        elif wt == 2:
            ln, i = read_varint(buf, i)
            yield fno, wt, buf[i:i+ln]
            i += ln
        elif wt == 5: yield fno, wt, buf[i:i+4]; i += 4
        elif wt == 1: yield fno, wt, buf[i:i+8]; i += 8
        else: raise ValueError(wt)

def range_to_bytes(r, line_starts, text_len):
    # SingleLineRange: (line, start_char, end_char); rust-analyzer emits
    # UTF-8 byte columns for Rust (position encoding UTF8)
    if len(r) == 3:
        line, sc, ec = r
        sl = line_starts[line]
        start = sl + sc
        end = sl + ec
        return start, end
    # 4 elems: [startLine, startChar, endLine, endChar]
    sl0, sc, el0, ec = r
    start = line_starts[sl0] + sc
    end = line_starts[el0] + ec
    return start, end

INDEX = sys.argv[1]
SPANS = sys.argv[2]
RA = sys.argv[3]

macro_spans = collections.defaultdict(list)
for line in open(SPANS):
    f, s, e = line.rstrip("\n").split("\t")
    macro_spans[f].append((int(s), int(e)))

data = open(INDEX, "rb").read()
i = 0
line_starts_cache = {}
occ_counts = collections.Counter()

def line_starts_for(path):
    if path in line_starts_cache: return line_starts_cache[path]
    t = open(RA + "/" + path, "rb").read()
    starts = [0]
    for j, b in enumerate(t):
        if b == 0x0A: starts.append(j + 1)
    line_starts_cache[path] = starts
    return starts

callees = set()
try:
    for line in open("opt4.scip.jsonl"):
        r = json.loads(line)
        if r.get("record") == "scip_fn_edge":
            callees.add(r["callee"])
except FileNotFoundError:
    pass

docs = 0
for fno, wt, val in fields(data):
    if fno != 2 or wt != 2: continue
    docs += 1
    path = None; occs = []
    for f2, w2, v2 in fields(val):
        if f2 == 1 and w2 == 2: path = v2.decode()
        elif f2 == 2 and w2 == 2:
            rng = None; sym = None; roles = 0
            for f3, w3, v3 in fields(v2):
                if f3 == 1 and w3 == 2:
                    r = []; j = 0
                    while j < len(v3):
                        x, j = read_varint(v3, j); r.append(x)
                    rng = r
                elif f3 == 2 and w3 == 2: sym = v3.decode()
                elif f3 == 3 and w3 == 0: roles = v3
            if sym is None or rng is None: continue
            if roles & 0x1 or roles & 0x2: continue  # def or import
            occs.append((rng, sym, roles))
    if not path or not occs: continue
    starts = None
    for rng, sym, roles in occs:
        if starts is None: starts = line_starts_for(path)
        try:
            s, e = range_to_bytes(rng, starts, 0)
        except Exception:
            occ_counts["range_error"] += 1
            continue
        hit = any(a <= s and e <= b for (a, b) in macro_spans.get(path, ()))
        occ_counts["total_refs"] += 1
        if hit:
            occ_counts["inside_macro"] += 1
            if sym in callees:
                occ_counts["inside_macro_fnedge_callee"] += 1
print("documents:", docs)
print("fn_edge callee symbols:", len(callees))
for k, v in occ_counts.items(): print(k, v)
