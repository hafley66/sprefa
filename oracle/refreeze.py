"""Rewrite every oracle case's existing `expected` fields from the v8 Rust engine."""

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
        env={**os.environ, **(env or {}), "DL8_ENGINE": "rust"},
    )
    if not run.stdout:
        raise RuntimeError(f"{args}: no stdout; exit {run.returncode}: {run.stderr.decode()[-400:]}")
    try:
        return json.loads(run.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"{args}: invalid JSON; exit {run.returncode}: {run.stderr.decode()[-400:]}") from error


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
        if key not in got:
            raise RuntimeError(f"dl8 omitted expected field {key}")
        if expected[key] != got[key]:
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
        try:
            got = door(binary, ["read", source.name], {"DL8_READ_PATH": case["path"]})
        finally:
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
        try:
            got = door(binary, ["eval", program.name])
        finally:
            os.unlink(program.name)
        if replace(case, got):
            write_json(path, case)
            count += 1
    return count


# Lists inside `runtime_program` that a case stores as hunks against the baseline.
RUNTIME_SEQUENCES = [
    "/args/0/args/0",
    "/args/0/args/1",
    "/args/1/args/0",
    "/args/1/args/1",
    "/args/1/args/2",
    "/args/2",
    "/args/3",
]


def hunks(baseline, got):
    """Content-anchored splices turning `baseline` into `got`, order exact.

    A hunk names the baseline row it follows (`after`, null at the start), the
    baseline rows it drops, and the rows it adds. No index is stored, so a row
    added to the baseline elsewhere leaves the hunk unchanged.
    """
    import difflib

    want = [json.dumps(r, sort_keys=True) for r in baseline]
    have = [json.dumps(r, sort_keys=True) for r in got]
    out = []
    matcher = difflib.SequenceMatcher(None, want, have, autojunk=False)
    for tag, i1, i2, j1, j2 in matcher.get_opcodes():
        if tag == "equal":
            continue
        out.append(
            {
                "after": baseline[i1 - 1] if i1 else None,
                "remove": baseline[i1:i2],
                "add": got[j1:j2],
            }
        )
    if apply_hunks(baseline, out) != got:
        raise RuntimeError("hunks do not reproduce the output")
    return out


def apply_hunks(baseline, hunks):
    """Twin of `apply_hunks` in tests/it/_1_compile_oracles.rs."""
    out = []
    cursor = 0
    for hunk in hunks:
        if hunk["after"] is not None:
            found = baseline.index(hunk["after"], cursor)
            out += baseline[cursor : found + 1]
            cursor = found + 1
        out += hunk["add"]
        if baseline[cursor : cursor + len(hunk["remove"])] != hunk["remove"]:
            raise RuntimeError("hunk removes rows the baseline does not hold")
        cursor += len(hunk["remove"])
    return out + baseline[cursor:]


def reconstruct(baseline, delta):
    """Twin of the reconstruction in tests/it/_1_compile_oracles.rs."""
    term = json.loads(json.dumps(baseline))
    for path, hs in delta.items():
        parts = path.split("/")[1:]
        holder = pointer(term, "/" + "/".join(parts[:-1])) if len(parts) > 1 else term
        last = int(parts[-1]) if isinstance(holder, list) else parts[-1]
        holder[last] = apply_hunks(holder[last], hs)
    return term


def pointer(value, path):
    for part in path.split("/")[1:]:
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def write_rows(handle, value, path, sequences):
    """Compact JSON with one row per line inside each named sequence."""
    if path in sequences:
        handle.write("[\n")
        handle.write(",\n".join(json.dumps(r, sort_keys=True, separators=(",", ":")) for r in value))
        handle.write("\n]")
    elif isinstance(value, dict):
        handle.write("{")
        for n, key in enumerate(sorted(value)):
            handle.write(("," if n else "") + json.dumps(key) + ":")
            write_rows(handle, value[key], f"{path}/{key}", sequences)
        handle.write("}")
    elif isinstance(value, list):
        handle.write("[")
        for n, item in enumerate(value):
            handle.write("," if n else "")
            write_rows(handle, item, f"{path}/{n}", sequences)
        handle.write("]")
    else:
        handle.write(json.dumps(value, sort_keys=True, separators=(",", ":")))


