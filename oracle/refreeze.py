"""Rewrite every oracle case's `expected` from dl8's own stdout.

The frozen JSON files are v8 goldens; this script runs the dl8
binary instead, so the committed bytes are dl8's goldens. Only the fields a
case already carries are replaced, which keeps each test comparing what it
compared before.
"""

import json
import os
import subprocess
import sys
import tempfile

# Every phase whose case file is passed straight to its door.
PLAIN = [
    ("macrotime", "expand"),
    ("lower", "lower"),
    ("check", "check"),
    ("load", "load"),
    ("comptime", "comptime"),
    ("reify", "reify"),
]


def door(binary, args, env=None):
    run = subprocess.run(
        [binary] + args,
        capture_output=True,
        env={**os.environ, **(env or {})},
    )
    if not run.stdout:
        raise RuntimeError(run.stderr.decode()[-400:])
    return json.loads(run.stdout)


def read_json(path):
    with open(path) as handle:
        return json.load(handle)


def write_json(path, case):
    """One compact line with no trailing newline, the shape the frozen dumps use."""
    with open(path, "w") as handle:
        json.dump(case, handle, sort_keys=True, separators=(",", ":"))


def skipped(directory):
    status = os.path.join(directory, "status.json")
    if not os.path.exists(status):
        return set()
    return set(read_json(status).get("skip", []))


def cases(directory, keep=lambda name: True):
    skip = skipped(directory)
    names = sorted(
        name
        for name in os.listdir(directory)
        if name.endswith(".json") and name != "status.json" and name not in skip
    )
    return [os.path.join(directory, name) for name in names if keep(name)]


def replace(case, got):
    """Only the keys the case already froze; a door prints more than that."""
    expected = case["expected"]
    changed = False
    for key in expected:
        if key in got and expected[key] != got[key]:
            expected[key] = got[key]
            changed = True
    return changed


def refreeze_plain(binary, directory, phase, keep=lambda name: True):
    count = 0
    for path in cases(directory, keep):
        case = read_json(path)
        if "expected" not in case:
            continue
        if replace(case, door(binary, [phase, path])):
            write_json(path, case)
            count += 1
    return count


def refreeze_read(binary, directory):
    count = 0
    for path in cases(directory):
        case = read_json(path)
        source = tempfile.NamedTemporaryFile("w", suffix=".dl7", delete=False)
        source.write(case["input"])
        source.close()
        got = door(binary, ["read", source.name], {"DL8_READ_PATH": case["path"]})
        os.unlink(source.name)
        if replace(case, got):
            write_json(path, case)
            count += 1
    return count


def refreeze_eval(binary, directory):
    count = 0
    for path in cases(directory):
        case = read_json(path)
        program = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
        json.dump(case["program"], program)
        program.close()
        got = door(binary, ["eval", program.name])
        os.unlink(program.name)
        if replace(case, got):
            write_json(path, case)
            count += 1
    return count


def refreeze_compile(binary, directory):
    """The compile test compares stdout key by key, with the source root elided.

    `replace` and not a whole-value assignment: `compile` prints `program`,
    which no case freezes and which puts the committed set over the 6 MB the
    test asserts.
    """
    sources = os.path.join(directory, "sources")
    repo = os.path.dirname(os.path.dirname(directory))
    count = 0
    for path in cases(os.path.join(directory, "cases")):
        case = read_json(path)
        arguments = [
            a.replace("<root>", sources).replace("<repo>", repo)
            for a in case["input"]["arguments"]
        ]
        got = json.loads(
            json.dumps(door(binary, ["compile"] + arguments))
            .replace(sources, "<root>")
            .replace(repo, "<repo>")
        )
        if replace(case, got):
            write_json(path, case)
            count += 1
    return count


def main():
    binary = sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    numbered = lambda name: name[: -len(".json")].rsplit("_", 1)[-1].isdigit()
    for name, phase in PLAIN:
        keep = numbered if name == "macrotime" else (lambda name: True)
        print(name, "rewritten", refreeze_plain(binary, os.path.join(here, name), phase, keep))
    print("read", "rewritten", refreeze_read(binary, os.path.join(here, "read")))
    print("eval", "rewritten", refreeze_eval(binary, os.path.join(here, "eval")))
    print("compile", "rewritten", refreeze_compile(binary, os.path.join(here, "compile")))


main()
