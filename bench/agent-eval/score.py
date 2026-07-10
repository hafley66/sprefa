#!/usr/bin/env python3
"""score.py <raw_dir> <experiment.toml> <out_prefix>

Scores one experiment run's raw cell outputs and writes the committed,
never-overwritten results pair (plans/2026-07-10-agent-eval-harness.md
protocol #6): `<out_prefix>.jsonl` (one row per task x cell x rep) and
`<out_prefix>.md` (the aggregated report, including a mandatory "where dl
lost" section — if the harness only ever reports wins, it isn't measuring
anything).

INPUT LAYOUT (written by run.sh)
  <raw_dir>/<task_id>.expected.json
      The task's expected_json + mutation_kind + class (copied out of the
      mutated worktree's task.json before that worktree is torn down).
  <raw_dir>/<task_id>__<cell>__rep<N>.json
      The RAW `claude -p --output-format json` blob for that (task, cell,
      rep) — untouched, so a parse failure here is scoreable evidence, not a
      script bug hiding it.
  <raw_dir>/<task_id>__<cell>__rep<N>.error
      Present instead of the .json file when the `claude` invocation itself
      failed (nonzero exit / timeout) — scored as a parse failure, distinct
      from a well-formed-JSON-but-wrong answer.

SCORING (C2 only; other classes fall through unscored — S2 wires C1)
  Lenient JSON extraction from the model's `result` text: direct
  `json.loads`, then a fenced ```...``` block, then the first balanced
  `{...}` substring, in that order — protocol #6's parse-failure rate is
  exactly "none of these three worked". A successful extraction is scored
  "solved" iff expected_json["file"] == answer["file"] (exact) AND
  abs(answer["line"] - expected_json["line"]) <= filters.score_window_lines
  (the "line-window" in "exact file+line-window match").

  Per (task, cell): the reported outcome is the MEDIAN of its `reps` binary
  outcomes (protocol #1: "3 reps... median scored"; for 3 binary reps this is
  majority vote). The per-cell headline rate is the mean of that per-task
  median over every task. The raw per-rep rate and the parse-failure rate are
  reported alongside it, never folded in silently.
"""
import glob
import json
import os
import re
import statistics
import sys


def load_toml(path):
    try:
        import tomllib
        with open(path, "rb") as f:
            return tomllib.load(f)
    except ModuleNotFoundError:  # pragma: no cover - py<3.11 fallback
        import tomli
        with open(path, "rb") as f:
            return tomli.load(f)


FENCE_RE = re.compile(r"```(?:json)?\s*(.*?)```", re.DOTALL)


def extract_json_lenient(text):
    """Try, in order: the whole text; the first fenced block; the first
    balanced {...} substring. Returns (parsed_dict_or_None, method_str)."""
    text = text.strip()
    try:
        return json.loads(text), "direct"
    except (json.JSONDecodeError, TypeError):
        pass
    m = FENCE_RE.search(text)
    if m:
        try:
            return json.loads(m.group(1).strip()), "fenced"
        except json.JSONDecodeError:
            pass
    # First balanced {...}: scan for a '{' then bracket-count to its match.
    start = text.find("{")
    while start != -1:
        depth = 0
        for i in range(start, len(text)):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    candidate = text[start:i + 1]
                    try:
                        return json.loads(candidate), "balanced-brace"
                    except json.JSONDecodeError:
                        break
        start = text.find("{", start + 1)
    return None, "parse-failure"


def score_c2(answer, expected, window):
    if not isinstance(answer, dict):
        return False
    answer_file = answer.get("file")
    answer_line = answer.get("line")
    try:
        answer_line = int(answer_line)
    except (TypeError, ValueError):
        return False
    if answer_file != expected["file"]:
        return False
    return abs(answer_line - expected["line"]) <= window


def load_expected(raw_dir, task_id):
    path = os.path.join(raw_dir, f"{task_id}.expected.json")
    with open(path) as f:
        return json.load(f)


