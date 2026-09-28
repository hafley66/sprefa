"""Run committed compile oracle cases with a per-case wall-clock cap."""

import json
import os
from pathlib import Path
import subprocess
import sys
import time


root = Path.cwd()
oracle = root / "oracle/compile"
sources = oracle / "sources"
exe = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/dl8"
selected = set(sys.argv[1:])
failed = False
for entry in json.loads((oracle / "status.json").read_text())["cases"]:
    if not entry["committed"] or (selected and entry["stem"] not in selected):
        continue
    stem = entry["stem"]
    case = json.loads((oracle / "cases" / f"{stem}.json").read_text())
    args = [
        arg.replace("<root>", str(sources)).replace("<repo>", str(root))
        for arg in case["input"]["arguments"]
    ]
    start = time.monotonic()
    try:
        run = subprocess.run(
            [str(exe), "compile", *args],
            env={**os.environ, "DL8_ENGINE": "ir:sqlite"},
            capture_output=True,
            timeout=900,
        )
    except subprocess.TimeoutExpired:
        print(f"TIMEOUT {stem} {time.monotonic() - start:.2f}s", flush=True)
        failed = True
        continue
    elapsed = time.monotonic() - start
    try:
        got = json.loads(
            run.stdout.decode().rstrip("\n")
            .replace(str(sources), "<root>")
            .replace(str(root), "<repo>")
        )
        matched = all(
            json.dumps(got.get(key), sort_keys=True) == json.dumps(value, sort_keys=True)
            for key, value in case["expected"].items()
        )
        matched &= run.returncode == int(bool(case["expected"]["diagnostics"]))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        matched = False
        print(f"ERROR {stem} {elapsed:.2f}s {error}; exit={run.returncode}", flush=True)
    if matched:
        print(f"PASS {stem} {elapsed:.2f}s", flush=True)
    else:
        if run.stdout:
            print(f"FAIL {stem} {elapsed:.2f}s; exit={run.returncode}", flush=True)
        failed = True

sys.exit(1 if failed else 0)
