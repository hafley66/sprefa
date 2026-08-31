#!/usr/bin/env python3
"""Normalize raw extract JSONL (resolve/diet_scip/scip family) into the
normal-form tsv: src_path src_name dst_path dst_name, paths relative to
corpus root."""
import json
import os
import sys


def relp(root, path):
    if path is None:
        return ""
    if path.startswith(root):
        rel = os.path.relpath(path, root)
        return rel
    return path


def resolved_to_tsv(raw_path, root, call_out, type_out, kind_filter=None):
    call_rows = []
    type_rows = []
    with open(raw_path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            rec = d["record"]
            if rec == "resolved_edge":
                if kind_filter and d["kind"] not in kind_filter:
                    continue
                call_rows.append("\t".join([
                    relp(root, d["caller_path"]), d.get("caller_name") or "",
                    relp(root, d["callee_path"]), d.get("callee_name") or "",
                ]))
            elif rec == "resolved_type_edge":
                if kind_filter and d["kind"] not in kind_filter:
                    continue
                type_rows.append("\t".join([
                    relp(root, d["owner_path"]), d.get("owner_name") or "",
                    relp(root, d["target_path"]), d.get("target_name") or "",
                ]))
    with open(call_out, "w") as f:
        f.write("\n".join(sorted(set(call_rows))) + "\n")
    with open(type_out, "w") as f:
        f.write("\n".join(sorted(set(type_rows))) + "\n")
    return len(set(call_rows)), len(set(type_rows))


def resolved_import_to_module_tsv(raw_path, root, out_path):
    # A module row carries no names, so `resolved_import`'s name/local/kind/hops
    # columns are dropped: the row answers "which file binds from which target".
    rows = set()
    with open(raw_path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            if d["record"] != "resolved_import":
                continue
            rows.add("\t".join([
                relp(root, d["src_path"]), "",
                relp(root, d["target_path"]), "",
            ]))
    with open(out_path, "w") as f:
        f.write("\n".join(sorted(rows)) + "\n")
    return len(rows)


def scip_to_call_tsv(scip_jsonl, root, out_path):
    sym_file = {}
    sym_name = {}
    fn_edges = []
    with open(scip_jsonl) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            d = json.loads(line)
            rec = d["record"]
            if rec == "scip_def":
                sym_file[d["symbol"]] = d["file"]
            elif rec == "scip_name":
                sym_name[d["symbol"]] = d["name"]
            elif rec == "scip_fn_edge":
                fn_edges.append((d["caller"], d["callee"]))
    rows = set()
    for caller, callee in fn_edges:
        cf = sym_file.get(caller)
        ef = sym_file.get(callee)
        if cf is None or ef is None:
            continue
        rows.add("\t".join([
            relp(root, cf), sym_name.get(caller, ""),
            relp(root, ef), sym_name.get(callee, ""),
        ]))
    with open(out_path, "w") as f:
        f.write("\n".join(sorted(rows)) + "\n")
    return len(rows)


if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "resolved":
        n_call, n_type = resolved_to_tsv(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5])
        print(f"call={n_call} type={n_type}")
    elif mode == "scip_call":
        n = scip_to_call_tsv(sys.argv[2], sys.argv[3], sys.argv[4])
        print(f"scip call edges (joined) = {n}")
    elif mode == "module":
        n = resolved_import_to_module_tsv(sys.argv[2], sys.argv[3], sys.argv[4])
        print(f"module={n}")
    elif mode == "resolved_filtered":
        n_call, n_type = resolved_to_tsv(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], kind_filter={sys.argv[6]})
        print(f"call={n_call} type={n_type}")