def parse_cell_rep_name(path):
    base = os.path.basename(path)
    base = re.sub(r"\.(json|error)$", "", base)
    task_id, cell, rep_tag = base.rsplit("__", 2)
    rep = int(rep_tag.replace("rep", ""))
    return task_id, cell, rep


def score_one(raw_path, expected, window):
    """Returns a result row dict for one (task, cell, rep)."""
    is_error_file = raw_path.endswith(".error")
    row = {
        "solved": False, "parse_ok": False, "method": "invocation-error",
        "answer_file": None, "answer_line": None,
        "tokens_in": None, "tokens_out": None, "cost_usd": None,
        "duration_ms": None, "is_error": True,
    }
    if is_error_file:
        with open(raw_path) as f:
            row["raw_note"] = f.read().strip()[:500]
        return row
    with open(raw_path) as f:
        blob = json.load(f)
    row["is_error"] = bool(blob.get("is_error", False))
    usage = blob.get("usage", {}) or {}
    row["tokens_in"] = usage.get("input_tokens")
    row["tokens_out"] = usage.get("output_tokens")
    row["cost_usd"] = blob.get("total_cost_usd")
    row["duration_ms"] = blob.get("duration_ms")
    result_text = blob.get("result") or ""
    answer, method = extract_json_lenient(result_text)
    row["method"] = method
    if answer is None:
        return row
    row["parse_ok"] = True
    row["answer_file"] = answer.get("file") if isinstance(answer, dict) else None
    row["answer_line"] = answer.get("line") if isinstance(answer, dict) else None
    if expected.get("class") == "C2":
        row["solved"] = score_c2(answer, expected["expected_json"], window)
    return row


def median_binary(outcomes):
    return statistics.median(outcomes) >= 0.5


def render_report(rows, experiment, out_prefix):
    version = experiment.get("version", "unversioned")
    filters = experiment.get("filters", {})
    window = filters.get("score_window_lines")

    by_cell_task = {}
    for r in rows:
        by_cell_task.setdefault((r["cell"], r["task_id"]), []).append(r)

    cells = sorted({r["cell"] for r in rows})
    lines = [
        f"# Agent eval harness — {version}",
        "",
        f"Score window: +/- {window} lines. Rows: {len(rows)}. "
        f"Tasks: {len(sorted({r['task_id'] for r in rows}))}. "
        f"Cells: {', '.join(cells)}.",
        "",
        "## Headline (median-of-reps C2 locate rate)",
        "",
        "| cell | median locate rate | raw per-rep rate | parse-failure rate | tasks | reps |",
        "|---|---|---|---|---|---|",
    ]
    cell_medians = {}
    for cell in cells:
        task_ids = sorted({t for (c, t) in by_cell_task if c == cell})
        medians = []
        raw_outcomes = []
        parse_fail = []
        for task_id in task_ids:
            reps = by_cell_task[(cell, task_id)]
            outcomes = [1 if r["solved"] else 0 for r in reps]
            medians.append(1 if median_binary(outcomes) else 0)
            raw_outcomes.extend(outcomes)
            parse_fail.extend([0 if r["parse_ok"] else 1 for r in reps])
        median_rate = sum(medians) / len(medians) if medians else 0.0
        raw_rate = sum(raw_outcomes) / len(raw_outcomes) if raw_outcomes else 0.0
        pf_rate = sum(parse_fail) / len(parse_fail) if parse_fail else 0.0
        cell_medians[cell] = {task_id: m for task_id, m in zip(task_ids, medians)}
        n_reps = len(by_cell_task[(cell, task_ids[0])]) if task_ids else 0
        lines.append(
            f"| {cell} | {median_rate:.2%} | {raw_rate:.2%} | {pf_rate:.2%} "
            f"| {len(task_ids)} | {n_reps} |"
        )

    lines += ["", "## Cost per solved task", "", "| cell | total cost (usd) | solved (median) | cost / solved |", "|---|---|---|---|"]
    for cell in cells:
        cell_rows = [r for r in rows if r["cell"] == cell]
        total_cost = sum(r["cost_usd"] or 0.0 for r in cell_rows)
        solved = sum(cell_medians[cell].values())
        per_solved = (total_cost / solved) if solved else float("nan")
        lines.append(f"| {cell} | ${total_cost:.4f} | {solved} | "
                      f"{'$' + format(per_solved, '.4f') if solved else 'n/a (0 solved)'} |")

    # "Where dl lost": for each model, tasks the bash-only cell solved (median)
    # but the +dl cell (same model) did not. Mandatory per protocol #0 — an
    # empty section here is a REAL empty section, not an omission.
    lines += ["", "## Where dl lost", "",
              "Tasks the bash-only cell solved (median) but the matching +dl cell "
              "did not, same model. An empty table under a model heading is a real "
              "finding (dl never lost for that model on this run), not an omission."]
    pairs = []
    for cell in cells:
        if cell.endswith("-dl"):
            base = cell[: -len("-dl")]
            bash_cell = f"{base}-bash"
            if bash_cell in cells:
                pairs.append((bash_cell, cell))
    for bash_cell, dl_cell in pairs:
        lines.append(f"\n### {bash_cell} vs {dl_cell}\n")
        lost = [t for t, m in cell_medians.get(bash_cell, {}).items()
                if m == 1 and cell_medians.get(dl_cell, {}).get(t) == 0]
        if not lost:
            lines.append("(none — dl did not lose a single task to bash/grep here)")
        else:
            lines.append("| task_id |")
            lines.append("|---|")
            for t in lost:
                lines.append(f"| {t} |")

    lines += ["", "## Tool bugs found", "",
              "Not wired in S1 (diagnose.dl + query_log join is stage S3 per the plan); "
              "this section is a placeholder so the report template shape is stable "
              "before S3 fills it."]

    with open(out_prefix + ".md", "w") as f:
        f.write("\n".join(lines) + "\n")


