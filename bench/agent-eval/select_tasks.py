#!/usr/bin/env python3
"""select_tasks.py <candidates.json> <experiment.toml> <out tasks.jsonl>

The deterministic sampler between gen-tasks.dl and mutate.sh
(plans/2026-07-10-agent-eval-harness.md anti-grift protocol #4: "corpus
sampling is seeded (order by sym-hash, not random)").

dl has no hash/random-seeded-sort primitive in its DSL (a genuine, minor
surface gap — see the S1 report), so this step lives outside the .dl program:
it reads gen-tasks.dl's raw candidate_site rows (every structurally eligible
site, unfiltered), sorts each mutation kind's pool by
sha256(f"{site_path}:{site_line}:{mutation_kind}") — a fixed, non-lexicographic,
reproducible order that does not privilege alphabetically-early files or
line-number order — and takes the first `tasks_per_kind` (an
experiment.toml-committed numeric filter) from each kind's pool. No
randomness (no `random`/`os.urandom` import), no post-hoc removal: the same
candidates.json + experiment.toml always produces byte-identical tasks.jsonl.

`candidates.json` is `dl gen-tasks.dl --query-json`'s single JSON line,
`{"query":"candidate_site","columns":[...],"rows":[[...],...]}`.

The generic C2 prompt is written here (identical across every cell — the
prompt text carries no dl-specific hint, satisfying protocol #2). Each task's
`expected_json` is left null: mutate.sh is the only writer of that field,
computed from the mutation it actually applied, never from gen-tasks.dl's
candidate pick.
"""
import hashlib
import json
import sys

C2_PROMPT = (
    "A recent change broke something in this repository. Find the exact "
    "file and line number of the defect. Respond with ONLY a JSON object of "
    'the form {"file": "<repo-relative path>", "line": <line number>} and '
    "nothing else — no prose, no markdown code fence."
)


def sym_hash_key(site_path, site_line, mutation_kind):
    digest = hashlib.sha256(f"{site_path}:{site_line}:{mutation_kind}".encode("utf-8")).hexdigest()
    return digest


def load_toml(path):
    try:
        import tomllib
        with open(path, "rb") as f:
            return tomllib.load(f)
    except ModuleNotFoundError:  # pragma: no cover - py<3.11 fallback
        import tomli
        with open(path, "rb") as f:
            return tomli.load(f)


def main():
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    candidates_path, experiment_toml_path, out_path = sys.argv[1:4]

    with open(candidates_path) as f:
        payload = json.load(f)
    columns = payload["columns"]
    col_idx = {name: i for i, name in enumerate(columns)}
    rows = payload["rows"]

    experiment = load_toml(experiment_toml_path)
    filters = experiment.get("filters", {})
    tasks_per_kind = int(filters.get("tasks_per_kind", 10))
    min_bound = int(filters.get("min_bound", 0))
    max_bound = int(filters.get("max_bound", 10_000_000))

    by_kind = {}
    for row in rows:
        mutation_kind = row[col_idx["mutation_kind"]]
        site_path = row[col_idx["site_path"]]
        site_line = int(row[col_idx["site_line"]])
        detail = row[col_idx["detail"]]
        if mutation_kind == "off-by-one":
            try:
                bound = int(detail)
            except ValueError:
                continue
            if not (min_bound <= bound <= max_bound):
                continue
        by_kind.setdefault(mutation_kind, []).append((site_path, site_line, detail))

    # Build each kind's capped, sym-hash-ordered pool first, then emit the
    # tasks ROUND-ROBIN across kinds (kind order = sorted, rank order = the
    # sha256 sort). The full set is identical to a grouped emission; only the
    # LINE ORDER differs, so `run.sh --tasks-limit N` yields a balanced
    # cross-kind subset (10 -> ~2-3 per kind) instead of the first kind's 10.
    # Determinism is unchanged: same candidates.json + experiment.toml -> the
    # same bytes. Nothing downstream depends on line order except the cap.
    per_kind = []
    for mutation_kind in sorted(by_kind):
        pool = by_kind[mutation_kind]
        pool.sort(key=lambda t: sym_hash_key(t[0], t[1], mutation_kind))
        kind_tasks = []
        for rank, (site_path, site_line, detail) in enumerate(pool[:tasks_per_kind]):
            kind_tasks.append({
                "id": f"c2-{mutation_kind}-{rank:02d}",
                "class": "C2",
                "mutation_kind": mutation_kind,
                "site_path": site_path,
                "site_line": site_line,
                "site_detail": detail,
                "prompt": C2_PROMPT,
                "expected_json": None,
            })
        per_kind.append(kind_tasks)

    tasks = []
    for rank in range(max((len(k) for k in per_kind), default=0)):
        for kind_tasks in per_kind:
            if rank < len(kind_tasks):
                tasks.append(kind_tasks[rank])

    with open(out_path, "w") as f:
        for task in tasks:
            f.write(json.dumps(task, sort_keys=True))
            f.write("\n")

    counts = {k: min(len(v), tasks_per_kind) for k, v in by_kind.items()}
    print(f"select_tasks: wrote {len(tasks)} tasks -> {out_path} ({counts})", file=sys.stderr)


if __name__ == "__main__":
    main()
