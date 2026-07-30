#!/usr/bin/env python3
"""lines.py -- leg B's stamped SSE capture -> the shared line format.

Input (stdin): one `<elapsed_ms> data: <tick log line>` per line, exactly what
`curl -N /ticks | stamp.py` produces. Non-`data:` lines (SSE blank separators)
are skipped.

Output (stdout): the shared `<step> <name> <sign> <payload>` format described in
rxoracle/README.md section 1, with normalizations N1/N2/N4 (and N3 when asked)
applied per README section 3.

Exit 1 with a named message when the boundary guard fires (README section 2):
an event landing within guardMs of a step boundary is a badly authored case, and
saying so out loud is the whole point of the guard. Nothing here is allowed to
"fix" a diff; the only transformations it performs are the four declared ones.
"""

import argparse
import json
import sys


def step_of(elapsed_ms: int, step_ms: int, guard_ms: int, context: str) -> int:
    within = elapsed_ms % step_ms
    if within < guard_ms or within > step_ms - guard_ms:
        raise SystemExit(
            f"BOUNDARY STRADDLE: {context} landed at {elapsed_ms}ms, "
            f"{within}ms into a {step_ms}ms step, inside the {guard_ms}ms guard. "
            "Re-author the case so the event lands near a step midpoint."
        )
    return elapsed_ms // step_ms


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--step-ms", type=int, required=True)
    parser.add_argument("--guard-ms", type=int, required=True)
    parser.add_argument("--t0-ms", type=int, required=True, help="epoch ms of the step-0 midpoint minus stepMs/2")
    parser.add_argument("--drop-del", action="store_true", help="normalization N3")
    parser.add_argument("--show-internal", default="", help="comma separated __ rels kept despite N4")
    parser.add_argument("--offsets-file", default="", help="write each tick's measured step offset here")
    arguments = parser.parse_args()

    kept_internal = {name for name in arguments.show_internal.split(",") if name}
    by_step: dict[int, list[str]] = {}
    offsets: list[str] = []

    for raw in sys.stdin:
        raw = raw.rstrip("\n")
        if not raw:
            continue
        stamp, _, rest = raw.partition(" ")
        if not rest.startswith("data: "):
            continue
        payload_text = rest[len("data: "):]
        elapsed_ms = int(stamp) - arguments.t0_ms
        tick = json.loads(payload_text)
        deltas = tick.get("deltas", {})
        if not deltas:
            # An empty tick carries no rows. N1 folds ticks into steps, so a
            # tick that moved nothing contributes nothing and never needs a
            # step index -- which also keeps the guard off drain ticks.
            continue
        step = step_of(elapsed_ms, arguments.step_ms, arguments.guard_ms, f"tick {tick.get('tick')}")
        margin = min(elapsed_ms % arguments.step_ms, arguments.step_ms - (elapsed_ms % arguments.step_ms))
        offsets.append(
            f"tick {tick.get('tick'):>3}  t+{elapsed_ms:>6}ms  step {step:02d}  "
            f"{elapsed_ms % arguments.step_ms:>4}ms in  margin {margin - arguments.guard_ms:>4}ms over guard  "
            f"[{', '.join(sorted(deltas))}]"
        )
        bucket = by_step.setdefault(step, [])
        for rel_name in sorted(deltas):
            if rel_name.startswith("__") and rel_name not in kept_internal:
                continue  # N4
            rel = deltas[rel_name]
            for row in rel.get("add", []):
                bucket.append(f"{rel_name} + {json.dumps(row, separators=(',', ':'))}")
            if arguments.drop_del:
                continue  # N3
            for row in rel.get("del", []):
                bucket.append(f"{rel_name} - {json.dumps(row, separators=(',', ':'))}")

    if arguments.offsets_file:
        with open(arguments.offsets_file, "w", encoding="utf-8") as handle:
            handle.write("\n".join(offsets) + ("\n" if offsets else ""))

    for step in sorted(by_step):
        for entry in sorted(by_step[step]):  # N2
            sys.stdout.write(f"{step:02d} {entry}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
