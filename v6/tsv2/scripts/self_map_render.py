#!/usr/bin/env python3
"""self_map_render.py: derived rows in, v6/ARCH-MAP.md out.

Reads one JSON document on stdin, `{"<rel>": {"rows": [[...], ...]}, ...}`,
exactly as v6/tsv2/scripts/self-map.sh assembles it from the served engine's
`GET /idb/:rel`, and writes the markdown to stdout.

WHAT THIS FILE IS ALLOWED TO DO, and nothing else: escape values for mermaid,
join rows into lines, and lay out sections. Every classification the document
asserts -- ready vs blocked, wired vs unwired, all-live axis, sink rel, every
count -- arrives as its own rel from v6/dl/fixtures/self-map.dl6. If a
judgement is being made here, it is in the wrong file.

The one thing that IS a judgement here and cannot move: the mermaid text
itself. dl6 has no string aggregate (`group_concat` is a named refusal,
`concat/1` folds a fixed expression list rather than N rows), so folding rows
into a document has no spelling in the language today. That is the rail's
headline language finding and it is stated in three places on purpose.

`--complete-check` exits 0 when every rel a diagram needs is non-empty and all
four sources have reported; the rail's settle loop uses it so a half-filled
mid-tick read is never rendered.
"""

import json
import re
import sys

# ─────────────────────────────────────────────────────────────────────────────
# The schema, mirrored from v6/dl/fixtures/self-map.dl6's decls.
#
# `GET /idb/:rel` answers positional rows in DECLARED column order
# (runtime/rows.ts), so the names live here and the arity is asserted on every
# read. A column added to a decl without updating this table fails loudly on
# the next run instead of silently shifting every value one place left, which
# is the same cross-contamination rule failure class 36 named.
# ─────────────────────────────────────────────────────────────────────────────

SCHEMA = {
    "source": ["path", "digest"],
    "phase": ["phase_order", "phase_name", "expander"],
    "phase_wired": ["phase_order", "phase_name", "expander"],
    "phase_unwired": ["phase_order", "phase_name"],
    "phase_step": ["from_name", "to_name"],
    "phase_first": ["phase_order", "phase_name"],
    "phase_last": ["phase_order", "phase_name"],
    "construct": ["functor", "arity", "axis", "status"],
    "axis": ["axis"],
    "axis_total": ["axis", "constructs"],
    "axis_status_total": ["axis", "status", "constructs"],
    "axis_all_live": ["axis"],
    "task": ["task_name", "state"],
    "task_dep": ["task_name", "dep_name"],
    "task_ready": ["task_name", "state"],
    "task_blocked": ["task_name", "state"],
    "frontier_edge": ["dep_name", "task_name"],
    "frontier_node": ["task_name", "state"],
    "state_total": ["state", "tasks"],
    "task_state_conflict": ["task_name", "state"],
    "program_rel": ["program", "rel_name", "rel_kind", "origin"],
    "program_edge": ["program", "from_rel", "to_rel", "sign", "arrow"],
    "program_sink": ["program", "rel_name"],
    "program_fan_in": ["program", "rel_name", "writers"],
    "program_fan_out": ["program", "rel_name", "readers"],
    "program_negated_edge": ["program", "from_rel", "to_rel"],
}

# Every diagram's own source rel. Empty means the engine has not answered yet,
# which is a not-ready read rather than an empty architecture.
REQUIRED_NONEMPTY = ["phase", "construct", "task", "task_dep", "program_rel", "program_edge"]
EXPECTED_SOURCES = 4

WATCHED = [
    ("v6/prolog/compile/registry.pl", "`surface/5`", "the construct inventory"),
    ("v6/prolog/1_expansion.pl", "`expansion_phase/3`", "the declared sugar order"),
    ("v6/prolog/ARCH.pl", "`task/3`", "the build DAG"),
    ("v6/dl/fixtures/self-map.dl6", "`analyze.pl`", "this program's own rel graph"),
]


