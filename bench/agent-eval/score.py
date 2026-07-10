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


def _coerce_sites(answer):
    """Normalize a model answer into a list of {file, line} dicts. Accepts a
    {"sites":[...]} object, a bare list of sites, or a single site object."""
    if isinstance(answer, dict):
        if isinstance(answer.get("sites"), list):
            raw = answer["sites"]
        elif "file" in answer and "line" in answer:
            raw = [answer]
        else:
            raw = []
    elif isinstance(answer, list):
        raw = answer
    else:
        raw = []
    sites = []
    for item in raw:
        if not isinstance(item, dict):
            continue
        file = item.get("file")
        line = item.get("line")
        try:
            line = int(line)
        except (TypeError, ValueError):
            continue
        if isinstance(file, str):
            sites.append((file, line))
    return sites


def score_ctf(answer, expected_sites, window):
    """Set-F1 of predicted gate sites vs the manifest's expected sites. A
    predicted (file, line) matches an expected one iff the file is identical and
    the line is within +/- window; each expected site is matched at most once
    (greedy). Returns (precision, recall, f1, true_positives, n_pred)."""
    pred = _coerce_sites(answer)
    expected = [(s["file"], int(s["line"])) for s in expected_sites]
    used = [False] * len(pred)
    tp = 0
    for exp_file, exp_line in expected:
        for i, (pf, pl) in enumerate(pred):
            if used[i]:
                continue
            if pf == exp_file and abs(pl - exp_line) <= window:
                used[i] = True
                tp += 1
                break
    n_pred = len(pred)
    n_exp = len(expected)
    precision = tp / n_pred if n_pred else (1.0 if n_exp == 0 else 0.0)
    recall = tp / n_exp if n_exp else (1.0 if n_pred == 0 else 0.0)
    f1 = (2 * precision * recall / (precision + recall)) if (precision + recall) else 0.0
    return precision, recall, f1, tp, n_pred


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
    elif expected.get("class") == "CTF":
        precision, recall, f1, tp, n_pred = score_ctf(
            answer, expected["expected_json"], window)
        row["precision"] = precision
        row["recall"] = recall
        row["f1"] = f1
        row["true_positives"] = tp
        row["n_pred"] = n_pred
        row["n_expected"] = len(expected["expected_json"])
        # "solved" is reserved for a PERFECT inventory (F1 == 1.0); the headline
        # metric for CTF is mean F1, not this flag.
        row["solved"] = f1 >= 0.999
    return row


def median_binary(outcomes):
    return statistics.median(outcomes) >= 0.5


def _cell_meta(experiment):
    """cell_name -> (model, is_dl) from the experiment's cell list."""
    meta = {}
    for c in experiment.get("cells", []):
        meta[c["name"]] = (c.get("model"), bool(c.get("mcp")) or c["name"].endswith("-dl"))
    return meta


def _mean(xs):
    return sum(xs) / len(xs) if xs else 0.0


