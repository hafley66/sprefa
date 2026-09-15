"""Rewrite the frozen v7 root to `<root>`, canonicalize each dump, commit what
fits the byte budget and index every case in status.json."""

import json
import os
import sys

BUDGET = 6 * 1024 * 1024
ROOT_TOKEN = "<root>"

# Coverage beats size: every diagnostic case and every project case is
# committed, then the named spread, then the rest smallest first.
SPREAD = (
    "test-fixtures-2_partial",
    "test-fixtures-3_type_algebra",
    "applications-dl6-0_catalog",
    "test-fixtures-14_syntax_macros",
    "test-fixtures-8_hosted",
    "test-fixtures-binding_symmetry-10_infix_colon_spelling",
    "test-fixtures-16_interned_storage",
    "test-fixtures-sqlite_query-0_cst_edge_kinds",
)


def priority(row):
    if row["diagnostics"]:
        return (0, row["bytes"])
    if row["stem"].startswith("project-"):
        return (1, row["bytes"])
    if row["stem"] in SPREAD:
        return (2, SPREAD.index(row["stem"]))
    return (3, row["bytes"])


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def main():
    scratch, here, v7 = sys.argv[1:4]
    cases_dir = os.path.join(here, "cases")
    os.makedirs(cases_dir, exist_ok=True)
    for stale in os.listdir(cases_dir):
        os.remove(os.path.join(cases_dir, stale))

    rows = []
    for name in sorted(os.listdir(scratch)):
        if not name.endswith(".json"):
            continue
        stem = name[: -len(".json")]
        text = open(os.path.join(scratch, name), encoding="utf-8").read()
        expected = json.loads(text.replace(v7, ROOT_TOKEN))
        tokens = open(os.path.join(scratch, stem + ".args"), encoding="utf-8").read().splitlines()
        arguments = [t if t.startswith("--") else ROOT_TOKEN + "/" + t for t in tokens]
        ms = int(open(os.path.join(scratch, stem + ".ms"), encoding="utf-8").read())
        body = canonical({"input": {"arguments": arguments}, "expected": expected})
        rows.append({
            "stem": stem,
            "arguments": arguments,
            "bytes": len(canonical(expected)),
            "compiler_rows": len(expected["compiler_rows"]),
            "diagnostics": len(expected["diagnostics"]),
            "v7_ms": ms,
            "body": body,
        })

    rows.sort(key=priority)
    committed = 0
    for row in rows:
        row["committed"] = committed + len(row["body"]) <= BUDGET
        if row["committed"]:
            path = os.path.join(cases_dir, row["stem"] + ".json")
            open(path, "w", encoding="utf-8").write(row["body"])
            committed += len(row["body"])

    keys = ("stem", "arguments", "bytes", "compiler_rows", "diagnostics",
            "v7_ms", "committed")
    status = {
        "rev": "f5018ad23",
        "root_token": ROOT_TOKEN,
        "committed_bytes": committed,
        "budget_bytes": BUDGET,
        "cases": [{k: row[k] for k in keys}
                  for row in sorted(rows, key=lambda r: r["stem"])],
    }
    open(os.path.join(here, "status.json"), "w", encoding="utf-8").write(
        json.dumps(status, indent=1, sort_keys=True) + "\n")
    print(f"{len(rows)} cases, {sum(r['committed'] for r in rows)} committed, "
          f"{committed} bytes")


if __name__ == "__main__":
    main()
