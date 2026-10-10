"""List ladder: one seed holding one list of N atoms; per N, the bootstrap levels and terms, phase ms, peak RSS.
Run from the repo root: python3 plans/v8/probes/list_ladder/run.py target/rcargo/aarch64-apple-darwin/debug/dl8"""
import json, os, re, resource, subprocess, sys, tempfile, time

binary = sys.argv[1]
sizes = [int(n) for n in (sys.argv[2:] or ["1", "16", "128", "1024", "4096"])]
print("n\tread_ms\tmacro_ms\tmacro_rows_sent\tlower_ms\tcheck_ms\tcomptime_ms\tboot_calls_max\tboot_terms\twall_ms\tmax_rss_mb\tdiagnostics")
for n in sizes:
    items = " ".join(f"\"f{i}\"" for i in range(n))
    source = (
        f"(files: (* (run: str) (list: any)))\n(files \"r\" [{items}])\n"
        "(copy: (* (run: str) (list: any)))\n(<- (copy ?Run ?List) (files ?Run ?List))\n"
    )
    with tempfile.NamedTemporaryFile("w", suffix=".dl7", delete=False) as handle:
        handle.write(source)
    env = dict(os.environ, RUST_LOG="dl8::ir=info,dl8::phase=info,dl8::macrotime=info", NO_COLOR="1")
    started = time.time()
    proc = subprocess.Popen([binary, "compile", handle.name], stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    out, err = proc.communicate()
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    wall = (time.time() - started) * 1000
    err = err.decode()
    boots = re.findall(r"ir bootstrap terms=(\d+) texts=\d+ (?:levels=(\d+) )?ms=(\d+)", err)
    levels = max((int(b[1] or 1) for b in boots), default=0)
    terms = sum(int(b[0]) for b in boots)
    boot_ms = sum(int(b[2]) for b in boots)
    phase = lambda name: sum(int(m) for m in re.findall(rf'name="{name}" ms=(\d+)', err))
    try:
        found = json.loads(out)["diagnostics"]
        diagnostics = len(found) if not found else json.dumps(found)[:160]
    except Exception:
        diagnostics = "no-json"
    sent = sum(int(m) for m in re.findall(r"macrotime wave wave=\d+ sent=(\d+)", err))
    print(f"{n}\t{phase('read')}\t{phase('macrotime')}\t{sent}\t{phase('lower')}\t{phase('check')}\t{phase('comptime')}\t{levels}\t{terms}\t{wall:.0f}\t{usage.ru_maxrss / 1048576:.0f}\t{diagnostics}", flush=True)
    os.unlink(handle.name)