def render_ctf(rows, experiment, out_prefix):
    """v2 CTF report: set-F1 of gate-site inventories per cell, plus the
    dl-vs-baseline separation verdict the pilot exists to answer."""
    version = experiment.get("version", "unversioned")
    window = int(experiment.get("filters", {}).get("score_window_lines", 2))
    meta = _cell_meta(experiment)
    cells = sorted({r["cell"] for r in rows})
    tasks = sorted({r["task_id"] for r in rows})

    # (cell, task) -> aggregated over reps.
    agg = {}
    for cell in cells:
        for task in tasks:
            reps = [r for r in rows if r["cell"] == cell and r["task_id"] == task]
            if not reps:
                continue
            agg[(cell, task)] = {
                "f1": _mean([r.get("f1", 0.0) for r in reps]),
                "precision": _mean([r.get("precision", 0.0) for r in reps]),
                "recall": _mean([r.get("recall", 0.0) for r in reps]),
                "prose_fail": any(r["method"] == "parse-failure" for r in reps),
                "err": any(r["method"] == "invocation-error" for r in reps),
                "n_pred": _mean([r.get("n_pred", 0) for r in reps]),
                # n_expected_sites is set for every CTF row (from the expected
                # file), unlike n_expected which only lands on a parsed row.
                "n_expected": reps[0].get("n_expected_sites")
                    or reps[0].get("n_expected", 0),
            }

    lines = [
        f"# Agent eval harness — {version} (CTF: dataflow capture-the-flag)",
        "",
        f"Set-F1 vs a hand-built manifest, +/- {window}-line window. "
        f"Rows: {len(rows)}. Tasks: {len(tasks)}. Cells: {', '.join(cells)}.",
        "",
    ]
    note = experiment.get("note")
    if note:
        lines += [f"> **Protocol note.** {note}", ""]

    # Headline: mean F1 / precision / recall per cell.
    lines += [
        "## Headline (mean set-F1 over tasks)",
        "",
        "Each task asks for a SET of enforcement sites; F1 is the set overlap "
        "with the manifest (a predicted `file:line` matches an expected one "
        "within the window). prose-fail = model answered in prose; timeout/err "
        "= `claude` exited nonzero. Both score F1=0.",
        "",
        "| cell | mean F1 | mean precision | mean recall | perfect (F1=1) | prose-fail | timeout/err | tasks |",
        "|---|---|---|---|---|---|---|---|",
    ]
    cell_taskf1 = {}
    for cell in cells:
        vals = [agg[(cell, t)] for t in tasks if (cell, t) in agg]
        f1s = [v["f1"] for v in vals]
        cell_taskf1[cell] = {t: agg[(cell, t)]["f1"] for t in tasks if (cell, t) in agg}
        perfect = sum(1 for v in vals if v["f1"] >= 0.999)
        prose = sum(1 for v in vals if v["prose_fail"])
        err = sum(1 for v in vals if v["err"])
        n = len(vals)
        lines.append(
            f"| {cell} | {_mean(f1s):.3f} | {_mean([v['precision'] for v in vals]):.3f} "
            f"| {_mean([v['recall'] for v in vals]):.3f} | {perfect}/{n} "
            f"| {prose}/{n} | {err}/{n} | {n} |"
        )

    # Per-task F1 matrix.
    lines += ["", "## Per-task F1", "",
              "| task | expected | " + " | ".join(cells) + " |",
              "|---|---|" + "|".join(["---"] * len(cells)) + "|"]
    for task in tasks:
        any_cell = next((c for c in cells if (c, task) in agg), None)
        n_exp = agg[(any_cell, task)]["n_expected"] if any_cell else 0
        cellvals = " | ".join(
            f"{agg[(c, task)]['f1']:.2f}" if (c, task) in agg else "-" for c in cells)
        lines.append(f"| {task} | {n_exp} | {cellvals} |")

    # Cost.
    lines += ["", "## Cost", "", "| cell | total cost (usd) | mean F1 | cost / task |", "|---|---|---|---|"]
    for cell in cells:
        cell_rows = [r for r in rows if r["cell"] == cell]
        total_cost = sum(r["cost_usd"] or 0.0 for r in cell_rows)
        f1s = [agg[(cell, t)]["f1"] for t in tasks if (cell, t) in agg]
        per_task = total_cost / len(f1s) if f1s else float("nan")
        lines.append(f"| {cell} | ${total_cost:.4f} | {_mean(f1s):.3f} | ${per_task:.4f} |")

    # Separation verdict: per model, dl cell vs baseline cell mean F1.
    lines += ["", "## Separation verdict (dl vs baseline, same model)", ""]
    models = sorted({m for (m, _) in meta.values() if m})
    any_pair = False
    for model in models:
        model_cells = [c for c in cells if meta.get(c, (None, None))[0] == model]
        dl_cell = next((c for c in model_cells if meta[c][1]), None)
        base_cell = next((c for c in model_cells if not meta[c][1]), None)
        if not dl_cell or not base_cell:
            continue
        any_pair = True
        dl_f1 = _mean([cell_taskf1[dl_cell][t] for t in tasks if t in cell_taskf1.get(dl_cell, {})])
        base_f1 = _mean([cell_taskf1[base_cell][t] for t in tasks if t in cell_taskf1.get(base_cell, {})])
        delta = dl_f1 - base_f1
        verdict = ("dl SEPARATES (+)" if delta > 0.05 else
                   "baseline ahead (-)" if delta < -0.05 else "no separation (~)")
        lines.append(f"### {model}: {base_cell} F1={base_f1:.3f} vs {dl_cell} F1={dl_f1:.3f} "
                     f"-> delta {delta:+.3f} ({verdict})")
        lines.append("")
        lines.append("| task | " + f"{base_cell}" + " | " + f"{dl_cell}" + " | delta |")
        lines.append("|---|---|---|---|")
        for task in tasks:
            b = cell_taskf1.get(base_cell, {}).get(task)
            d = cell_taskf1.get(dl_cell, {}).get(task)
            if b is None or d is None:
                continue
            lines.append(f"| {task} | {b:.2f} | {d:.2f} | {d - b:+.2f} |")
        lines.append("")
    if not any_pair:
        lines.append("(no dl/baseline cell pair sharing a model in this run)")

    # Invocation errors detail.
    err_rows = [r for r in rows if r["method"] == "invocation-error"]
    lines += ["", "## Invocation errors (timeouts)", "",
              f"{len(err_rows)} total (exit 124 = per-cell wall-clock cap; scored F1=0)."]
    if err_rows:
        lines.append("\n| cell | task_id | note |")
        lines.append("|---|---|---|")
        for r in sorted(err_rows, key=lambda x: (x["cell"], x["task_id"])):
            note_txt = (r.get("raw_note") or "").replace("\n", " ")[:60]
            lines.append(f"| {r['cell']} | {r['task_id']} | {note_txt} |")
    else:
        lines.append("\n(none)")

    with open(out_prefix + ".md", "w") as f:
        f.write("\n".join(lines) + "\n")


