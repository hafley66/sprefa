#!/usr/bin/env python3
"""Pass 2: bin the 'direct' signal rows into gap classes from call-site shape."""
import re
import sys
import pathlib

CORPUS = pathlib.Path("/Users/chrishafley/projects/typescript-go")

# read defs/iface counts from signals pass is unnecessary; we re-derive quickly
recv_re = re.compile(r"^func \((\w+ )?\*?(\w+)\) (\w+)\(")


def call_lines(src_path, src_name, dst_name):
    p = CORPUS / src_path
    text = p.read_text(errors="replace").splitlines()
    # locate caller fn start crudely: last "func ... src_name(" before each hit
    out = []
    pat = re.compile(r"\." + re.escape(dst_name) + r"\(|\b" + re.escape(dst_name) + r"\(")
    fstarts = [(i, l) for i, l in enumerate(text) if l.startswith("func ")]
    for i, line in enumerate(text):
        if pat.search(line):
            # find enclosing func name
            fn = "?"
            for j in range(len(fstarts) - 1, -1, -1):
                if fstarts[j][0] <= i:
                    m = re.search(r"func (?:\([^)]*\) )?(\w+)", fstarts[j][1])
                    fn = m.group(1) if m else "?"
                    break
            if src_name.startswith("closure@") or (fn == src_name.split("$")[0]):
                out.append((i + 1, line.strip()))
    return out[:4]


def bin_row(src_name, dst_name, ndefs, iface, sites):
    # closure caller: vta names caller by enclosing fn$N; we mirror to enclosing name
    if "$" in src_name or src_name.startswith("closure@"):
        return "closure-caller-naming (mirrored, attr mismatch)"
    for ln, t in sites:
        # multi-hop: X.Y().Z( or X.Y().Z().W(
        if re.search(r"\.\w+\(\)\.\w+\(", t):
            return "multi-hop receiver chain (a.b().c())"
        # method value / func-typed field: assigned without call parens
        if re.search(r"\b" + re.escape(dst_name) + r"\s*[,}\)\]]", t) and not re.search(re.escape(dst_name) + r"\(", t):
            return "method value / func-typed field"
    if ndefs > 1 or iface == "y":
        return "interface method dispatch"
    return "MANUAL"


def main():
    for raw in sys.stdin:
        parts = raw.rstrip("\n").split("\t")
        dst_name, kind, nd, iface, sitetxt = parts[:5]
        src_path, src_name, dst_path = parts[5:8] if len(parts) >= 8 else (None, None, None)
        if kind == "closure":
            print("\t".join([dst_name, "vta-closure-callee (func literal, we drop)"]), flush=True)
            continue
        # re-find src info from original tsv is lost; caller/closure info in src_name not here.
        # fall back: parse from sitetxt impossible; expect env-provided src_name via arg
        src_name_env = sys.argv[1] if len(sys.argv) > 1 else ""
        sites = []
        for chunk in sitetxt.split(" || "):
            if ":" in chunk:
                ln, _, txt = chunk.partition(":")
                sites.append((ln, txt))
        b = bin_row(src_name_env, dst_name, int(nd), iface, sites)
        print("\t".join([dst_name, b]), flush=True)


if __name__ == "__main__":
    main()