def read_rel(document, name):
    """Rows of one rel as dicts, arity-checked, sorted."""
    columns = SCHEMA[name]
    payload = document.get(name) or {}
    rows = payload.get("rows") or []
    out = []
    for row in rows:
        if len(row) != len(columns):
            raise SystemExit(
                f"rel '{name}' answered {len(row)} columns, schema says {len(columns)}: {row}"
            )
        out.append(dict(zip(columns, row)))
    return sorted(out, key=lambda item: [str(item[column]) for column in columns])


def node_id(prefix, value):
    """A mermaid-safe identifier. Distinct inputs must stay distinct, so the
    sanitized text carries the original's own characters where it can and a
    hex escape where it cannot."""
    safe = re.sub(r"[^A-Za-z0-9_]", lambda m: f"x{ord(m.group(0)):02x}", str(value))
    return f"{prefix}_{safe}"


def label(text):
    """Mermaid label text. `#NNN;` entities are mermaid's own escape and are
    what keep `<-`, `"`, and `\\==` from closing the node or the diagram."""
    escaped = str(text)
    for character, entity in (
        ("#", "#35;"),
        ('"', "#quot;"),
        ("<", "#lt;"),
        (">", "#gt;"),
        ("[", "#91;"),
        ("]", "#93;"),
        ("{", "#123;"),
        ("}", "#125;"),
        ("(", "#40;"),
        (")", "#41;"),
        ("|", "#124;"),
    ):
        escaped = escaped.replace(character, entity)
    return escaped


def table(headers, rows):
    lines = ["| " + " | ".join(headers) + " |", "|" + "|".join(["---"] * len(headers)) + "|"]
    for row in rows:
        lines.append("| " + " | ".join(str(cell) for cell in row) + " |")
    return lines


# ─────────────────────────────────────────────────────────────────────────────
# Diagram 1: the expansion phase order
# ─────────────────────────────────────────────────────────────────────────────


def diagram_phases(document, out):
    phases = read_rel(document, "phase")
    steps = read_rel(document, "phase_step")
    unwired = {row["phase_name"] for row in read_rel(document, "phase_unwired")}
    # The expander text comes from `phase_wired`, NOT from this file testing
    # `expander == "unwired"`. That test is the dl6 rule's job and it already
    # ran; repeating it here would be the second implementation of a
    # classification, which is exactly what the header forbids.
    expanders = {row["phase_name"]: row["expander"] for row in read_rel(document, "phase_wired")}
    first = {row["phase_name"] for row in read_rel(document, "phase_first")}
    last = {row["phase_name"] for row in read_rel(document, "phase_last")}

    out.append("## 1. Surface sugar expands in a declared order")
    out.append("")
    out.append(
        "`expansion_phase/3` in `v6/prolog/1_expansion.pl`, read by the compiler and "
        "the oracle both. Arrows are the computed successor (the smallest order "
        "strictly greater), not the literal numbers, so a phase landing between two "
        "existing ones redraws correctly. Dashed = declared slot with no expander."
    )
    out.append("")
    out.append("```mermaid")
    out.append("flowchart LR")
    for row in sorted(phases, key=lambda item: item["phase_order"]):
        name = row["phase_name"]
        identifier = node_id("ph", name)
        expander = expanders.get(name)
        suffix = f"<br/>{label(expander.split(':')[-1])}" if expander else ""
        out.append(f'  {identifier}["{label(row["phase_order"])} {label(name)}{suffix}"]')
    for row in steps:
        out.append(f'  {node_id("ph", row["from_name"])} --> {node_id("ph", row["to_name"])}')
    out.append("  classDef slot stroke-dasharray: 4 3")
    for name in sorted(unwired):
        out.append(f'  class {node_id("ph", name)} slot')
    out.append("```")
    out.append("")
    out.append(
        f"First phase: {', '.join(f'`{name}`' for name in sorted(first)) or 'none'}. "
        f"Last phase: {', '.join(f'`{name}`' for name in sorted(last)) or 'none'}. "
        f"Unwired slots: {', '.join(f'`{name}`' for name in sorted(unwired)) or 'none'}."
    )
    out.append("")