def write_case(path, case):
    """Header keys compact, each hunk and each row on its own line."""
    sequences = {"/delta/compiler_rows"} | {
        f"/delta/runtime_program/{p}" for p in RUNTIME_SEQUENCES
    }
    with open(path, "w") as handle:
        handle.write("{")
        for n, key in enumerate(sorted(case)):
            handle.write(("," if n else "") + json.dumps(key) + ":")
            if key == "delta":
                delta = case[key]
                handle.write("{")
                for m, name in enumerate(sorted(delta)):
                    handle.write(("," if m else "") + json.dumps(name) + ":")
                    if name == "compiler_rows":
                        handle.write("[\n" + ",\n".join(json.dumps(h, sort_keys=True, separators=(",", ":")) for h in delta[name]) + "\n]")
                    else:
                        handle.write("{")
                        for k, (ptr, hs) in enumerate(sorted(delta[name].items())):
                            handle.write(("," if k else "") + json.dumps(ptr) + ":[\n")
                            handle.write(",\n".join(json.dumps(h, sort_keys=True, separators=(",", ":")) for h in hs) + "\n]")
                        handle.write("}")
                handle.write("}")
            else:
                handle.write(json.dumps(case[key], sort_keys=True, separators=(",", ":")))
        handle.write("}")


def compile_door(binary, sources, repo, arguments):
    arguments = [a.replace("<root>", sources).replace("<repo>", repo) for a in arguments]
    return json.loads(
        json.dumps(door(binary, ["compile"] + arguments))
        .replace(sources, "<root>")
        .replace(repo, "<repo>")
    )


def refreeze_compile(binary, directory):
    """`baseline.json` is the output of the empty program; a case stores the
    diagnostics verbatim and `compiler_rows` and `runtime_program` as hunks
    against it, which omits the rows that name its own empty source file. Only the keys a case already froze are rewritten; `program` is
    never frozen. Returns the number of case files rewritten."""
    sources = os.path.join(directory, "sources")
    repo = os.path.dirname(os.path.dirname(directory))
    base = compile_door(binary, sources, repo, ["<root>/empty.dl7"])
    baseline = {k: base[k] for k in ("compiler_rows", "runtime_program", "diagnostics")}
    # Rows naming the empty file itself belong to that input, not to the prelude.
    own = "<root>/empty.dl7"
    baseline["compiler_rows"] = [r for r in baseline["compiler_rows"] if own not in json.dumps(r)]
    for path in RUNTIME_SEQUENCES:
        holder = pointer(baseline["runtime_program"], os.path.dirname(path))
        last = int(os.path.basename(path)) if isinstance(holder, list) else os.path.basename(path)
        holder[last] = [r for r in holder[last] if own not in json.dumps(r)]
    with open(os.path.join(directory, "baseline.json"), "w") as handle:
        write_rows(handle, baseline, "", {"/compiler_rows"} | {"/runtime_program" + p for p in RUNTIME_SEQUENCES})
    count = 0
    for path in cases(os.path.join(directory, "cases")):
        case = read_json(path)
        got = compile_door(binary, sources, repo, case["input"]["arguments"])
        frozen = set(case.get("delta", {})) | set(case.get("expected", {}))
        expected = {"diagnostics": got["diagnostics"]}
        delta = {}
        for key in sorted(frozen - {"diagnostics"}):
            if key == "compiler_rows" and got[key]:
                delta[key] = hunks(baseline[key], got[key])
            elif key == "runtime_program" and got[key]:
                delta[key] = {
                    p: hunks(pointer(baseline[key], p), pointer(got[key], p))
                    for p in RUNTIME_SEQUENCES
                }
                if reconstruct(baseline[key], delta[key]) != got[key]:
                    raise RuntimeError("runtime_program shell differs from the baseline")
            else:
                expected[key] = got[key]
        fresh = {"input": case["input"], "expected": expected}
        if delta:
            fresh["delta"] = delta
        if fresh != case:
            write_case(path, fresh)
            count += 1
    return count


def main():
    binary = sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    numbered = lambda name: name[: -len(".json")].rsplit("_", 1)[-1].isdigit()
    for name, phase in PLAIN:
        keep = numbered if name == "macrotime" else (lambda name: True)
        print(name, "rewritten", refreeze_plain(binary, os.path.join(here, name), phase, keep))
        if skipped(os.path.join(here, name)):
            print(name, "skipped", sorted(skipped(os.path.join(here, name))))
    print("read", "rewritten", refreeze_read(binary, os.path.join(here, "read")))
    print("eval", "rewritten", refreeze_eval(binary, os.path.join(here, "eval")))
    print("compile", "rewritten", refreeze_compile(binary, os.path.join(here, "compile")))


main()
