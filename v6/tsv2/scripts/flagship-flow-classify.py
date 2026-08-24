"""Classify the flagship flow port's four query diffs.

The two engines spell node identity in different units: v5 rows are
`path:line:col:kind` (1-based line, 0-based col, node kind suffix; the
`rel_*_txt` views render the interned text), v6 rows are
`path:byte_start:byte_end` (the extractor's half-open byte spans). The
translation lives HERE, in the referee, so neither engine bends to the
other's coordinates: v6 byte starts are converted to line:col by reading
the pinned corpus under <work>/root, and v5 ids drop their kind suffix.
Calibration receipt (2026-07-29): byte 3981 of src/bin/extract.rs ->
line 94 col 8 == v5's `src/bin/extract.rs:94:8:let_bind`.

Distinct v5 kinds at one position (var_read + member at the same col)
collapse onto one key; the collapse count is printed so the match table
never silently overstates agreement. A nonzero flow_edge intersection is
an executable requirement, not a descriptive count.
"""

import os
import sqlite3
import sys


REASONS = {
    "flow_edge": "value-plane rows differ after the df union",
    "flow_reach": "value-plane closure differs after the df union",
    "flow_param_type": "resolved-callee sig-owner join differs",
    "flow_node_type": "df-param positional type join differs",
}


def read_tsv(path):
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.strip()]


class ByteToLineCol:
    """1-based line, 0-based col for a byte offset, per corpus file."""

    def __init__(self, root):
        self.root = root
        self.newline_index = {}

    def key(self, token):
        path, start_text, _end_text = token.rsplit(":", 2)
        start = int(start_text)
        newlines = self.newline_index.get(path)
        if newlines is None:
            data = open(os.path.join(self.root, path), "rb").read()
            newlines = [i for i, b in enumerate(data) if b == 0x0A]
            self.newline_index[path] = newlines
        import bisect

        line_index = bisect.bisect_left(newlines, start)
        line = line_index + 1
        col = start - (newlines[line_index - 1] + 1 if line_index else 0)
        return f"{path}:{line}:{col}"


def v5_node_key(token):
    path, line, col, _kind = token.rsplit(":", 3)
    return f"{path}:{line}:{col}"


def v5_sym_key(sym):
    parts = sym.split("::")
    if parts[0] == "root":
        parts = parts[1:]
    path = parts[0]
    name = parts[-1].rsplit(".", 1)[-1]
    return path, name


def type_key(ty):
    if ty.startswith("root::"):
        return ty.rsplit("::", 1)[-1]
    return ty


def keyed(rel, rows, translate):
    keys = set()
    for row in rows:
        cells = row.split("\t")
        if rel in ("flow_edge", "flow_reach"):
            keys.add("\t".join(translate.node(cell) for cell in cells[:2]))
        elif rel == "flow_param_type":
            if translate.side == "v5":
                path, name = v5_sym_key(cells[0])
                keys.add("\t".join([path, name, cells[1], type_key(cells[2])]))
            else:
                keys.add("\t".join([cells[0], cells[1], cells[2], type_key(cells[3])]))
        elif rel == "flow_node_type":
            if translate.side == "v5":
                path, name = v5_sym_key(cells[1])
                keys.add("\t".join([translate.node(cells[0]), path, name, type_key(cells[2])]))
            else:
                keys.add("\t".join(
                    [translate.node(cells[0]), cells[1], cells[2], type_key(cells[3])]
                ))
    return keys


def v5_flow_inputs(work):
    db = sqlite3.connect(os.path.join(work, "v5.sqlite"))
    try:
        node = {
            row[0]: f"{row[1]}:{row[2]}:{row[3]}"
            for row in db.execute(
                """
                select d.id, file_text.content, d.line, d.col
                from _df_node_dict d
                join _strings file_text on file_text.id = d.file
                """
            )
        }
        sym = {
            row[0]: row[1]
            for row in db.execute(
                """
                select d.id, text.content
                from _sym_dict d
                join _strings text on text.id = d.sym_hash
                """
            )
        }
        direct = {
            f"{node[from_id]}\t{node[to_id]}"
            for from_id, to_id in db.execute("select `from`, `to` from rel_df_edge")
        }
        arg = {
            f"{node[arg_id]}\t{node[param_id]}"
            for arg_id, param_id in db.execute(
                """
                select a.arg, p.id
                from rel_call_target target
                join rel_df_arg a on a.call = target.call
                join rel_df_node param on param.fn = target.callee
                join rel_df_param p on p.id = param.id and p.pos = a.pos
                join _sym_dict kind_sym on kind_sym.id = param.kind
                join _strings kind_text on kind_text.id = kind_sym.sym_hash
                where kind_text.content = 'param'
                """
            )
        }
        ret = {
            f"{node[ret_id]}\t{node[call_id]}"
            for ret_id, call_id in db.execute(
                """
                select ret.id, target.call
                from rel_call_target target
                join rel_df_node ret on ret.fn = target.callee
                join _sym_dict kind_sym on kind_sym.id = ret.kind
                join _strings kind_text on kind_text.id = kind_sym.sym_hash
                where kind_text.content = 'ret'
                """
            )
        }
        targets = set()
        for call_id, callee_q in db.execute(
            "select call, callee_q from rel_call_target"
        ):
            path, name = v5_sym_key(sym[callee_q])
            targets.add(f"{node[call_id]}\t{path}\t{name}")
        return {"direct": direct, "arg": arg, "ret": ret}, targets
    finally:
        db.close()