# ─────────────────────────────────────────────────────────────────────────────
# Diagram 2: the construct registry, by axis
# ─────────────────────────────────────────────────────────────────────────────


def diagram_registry(document, out):
    constructs = read_rel(document, "construct")
    totals = {row["axis"]: row["constructs"] for row in read_rel(document, "axis_total")}
    status_totals = read_rel(document, "axis_status_total")
    all_live = {row["axis"] for row in read_rel(document, "axis_all_live")}

    by_axis = {}
    for row in constructs:
        by_axis.setdefault(row["axis"], []).append(row)

    out.append("## 2. The writable surface, by axis")
    out.append("")
    out.append(
        "`surface/5` in `v6/prolog/compile/registry.pl` is the single inventory the "
        "parser, printer, analyzer and supported-subset gate all project from, so this "
        "is the whole writable language and not a selection from it. An axis whose "
        "every row is `live` is drawn solid; an axis still carrying a reserved or "
        "refused row is dashed."
    )
    out.append("")
    out.append("```mermaid")
    out.append("flowchart TD")
    for axis in sorted(by_axis):
        out.append(f'  subgraph {node_id("ax", axis)}["{label(axis)}"]')
        out.append("    direction TB")
        for row in by_axis[axis]:
            signature = f'{row["functor"]}/{row["arity"]}'
            identifier = node_id("c", signature)
            suffix = "" if row["status"] == "live" else f'<br/>{label(row["status"])}'
            out.append(f'    {identifier}["{label(signature)}{suffix}"]')
        out.append("  end")
    out.append("  classDef moving stroke-dasharray: 4 3")
    for axis in sorted(set(by_axis) - all_live):
        out.append(f'  class {node_id("ax", axis)} moving')
    out.append("```")
    out.append("")

    statuses = sorted({row["status"] for row in status_totals})
    counts = {(row["axis"], row["status"]): row["constructs"] for row in status_totals}
    rows = []
    for axis in sorted(totals):
        cells = [f"`{axis}`", totals[axis]]
        cells.extend(counts.get((axis, status), 0) for status in statuses)
        cells.append("yes" if axis in all_live else "")
        rows.append(cells)
    out.extend(table(["axis", "constructs"] + statuses + ["all live"], rows))
    out.append("")


# ─────────────────────────────────────────────────────────────────────────────
# Diagram 3: the ARCH build DAG's open frontier
# ─────────────────────────────────────────────────────────────────────────────


