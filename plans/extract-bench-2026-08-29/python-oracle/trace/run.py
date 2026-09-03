#!/usr/bin/env python3
"""Trace oracle for the PyCG micro-suite.

Every case's `main.py` runs in a subprocess under a call tracer
(`sys.monitoring`, PEP 669, on python 3.12+; `sys.setprofile` below that).
Each executed call whose caller and callee code objects both live inside the
case dir becomes one oracle row in the 4-column spelling of
`../oracle/<category>__<case>.call.tsv`:

  src_path  src_name  dst_path  dst_name

  path  = `<category>/<case>/<file rel to case dir>` (suite-relative)
  name  = last qualname segment; "" for module-level code; `<lambdaN>` with N
          the 1-based pre-order index of the lambda among the file's lambdas
          (the PyCG counter); `<locals>` and comprehension segments dropped.

Edge sources, deduplicated:
  CALL      the calling frame's code -> the callable's code (python function
            or bound method). Records the call site even when the body runs
            later (generators).
  PY_START  the nearest in-case python frame above the started frame -> the
            started code. Records calls made from C (map, sorted, a class
            call reaching __init__, a for loop reaching __iter__/__next__).
Class bodies and comprehension bodies are never callees. A class body is a
code object without CO_OPTIMIZED whose name is not `<module>`.

Outputs (in --out, default alongside this file):
  TRACE.tsv   case category src_path src_name dst_path dst_name
  RUNS.tsv    case category status detail edges python
Stdout: one line per case (`<case> <status> <edges>`), then a total line.

Usage:
  run.py [--suite DIR] [--out DIR] [--timeout SECONDS] [--python EXE]
  run.py --child CASE_DIR SUITE_DIR EDGES_FILE STATUS_FILE   (internal)
"""

from __future__ import annotations

import argparse
import os
import runpy
import subprocess
import sys
import tempfile
import traceback
from pathlib import Path
from types import CodeType, FrameType, FunctionType, MethodType
from typing import IO, Callable, Optional

HERE = Path(__file__).resolve().parent
SUITE_DEFAULT = HERE.parent / "suite"
CO_OPTIMIZED = 0x1
COMPREHENSION_NAMES = {"<genexpr>", "<listcomp>", "<dictcomp>", "<setcomp>"}


