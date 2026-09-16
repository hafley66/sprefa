#!/usr/bin/env python3
"""ctf_tasks.py <MANIFEST.json> <out tasks.jsonl>

Task generator for the v2 CTF (capture-the-flag) class. Unlike the C2 path
(gen-tasks.dl -> select_tasks.py -> mutate.sh), CTF tasks are NOT dl-generated
and NOT mutation-seeded: they are a fixed set of questions over one committed
fixture, each with an expected answer set sliced out of the hand-built
MANIFEST.json (bench/agent-eval/build_manifest.py's output). dl never touches
the key.

Each task's `expected_json` is a LIST of {"file","line"} sites (not the single
{"file","line"} of C2), scored downstream as set-F1 with the manifest's
window_lines tolerance.

The prompt is IDENTICAL in shape across cells (protocol #2 symmetry): it tells
the agent to read SKILL.md first (the per-cell tool inventory) and then answer,
emitting ONLY a JSON object {"sites":[...]}. The per-task question text differs
by scope (full inventory vs one abstraction vs the import negative control), the
SAME question for every cell.

Emission order puts `full-inventory` first so `run.sh --tasks-limit 1` yields
the single hardest task (for an optional sonnet-only add-on run).
"""
import json
import sys

PROMPT_TEMPLATE = (
    "Read the file SKILL.md at the root of this repository FIRST — it documents "
    "the tools available to you for this task. Then answer this question.\n\n"
    "{question}\n\n"
    "Respond with ONLY a JSON object of the form "
    '{{"sites": [{{"file": "<repo-relative path>", "line": <line number>}}, ...]}} '
    "listing every enforcement site you found, and nothing else — no prose, no "
    "markdown fence. If there are none, return {{\"sites\": []}}."
)

EXPORT_INTRO = (
    "This service enforces an EXPORT permission — a user's right to export data, "
    "canonically named `can_export` — and it does so in several different ways "
    "across the codebase, added by different people over different releases "
    "(a raw boolean flag, a permission service, a guard/middleware wrapper, and "
    "a runtime config-file rule). Beware decoys: names containing \"export\" that "
    "are UI toggles or unrelated flags are NOT permission enforcement, and the "
    "IMPORT permission is a different concept."
)

# task_id -> (question, filter over manifest sites)
# filter is (concept, abstraction_or_None)
TASK_SPECS = [
    ("full-inventory", EXPORT_INTRO + " Find EVERY place the export permission is "
     "actually enforced (the check that denies the request). List every "
     "enforcement site.", ("export", None)),
    ("abstraction-flag", EXPORT_INTRO + " Find ONLY the export-permission gates "
     "implemented as a direct boolean flag check on the user model (the oldest "
     "style). List those sites.", ("export", "flag")),
    ("abstraction-service", EXPORT_INTRO + " Find ONLY the export-permission gates "
     "implemented through the permission service (a require/allows call). List "
     "those sites.", ("export", "service")),
    ("abstraction-guard", EXPORT_INTRO + " Find ONLY the export-permission gates "
     "implemented through a guard or middleware wrapper. List those sites.",
     ("export", "guard")),
    ("abstraction-config", EXPORT_INTRO + " Find ONLY the export-permission gates "
     "implemented through the runtime config-file rule engine. List those sites.",
     ("export", "config")),
    ("neg-control-import", "This service also enforces an IMPORT permission "
     "(`can_import`), which is a DIFFERENT concept from export. Find EVERY place "
     "the IMPORT permission is enforced — NOT export. List those sites.",
     ("import", None)),
]


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    manifest_path, out_path = sys.argv[1:3]
    manifest = json.load(open(manifest_path))
    all_sites = manifest["sites"]

    tasks = []
    for task_stub, question, (concept, abstraction) in TASK_SPECS:
        expected = [
            {"file": s["file"], "line": s["line"]}
            for s in all_sites
            if s["concept"] == concept
            and (abstraction is None or s["abstraction"] == abstraction)
        ]
        tasks.append({
            "id": f"ctf-{task_stub}",
            "class": "CTF",
            "concept": concept,
            "abstraction": abstraction or "all",
            "prompt": PROMPT_TEMPLATE.format(question=question),
            "expected_json": expected,
        })

    with open(out_path, "w") as f:
        for task in tasks:
            f.write(json.dumps(task, sort_keys=True))
            f.write("\n")
    print(f"ctf_tasks: wrote {len(tasks)} tasks -> {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