def render_report(rows, experiment, out_prefix):
    if experiment.get("class", "C2") == "CTF":
        return render_ctf(rows, experiment, out_prefix)
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
    ]
    note = experiment.get("note")
    if note:
        lines += [f"> **Protocol note.** {note}", ""]
    lines += [
        "## Headline (median-of-reps C2 locate rate)",
        "",
        "Two distinct failure modes are reported separately (protocol #6: a "
        "contract problem must not masquerade as knowledge). **prose-fail** = the "
        "model returned a well-formed response with no extractable JSON object "
        "(answered in prose). **timeout/err** = the `claude` invocation itself "
        "exited nonzero (exit 124 = the per-cell wall-clock `timeout_secs` cap; a "
        "slow path, e.g. the agent shelling out to `cargo`/`tsc` on a large "
        "corpus — NOT a model-knowledge signal). Both count as not-solved.",
        "",
        "| cell | median locate rate | raw per-rep rate | prose-fail rate | timeout/err rate | tasks | reps |",
        "|---|---|---|---|---|---|---|",
    ]
    cell_medians = {}
    for cell in cells:
        task_ids = sorted({t for (c, t) in by_cell_task if c == cell})
        medians = []
        raw_outcomes = []
        prose_fail = []
        invoke_err = []
        for task_id in task_ids:
            reps = by_cell_task[(cell, task_id)]
            outcomes = [1 if r["solved"] else 0 for r in reps]
            medians.append(1 if median_binary(outcomes) else 0)
            raw_outcomes.extend(outcomes)
            # prose parse-failure: a real response blob whose text had no JSON.
            prose_fail.extend([1 if (not r["parse_ok"] and r["method"] == "parse-failure") else 0 for r in reps])
            # invocation error / timeout: the .error path (nonzero claude exit).
            invoke_err.extend([1 if r["method"] == "invocation-error" else 0 for r in reps])
        median_rate = sum(medians) / len(medians) if medians else 0.0
        raw_rate = sum(raw_outcomes) / len(raw_outcomes) if raw_outcomes else 0.0
        prose_rate = sum(prose_fail) / len(prose_fail) if prose_fail else 0.0
        err_rate = sum(invoke_err) / len(invoke_err) if invoke_err else 0.0
        cell_medians[cell] = {task_id: m for task_id, m in zip(task_ids, medians)}
        n_reps = len(by_cell_task[(cell, task_ids[0])]) if task_ids else 0
        lines.append(
            f"| {cell} | {median_rate:.2%} | {raw_rate:.2%} | {prose_rate:.2%} "
            f"| {err_rate:.2%} | {len(task_ids)} | {n_reps} |"
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

    # Invocation errors (timeouts) detail: which (cell, task) hit the wall
    # clock, so a depressed locate rate can be read against a slow-path cause
    # rather than mistaken for a knowledge gap.
    err_rows = [r for r in rows if r["method"] == "invocation-error"]
    lines += ["", "## Invocation errors (timeouts)", "",
              "Cells whose `claude` process exited nonzero (exit 124 = the "
              f"per-cell wall-clock cap). Scored not-solved. {len(err_rows)} total."]
    if not err_rows:
        lines.append("\n(none)")
    else:
        lines.append("\n| cell | task_id | note |")
        lines.append("|---|---|---|")
        for r in sorted(err_rows, key=lambda x: (x["cell"], x["task_id"])):
            note = (r.get("raw_note") or "").replace("\n", " ")[:60]
            lines.append(f"| {r['cell']} | {r['task_id']} | {note} |")

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
        # C2 carries a single expected {file,line}; CTF carries a LIST of sites.
        exp_json = expected.get("expected_json")
        exp_file = exp_json["file"] if isinstance(exp_json, dict) else None
        exp_line = exp_json["line"] if isinstance(exp_json, dict) else None
        row.update({
            "task_id": task_id, "cell": cell, "rep": rep,
            "class": expected.get("class"), "mutation_kind": expected.get("mutation_kind"),
            "expected_file": exp_file, "expected_line": exp_line,
            "n_expected_sites": len(exp_json) if isinstance(exp_json, list) else None,
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