def diagram_frontier(document, out):
    edges = read_rel(document, "frontier_edge")
    nodes = read_rel(document, "frontier_node")
    ready = read_rel(document, "task_ready")
    blocked = {row["task_name"] for row in read_rel(document, "task_blocked")}
    state_totals = read_rel(document, "state_total")
    conflicts = read_rel(document, "task_state_conflict")
    task_count = len(read_rel(document, "task"))
    dep_count = len(read_rel(document, "task_dep"))

    out.append("## 3. The build DAG's open frontier")
    out.append("")
    out.append(
        f"`task/3` in `v6/prolog/ARCH.pl`: {task_count} tasks, {dep_count} dependency "
        "edges. Drawing all of them is noise, so the rel `frontier_edge` keeps only "
        "the edges touching a task that is not `done` -- a done task appears exactly "
        "when something open still waits behind it. `task_blocked` and `task_ready` "
        "are the two antijoins that split the open set."
    )
    out.append("")
    # A task carrying two state rows (the ARCH.pl contradiction below) is ONE
    # node whose label shows both, joined here rather than picked between --
    # picking would be a classification, and this file does not classify.
    states_by_task = {}
    for row in nodes:
        states_by_task.setdefault(row["task_name"], set()).add(row["state"])
    seen = sorted(states_by_task)

    out.append("```mermaid")
    out.append("flowchart LR")
    for name in seen:
        states = " / ".join(sorted(states_by_task[name]))
        out.append(f'  {node_id("t", name)}["{label(name)}<br/>{label(states)}"]')
    for row in edges:
        out.append(f'  {node_id("t", row["dep_name"])} --> {node_id("t", row["task_name"])}')
    out.append("  classDef blocked stroke-width:3px")
    for name in sorted(blocked & set(seen)):
        out.append(f'  class {node_id("t", name)} blocked')
    out.append("```")
    out.append("")
    out.extend(
        table(
            ["state", "tasks"],
            [[f"`{row['state']}`", row["tasks"]] for row in sorted(state_totals, key=lambda i: i["state"])],
        )
    )
    out.append("")
    # NAMES, not rows: a task carrying two state rows contributes two
    # `task_ready` rows and is still one task. The contradiction is reported by
    # its own table below rather than by printing the name twice here.
    ready_names = sorted({f"`{row['task_name']}`" for row in ready})
    out.append(
        f"**Ready** (open, every dependency done): {len(ready_names)} tasks. "
        f"**Blocked**: {len(blocked)} tasks."
    )
    out.append("")
    out.append("<details><summary>the ready set</summary>")
    out.append("")
    out.append(", ".join(ready_names))
    out.append("")
    out.append("</details>")
    out.append("")
    if conflicts:
        conflicting = {}
        for row in conflicts:
            conflicting.setdefault(row["task_name"], set()).add(row["state"])
        out.append(
            "**`task/3` state contradictions.** `task_state_conflict` counts states per "
            "task name; a name with more than one is a row that was appended when its "
            "state changed instead of rewritten. `just arch`'s `go` does not catch this "
            "-- it checks the dependency graph is acyclic and total, which a duplicated "
            "row does not break."
        )
        out.append("")
        out.extend(
            table(
                ["task", "states"],
                [
                    [f"`{name}`", ", ".join(f"`{state}`" for state in sorted(states))]
                    for name, states in sorted(conflicting.items())
                ],
            )
        )
        out.append("")


# ─────────────────────────────────────────────────────────────────────────────
# Diagram 4: a compiled program's rel dataflow, over this program
# ─────────────────────────────────────────────────────────────────────────────