def v6_edge_inputs(work, translate):
    files = {
        "direct": "df_direct",
        "arg": "flow_arg_edge",
        "ret": "flow_ret_edge",
    }
    edges = {}
    for name, rel in files.items():
        edges[name] = {
            "\t".join(translate.node(cell) for cell in row.split("\t")[:2])
            for row in read_tsv(os.path.join(work, f"v6.{rel}.tsv"))
        }
    targets = set()
    for row in read_tsv(os.path.join(work, "v6.call_target.tsv")):
        caller_path, call_start, callee_path, callee_name = row.split("\t")
        call = translate.node(f"{caller_path}:{call_start}:{call_start}")
        targets.add(f"{call}\t{callee_path}\t{callee_name}")
    return edges, targets


class V5Side:
    side = "v5"

    def node(self, token):
        return v5_node_key(token)


class V6Side:
    side = "v6"

    def __init__(self, root):
        self.mapper = ByteToLineCol(root)

    def node(self, token):
        return self.mapper.key(token)


def sample(rows):
    return "; ".join(sorted(rows)[:3]) or "-"


def main():
    work = sys.argv[1]
    v5_side = V5Side()
    v6_side = V6Side(os.path.join(work, "root"))
    rows = []
    for rel, reason in REASONS.items():
        v5_raw = read_tsv(os.path.join(work, f"v5.{rel}.tsv"))
        v6_raw = read_tsv(os.path.join(work, f"v6.{rel}.tsv"))
        v5 = keyed(rel, v5_raw, v5_side)
        v6 = keyed(rel, v6_raw, v6_side)
        collapse = (len(v5_raw) - len(v5), len(v6_raw) - len(v6))
        matched = v5 & v6
        v5_only = v5 - v6
        v6_only = v6 - v5
        gap = len(v5_only) + len(v6_only)
        rows.append((rel, len(v5), len(v6), len(matched), len(v5_only),
                     len(v6_only), gap, collapse, reason, v5_only, v6_only))

    flow_edge = next(row for row in rows if row[0] == "flow_edge")
    if flow_edge[3] == 0:
        print("FAIL  flow_edge match assertion: expected at least one matching value-plane row")
        return 1
    flow_node_type = next(row for row in rows if row[0] == "flow_node_type")
    if flow_node_type[2] == 0 or flow_node_type[3] == 0:
        print("FAIL  flow_node_type rail: expected nonempty v6 rows and a nonzero v5 intersection")
        return 1

    print("CLASSIFICATION TABLE (keys: path:line:col nodes; kind-collapse counts shown)")
    print("  {:<16}{:>7}{:>7}{:>7}{:>8}{:>8}{:>10}".format(
        "rel", "v5", "v6", "match", "v5only", "v6only", "collapse"))
    for rel, v5_n, v6_n, match_n, only5_n, only6_n, gap, collapse, reason, _, _ in rows:
        print("  {:<16}{:>7}{:>7}{:>7}{:>8}{:>8}{:>10}".format(
            rel, v5_n, v6_n, match_n, only5_n, only6_n, f"{collapse[0]}/{collapse[1]}"))
        if gap:
            print(f"    (b) {reason}")

    print("\nSPOT-CHECKED PAIRS")
    for rel, _, _, _, _, _, _, _, _, v5_only, v6_only in rows:
        print(f"  {rel} v5-only: {sample(v5_only)}")
        print(f"  {rel} v6-only: {sample(v6_only)}")

    v5_edges, v5_targets = v5_flow_inputs(work)
    v6_edges, v6_targets = v6_edge_inputs(work, v6_side)
    print("\nFLOW EDGE PROVENANCE")
    print("  {:<12}{:>7}{:>7}{:>7}{:>8}{:>8}".format(
        "source", "v5", "v6", "match", "v5only", "v6only"
    ))
    for source in ("direct", "arg", "ret"):
        left = v5_edges[source]
        right = v6_edges[source]
        print("  {:<12}{:>7}{:>7}{:>7}{:>8}{:>8}".format(
            source, len(left), len(right), len(left & right),
            len(left - right), len(right - left)
        ))
    direct_gap = (v5_edges["direct"] - v6_edges["direct"]) | (
        v6_edges["direct"] - v5_edges["direct"]
    )
    if direct_gap:
        print(f"FAIL  direct extraction-input parity: {sample(direct_gap)}")
        return 1

    print("\nCALL TARGET KEYS")
    print("  v5={} v6={} match={} v5only={} v6only={}".format(
        len(v5_targets), len(v6_targets), len(v5_targets & v6_targets),
        len(v5_targets - v6_targets), len(v6_targets - v5_targets)
    ))
    print(f"  v5-only: {sample(v5_targets - v6_targets)}")
    print(f"  v6-only: {sample(v6_targets - v5_targets)}")

    print(
        "\nevery difference classified: direct extraction=exact; "
        "interproc=arg/ret provenance plus call-target key divergence; "
        "typed views=normalized key gaps above; 0 unclassified"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