class CaseTracer:
    """Maps code objects to (suite-relative path, PyCG-style name) and records edges."""

    def __init__(self, case_dir: Path, suite: Path, edges_out: IO[str]) -> None:
        self.case_dir = case_dir.resolve()
        self.suite = suite.resolve()
        self.edges_out = edges_out
        self.edges: set[tuple[str, str, str, str]] = set()
        self.path_cache: dict[str, Optional[str]] = {}
        self.lambda_cache: dict[str, list[CodeType]] = {}

    def rel_path(self, filename: str) -> Optional[str]:
        cached = self.path_cache.get(filename, "unset")
        if cached != "unset":
            return cached
        rel: Optional[str] = None
        if filename and not filename.startswith("<"):
            candidate = Path(filename)
            if not candidate.is_absolute():
                candidate = self.case_dir / candidate
            candidate = candidate.resolve()
            if candidate.is_relative_to(self.case_dir):
                rel = candidate.relative_to(self.suite).as_posix()
        self.path_cache[filename] = rel
        return rel

    def lambdas_in_file(self, filename: str) -> list[CodeType]:
        found = self.lambda_cache.get(filename)
        if found is not None:
            return found
        found = []
        try:
            source = Path(filename).read_bytes()
            root = compile(source, filename, "exec", dont_inherit=True)
        except (OSError, SyntaxError, ValueError):
            self.lambda_cache[filename] = found
            return found

        def walk(code: CodeType) -> None:
            for const in code.co_consts:
                if isinstance(const, CodeType):
                    if const.co_name == "<lambda>":
                        found.append(const)
                    walk(const)

        walk(root)
        self.lambda_cache[filename] = found
        return found

    def lambda_name(self, code: CodeType) -> str:
        for index, candidate in enumerate(self.lambdas_in_file(code.co_filename), start=1):
            if candidate == code:
                return f"<lambda{index}>"
        return "<lambda>"

    def name_of(self, code: CodeType) -> Optional[tuple[str, str]]:
        rel = self.rel_path(code.co_filename)
        if rel is None:
            return None
        if code.co_name == "<module>":
            return rel, ""
        if code.co_name == "<lambda>":
            return rel, self.lambda_name(code)
        qualname = getattr(code, "co_qualname", code.co_name)
        segments = [seg for seg in qualname.split(".") if seg != "<locals>"]
        while segments and segments[-1] in COMPREHENSION_NAMES:
            segments.pop()
        return rel, (segments[-1] if segments else "")

    @staticmethod
    def is_def(code: CodeType) -> bool:
        if code.co_name in COMPREHENSION_NAMES or code.co_name == "<module>":
            return False
        return bool(code.co_flags & CO_OPTIMIZED)

    def record(self, caller: CodeType, callee: CodeType) -> None:
        if not self.is_def(callee):
            return
        dst = self.name_of(callee)
        if dst is None:
            return
        src = self.name_of(caller)
        if src is None:
            return
        row = (src[0], src[1], dst[0], dst[1])
        if row in self.edges:
            return
        self.edges.add(row)
        self.edges_out.write("\t".join(row) + "\n")
        self.edges_out.flush()

    def nearest_in_case_frame(self, frame: Optional[FrameType]) -> Optional[FrameType]:
        while frame is not None:
            if self.rel_path(frame.f_code.co_filename) is not None:
                return frame
            frame = frame.f_back
        return None

    def on_py_start(self, started: FrameType) -> None:
        caller = self.nearest_in_case_frame(started.f_back)
        if caller is not None:
            self.record(caller.f_code, started.f_code)

    def on_call(self, caller_code: CodeType, callable_obj: object) -> None:
        code = code_of_callable(callable_obj)
        if code is not None:
            self.record(caller_code, code)


def code_of_callable(obj: object) -> Optional[CodeType]:
    if isinstance(obj, FunctionType):
        return obj.__code__
    if isinstance(obj, MethodType) and isinstance(obj.__func__, FunctionType):
        return obj.__func__.__code__
    return None


def install_monitoring(tracer: CaseTracer) -> Callable[[], None]:
    monitoring = sys.monitoring
    tool = monitoring.PROFILER_ID
    monitoring.use_tool_id(tool, "pycg-trace-oracle")
    events = monitoring.events

    def py_start(code: CodeType, offset: int) -> None:
        frame = sys._getframe(1)
        while frame is not None and frame.f_code is not code:
            frame = frame.f_back
        if frame is not None:
            tracer.on_py_start(frame)

    def call(code: CodeType, offset: int, callable_obj: object, arg0: object) -> None:
        tracer.on_call(code, callable_obj)

    monitoring.register_callback(tool, events.PY_START, py_start)
    monitoring.register_callback(tool, events.CALL, call)
    monitoring.set_events(tool, events.PY_START | events.CALL)

    def uninstall() -> None:
        monitoring.set_events(tool, 0)
        monitoring.free_tool_id(tool)

    return uninstall


def install_setprofile(tracer: CaseTracer) -> Callable[[], None]:
    def profile(frame: FrameType, event: str, arg: object) -> None:
        if event == "call":
            tracer.on_py_start(frame)
        elif event == "c_call":
            tracer.on_call(frame.f_code, arg)

    sys.setprofile(profile)

    def uninstall() -> None:
        sys.setprofile(None)

    return uninstall