def main():
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raw_dir, experiment_toml_path, out_prefix = sys.argv[1:4]
    experiment = load_toml(experiment_toml_path)
    window = int(experiment.get("filters", {}).get("score_window_lines", 2))

    os.makedirs(os.path.dirname(out_prefix) or ".", exist_ok=True)
    if os.path.exists(out_prefix + ".md") or os.path.exists(out_prefix + ".jsonl"):
        print(f"score.py: refusing to overwrite existing {out_prefix}.md/.jsonl "
              "(results are never overwritten — pick a new out_prefix)", file=sys.stderr)
        raise SystemExit(1)

    expected_cache = {}
    rows = []
    raw_paths = sorted(glob.glob(os.path.join(raw_dir, "*__*__rep*.json"))
                        + glob.glob(os.path.join(raw_dir, "*__*__rep*.error")))
    for raw_path in raw_paths:
        task_id, cell, rep = parse_cell_rep_name(raw_path)
        if task_id not in expected_cache:
            expected_cache[task_id] = load_expected(raw_dir, task_id)
        expected = expected_cache[task_id]
        row = score_one(raw_path, expected, window)
        row.update({
            "task_id": task_id, "cell": cell, "rep": rep,
            "class": expected.get("class"), "mutation_kind": expected.get("mutation_kind"),
            "expected_file": expected["expected_json"]["file"] if expected.get("expected_json") else None,
            "expected_line": expected["expected_json"]["line"] if expected.get("expected_json") else None,
        })
        rows.append(row)

    with open(out_prefix + ".jsonl", "w") as f:
        for row in rows:
            f.write(json.dumps(row, sort_keys=True))
            f.write("\n")

    render_report(rows, experiment, out_prefix)
    print(f"score.py: {len(rows)} rows -> {out_prefix}.jsonl + {out_prefix}.md", file=sys.stderr)


if __name__ == "__main__":
    main()
