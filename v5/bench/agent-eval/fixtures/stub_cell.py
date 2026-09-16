#!/usr/bin/env python3
"""stub_cell.py <raw_dir> <task_id> <cell_name> <rep>

The `--dry-run` model stand-in: NOT a real model call. run.sh's --dry-run
path invokes this instead of `claude -p` so the full generate -> mutate ->
"cell" -> score pipeline runs end to end without spending anything, while
still exercising every scoring branch (exact hit, window-tolerance hit, wrong
file, and unparsable prose -> parse failure).

Reads the REAL expected_json that mutate.sh already wrote to
`<raw_dir>/<task_id>.expected.json` (mutate.sh always runs for real, even
under --dry-run — only the model call is stubbed) and fabricates one of four
canned answers, chosen deterministically (sha256 of task_id+cell_name+rep,
NOT random, matching the harness's own no-randomness rule) so a --dry-run
rerun is byte-identical.

Prints a `claude -p --output-format json`-shaped blob on stdout (the same
shape score.py parses for a real cell), so score.py needs no dry-run special
case.
"""
import hashlib
import json
import os
import sys


def main():
    if len(sys.argv) != 5:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raw_dir, task_id, cell_name, rep = sys.argv[1:5]

    with open(os.path.join(raw_dir, f"{task_id}.expected.json")) as f:
        expected = json.load(f)["expected_json"]

    digest = hashlib.sha256(f"{task_id}:{cell_name}:{rep}".encode()).hexdigest()
    variant = int(digest[:8], 16) % 4

    # CTF tasks expect a LIST of sites -> stub a {"sites":[...]} answer that
    # exercises the F1 scorer (perfect set, partial recall, extra false
    # positive, and prose parse-failure).
    if isinstance(expected, list):
        if variant == 0:
            result_text = json.dumps({"sites": expected})                       # perfect
        elif variant == 1:
            result_text = json.dumps({"sites": expected[:-1]})                  # miss one (recall)
        elif variant == 2:
            result_text = json.dumps({"sites": expected + [                     # extra FP (precision)
                {"file": "app-ts/src/routes/report.ts", "line": 19}]})
        else:
            result_text = "I could not enumerate the enforcement sites confidently."
        blob = {"type": "result", "subtype": "success", "is_error": False,
                "result": result_text,
                "usage": {"input_tokens": 1200, "output_tokens": 60},
                "total_cost_usd": 0.001, "duration_ms": 500, "num_turns": 1}
        print(json.dumps(blob))
        return

    if variant == 0:
        # exact hit
        result_text = json.dumps({"file": expected["file"], "line": expected["line"]})
    elif variant == 1:
        # window-tolerance hit: off by the window's own edge (score_window_lines
        # is 2 in experiment.toml; +2 is still "located").
        result_text = json.dumps({"file": expected["file"], "line": expected["line"] + 2})
    elif variant == 2:
        # wrong file entirely -> not solved, but a clean parse.
        result_text = json.dumps({"file": "src/definitely_not_it.rs", "line": expected["line"]})
    else:
        # unparsable: prose with no embedded JSON object at all.
        result_text = ("I looked through the repository but could not pin down a single "
                        "defect location with confidence.")

    blob = {
        "type": "result", "subtype": "success", "is_error": False,
        "result": result_text,
        "usage": {"input_tokens": 1000, "output_tokens": 40},
        "total_cost_usd": 0.001,
        "duration_ms": 500,
        "num_turns": 1,
    }
    print(json.dumps(blob))


if __name__ == "__main__":
    main()
