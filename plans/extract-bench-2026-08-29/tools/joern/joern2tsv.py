#!/usr/bin/env python3
"""joern 6-column dump -> normal form (src_path src_name dst_path dst_name).

Input columns, tab separated, as written by go_calls2.sc / ts_calls2.sc:
  caller_file caller_name caller_fullname callee_file callee_name callee_fullname

Name conventions match the compiler oracles:
  ts  `<module>` for a top-level call site, nearest named enclosing function
      otherwise (joern spells those `:program`, `<lambda>N`, `anonymous`).
  go  bare function name, receiver type stripped (joern spells a method
      `pkg.Type.Method` or `(*Type).Method`).
"""
import argparse
import sys

TS_ANON = ("<lambda>", "anonymous", "<global>")


def strip_root(path, root, prefix):
    if root and path.startswith(root):
        path = path[len(root):]
    path = path.lstrip("/")
    if prefix and not path.startswith(prefix):
        path = prefix + path
    return path


def ts_name(name, fullname):
    parts = [p for p in fullname.split(":") if p]
    named = []
    for part in parts:
        if part.endswith(".ts") or part.endswith(".js") or "/" in part:
            continue
        if part == "program" or part.startswith(TS_ANON):
            continue
        named.append(part)
    if name and name != ":program" and not name.startswith(TS_ANON):
        return name
    if named:
        return named[-1]
    return "<module>"


def go_name(name, fullname):
    bare = name
    if "." in bare:
        bare = bare.rsplit(".", 1)[-1]
    return bare.strip("()*")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lang", choices=["go", "ts"], required=True)
    ap.add_argument("--root", default="")
    ap.add_argument("--prefix", default="")
    ap.add_argument("--input", required=True)
    args = ap.parse_args()

    rows = set()
    ext = ".go" if args.lang == "go" else ".ts"
    with open(args.input) as fh:
        for line in fh:
            cols = line.rstrip("\n").split("\t")
            if len(cols) != 6:
                continue
            src_file, src_name, src_full, dst_file, dst_name, dst_full = cols
            if not src_file or not dst_file:
                continue
            # joern names a package-level init pseudo-method after its package,
            # not after a file; those rows have no counterpart in the oracles.
            if not src_file.endswith(ext) or not dst_file.endswith(ext):
                continue
            namer = ts_name if args.lang == "ts" else go_name
            rows.add(
                "\t".join(
                    [
                        strip_root(src_file, args.root, args.prefix),
                        namer(src_name, src_full),
                        strip_root(dst_file, args.root, args.prefix),
                        namer(dst_name, dst_full),
                    ]
                )
            )
    for row in sorted(rows):
        sys.stdout.write(row + "\n")


if __name__ == "__main__":
    main()