def diagram_dataflow(document, out):
    rels = read_rel(document, "program_rel")
    edges = read_rel(document, "program_edge")
    sinks = {(row["program"], row["rel_name"]) for row in read_rel(document, "program_sink")}
    negated = {
        (row["program"], row["from_rel"], row["to_rel"])
        for row in read_rel(document, "program_negated_edge")
    }
    fan_in = read_rel(document, "program_fan_in")
    fan_out = read_rel(document, "program_fan_out")

    programs = sorted({row["program"] for row in rels})
    out.append("## 4. A compiled program's rel dataflow: this program's own")
    out.append("")
    out.append(
        "Read out of `analyze.pl` by `v6/prolog/tools/self_map_facts.pl`, through the "
        "same two-step `program_plan/2` runs: the host pre-pass first (which is why "
        "`__host_demand_*` and `__host_response_*` appear -- the demand/answer round "
        "trip the `sh` surface hides), then the declared sugar phases. `origin` is the "
        "analyzer's own partition: `world` = headed by no rule, `level` = headed by a "
        "`<-` rule, `edge` = headed by a `<+` rule. Dashed edges are negated bodies, "
        "which are the reason this program has strata."
    )
    out.append("")
    out.append(
        "A `*` marks a rel no RULE reads. Two different things wear that mark: a real "
        "answer rel meant to be read from outside, and a `__host_demand_*` rel, which "
        "the host RUNTIME consumes rather than any rule. The map draws the set and "
        "does not guess which is which."
    )
    out.append("")

    for program in programs:
        program_rels = [row for row in rels if row["program"] == program]
        program_edges = [row for row in edges if row["program"] == program]
        by_origin = {}
        for row in program_rels:
            by_origin.setdefault(row["origin"], []).append(row)

        out.append("```mermaid")
        out.append("flowchart LR")
        for origin in sorted(by_origin):
            out.append(f'  subgraph {node_id("og", origin)}["{label(origin)}"]')
            out.append("    direction TB")
            for row in by_origin[origin]:
                identifier = node_id("r", row["rel_name"])
                mark = " *" if (program, row["rel_name"]) in sinks else ""
                out.append(f'    {identifier}["{label(row["rel_name"])}{label(mark)}"]')
            out.append("  end")
        for row in program_edges:
            arrow = (
                "-.->"
                if (program, row["from_rel"], row["to_rel"]) in negated
                else "-->"
            )
            out.append(
                f'  {node_id("r", row["from_rel"])} {arrow} {node_id("r", row["to_rel"])}'
            )
        out.append("```")
        out.append("")
        sink_names = sorted(name for (owner, name) in sinks if owner == program)
        out.append(
            f"`{program}`: {len(program_rels)} rels, {len(program_edges)} edges. "
            f"Sinks (`*`, nothing reads them): {', '.join(f'`{name}`' for name in sink_names) or 'none'}."
        )
        out.append("")
        widest_in = sorted(
            (row for row in fan_in if row["program"] == program),
            key=lambda row: (-row["writers"], row["rel_name"]),
        )[:5]
        widest_out = sorted(
            (row for row in fan_out if row["program"] == program),
            key=lambda row: (-row["readers"], row["rel_name"]),
        )[:5]
        out.extend(
            table(
                ["widest fan-in", "writers", "widest fan-out", "readers"],
                [
                    [
                        f"`{a['rel_name']}`" if a else "",
                        a["writers"] if a else "",
                        f"`{b['rel_name']}`" if b else "",
                        b["readers"] if b else "",
                    ]
                    for a, b in zip(widest_in, widest_out)
                ],
            )
        )
        out.append("")


# ─────────────────────────────────────────────────────────────────────────────


def render(document):
    out = []
    out.append("# v6 architecture map")
    out.append("")
    out.append(
        "GENERATED by `just self-map` (`v6/tsv2/scripts/self-map.sh`). Do not edit: "
        "the next run overwrites it, byte for byte."
    )
    out.append("")
    out.append(
        "Every fact below travelled through the served tsv2 engine. "
        "`v6/dl/fixtures/self-map.dl6` declares the sources as `bind watch` globs and "
        "one `sh` host with six projections; the derivations -- ready vs blocked, "
        "wired vs unwired, all-live axis, sink rel, every count -- are ordinary dl6 "
        "rules in that file. This renderer only escapes, joins and lays out, because "
        "dl6 has no string aggregate and the mermaid document is a fold over rows."
    )
    out.append("")
    out.extend(
        table(
            ["watched source", "authority", "what it says"],
            [[f"`{path}`", authority, what] for (path, authority, what) in WATCHED],
        )
    )
    out.append("")
    diagram_phases(document, out)
    diagram_registry(document, out)
    diagram_frontier(document, out)
    diagram_dataflow(document, out)
    return "\n".join(out).rstrip() + "\n"


def complete(document):
    if len(read_rel(document, "source")) != EXPECTED_SOURCES:
        return False
    return all(read_rel(document, name) for name in REQUIRED_NONEMPTY)


def main():
    document = json.load(sys.stdin)
    if "--complete-check" in sys.argv[1:]:
        sys.exit(0 if complete(document) else 1)
    sys.stdout.write(render(document))


if __name__ == "__main__":
    main()