def child(case_dir: Path, suite: Path, edges_file: Path, status_file: Path) -> int:
    case_dir = case_dir.resolve()
    main_py = case_dir / "main.py"
    sys.dont_write_bytecode = True
    sys.path = [str(case_dir)] + [entry for entry in sys.path if Path(entry or ".").resolve() != HERE]
    os.chdir(case_dir)
    sys.argv = [str(main_py)]
    status = "ok"
    detail = ""
    with open(edges_file, "w") as edges_out:
        tracer = CaseTracer(case_dir, suite, edges_out)
        install = install_monitoring if hasattr(sys, "monitoring") else install_setprofile
        uninstall = install(tracer)
        try:
            runpy.run_path(str(main_py), run_name="__main__")
        except SystemExit as stop:
            if stop.code not in (None, 0):
                status = "error"
                detail = f"SystemExit({stop.code})"
        except BaseException as failure:  # the case's own crash is the recorded fact
            status = "error"
            last = traceback.extract_tb(failure.__traceback__)[-1:]
            where = f" at {Path(last[0].filename).name}:{last[0].lineno}" if last else ""
            detail = f"{type(failure).__name__}: {str(failure).splitlines()[0] if str(failure) else ''}{where}"
        finally:
            uninstall()
    status_file.write_text(f"{status}\t{detail}\n")
    return 0


def run_case(python: str, case_dir: Path, suite: Path, timeout: float, scratch: Path) -> tuple[str, str, list[tuple[str, ...]]]:
    name = f"{case_dir.parent.name}__{case_dir.name}"
    edges_file = scratch / f"{name}.edges.tsv"
    status_file = scratch / f"{name}.status"
    argv = [python, str(Path(__file__).resolve()), "--child", str(case_dir), str(suite), str(edges_file), str(status_file)]
    status = "ok"
    detail = ""
    try:
        proc = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=timeout)
        if status_file.exists():
            status, _, detail = status_file.read_text().rstrip("\n").partition("\t")
        else:
            status = "error"
            detail = f"child exit {proc.returncode}: {proc.stderr.strip().splitlines()[-1] if proc.stderr.strip() else ''}"
    except subprocess.TimeoutExpired:
        status = "timeout"
        detail = f"killed after {timeout:g}s"
    edges: list[tuple[str, ...]] = []
    if edges_file.exists():
        edges = [tuple(line.split("\t")) for line in edges_file.read_text().splitlines() if line.strip()]
    return status, detail, edges


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        return child(Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]), Path(sys.argv[5]))
    ap = argparse.ArgumentParser()
    ap.add_argument("--suite", default=str(SUITE_DEFAULT))
    ap.add_argument("--out", default=str(HERE))
    ap.add_argument("--timeout", type=float, default=10.0)
    ap.add_argument("--python", default=sys.executable)
    args = ap.parse_args()
    suite = Path(args.suite).resolve()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    version = subprocess.run([args.python, "--version"], capture_output=True, text=True).stdout.strip()

    cases = sorted(p.parent for p in suite.glob("*/*/callgraph.json"))
    trace_lines = ["case\tcategory\tsrc_path\tsrc_name\tdst_path\tdst_name"]
    run_lines = ["case\tcategory\tstatus\tdetail\tedges\tpython"]
    ran = 0
    total_edges = 0
    with tempfile.TemporaryDirectory() as scratch:
        for case_dir in cases:
            prefix = f"{case_dir.parent.name}/{case_dir.name}"
            category = case_dir.parent.name
            status, detail, edges = run_case(args.python, case_dir, suite, args.timeout, Path(scratch))
            for row in sorted(set(edges)):
                trace_lines.append("\t".join((prefix, category, *row)))
            run_lines.append(f"{prefix}\t{category}\t{status}\t{detail}\t{len(set(edges))}\t{version}")
            ran += status == "ok"
            total_edges += len(set(edges))
            print(f"{prefix}\t{status}\t{len(set(edges))}\t{detail}")
    (out_dir / "TRACE.tsv").write_text("\n".join(trace_lines) + "\n")
    (out_dir / "RUNS.tsv").write_text("\n".join(run_lines) + "\n")
    print(f"TOTAL cases {len(cases)} ok {ran} edges {total_edges} python {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
