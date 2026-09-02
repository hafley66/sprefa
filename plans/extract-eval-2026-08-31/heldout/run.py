#!/usr/bin/env python3
"""Held-out oracle harness: score sprefa-extract against each language's own
SCIP indexer on large real projects nobody tuned the resolvers against.

Every accuracy number this repo quotes comes from three checkouts
(plans/extract-bench-2026-08-29/COMMON.md). This measures the same protocol on
repos picked at random from GitHub, and on those same three, through ONE code
path, so the two sides are comparable by construction.

No accuracy number is written into this file. Every number it emits is measured.

Reproducibility: selection is a seeded shuffle over a committed candidate pool.
Same seed plus same POOL.<lang>.tsv gives the same repos in the same order.

Subcommands
    pool    --lang L                       refresh POOL.<lang>.tsv from gh search
    run     --lang L --seed N --count K     select, clone, score, delete
    tuning  --lang L                       score the tuning corpus through the same path
    report                                 render REPORT.md from SCORES.tsv

Protocol (plans/extract-bench-2026-08-29/COMMON.md):
    normal form  src_path  src_name  dst_path  dst_name, paths repo-relative
    recall       overlap / |oracle|
    precision    overlap / |ours|
    3-bucket     ours = matched + contradicted + unjudged, keyed on (src_path, src_name)
    measure id   {lang}.{family}.{tier}.{oracle}
"""

import argparse
import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
WORKTREE = HERE.parents[2]
EXTRACT = WORKTREE / "v6/sprefa-extract/target/release/extract"
SCORES = HERE / "SCORES.tsv"
SKIPS = HERE / "SKIPS.tsv"
CHECKOUTS = Path("/tmp/heldout-checkouts")

# scip_ensure.rs:344 gates on which(bin) before any fallback, so scip-go must
# be findable by name; this dir is not on the login PATH.
EXTRA_PATH = ["/Users/chrishafley/go/bin"]

DISK_FLOOR_GB = 15
SCIP_BUDGET_SECS = 900
RESOLVE_BUDGET_SECS = 600
CLONE_BUDGET_SECS = 600
MIN_SOURCE_FILES = 50

# One route for every language, so the oracle slot of the measure id is one
# word and the lang slot already says which indexer ran.
ORACLE = "scip"
FAMILY = "call"

# The tuning corpora reach SCORES.tsv through `tuning`, never as a held-out
# draw; hafley66 is not a held-out sample of anything.
EXCLUDED_REPOS = {
    "microsoft/typescript",
    "microsoft/typescript-go",
    "rust-lang/rust-analyzer",
}
EXCLUDED_OWNERS = {"hafley66"}

# Directories that hold code the project did not write. Both sides are scoped
# to the same file set, so this only decides what gets extracted at all.
SKIP_DIRS = {
    ".git", "node_modules", "vendor", "third_party", "thirdparty",
    "target", "dist", "build", "out", "testdata", ".venv", "venv",
    "site-packages", "__pycache__", ".tox", "bazel-out",
}


class Lang:
    def __init__(self, key, exts, markers, checker, tuning_root, gh_language):
        self.key = key
        self.exts = exts
        self.markers = markers
        self.checker = checker
        self.tuning_root = tuning_root
        self.gh_language = gh_language

    @property
    def tiers(self):
        return ["syntax", "checker"] if self.checker else ["syntax"]


LANGS = {
    "ts": Lang(
        "ts", {".ts", ".tsx", ".mts", ".cts"}, ["tsconfig.json", "package.json"],
        "--ts-checker", "/Users/chrishafley/projects/TypeScript-5.9", "typescript",
    ),
    "go": Lang(
        "go", {".go"}, ["go.mod"],
        None, "/Users/chrishafley/projects/typescript-go", "go",
    ),
    "rust": Lang(
        "rust", {".rs"}, ["Cargo.toml"],
        "--rust-checker", "/Users/chrishafley/projects/rust-analyzer", "rust",
    ),
    "python": Lang(
        "python", {".py"}, ["pyproject.toml", "setup.py", "setup.cfg"],
        None, None, "python",
    ),
}

SCORE_COLUMNS = [
    "repo", "lang", "corpus_class", "family", "tier", "oracle", "measure_id",
    "recall", "precision", "matched", "contradicted", "unjudged",
    "ours", "oracle_rows", "files", "wall_ms", "sha",
]
SKIP_COLUMNS = ["repo", "lang", "stage", "reason", "detail"]


# ── shell ────────────────────────────────────────────────────────────────────

def child_env():
    env = dict(os.environ)
    env["PATH"] = os.pathsep.join(EXTRA_PATH + [env.get("PATH", "")])
    env["SPREFA_SCIP_TIMEOUT_SECS"] = str(SCIP_BUDGET_SECS)
    return env


def run(argv, timeout, cwd=None, stdout_path=None):
    """Returns (returncode, stderr_tail, wall_ms). stdout goes to a file when
    asked: a whole-repo fact stream does not belong in a pipe buffer."""
    started = time.monotonic()
    sink = open(stdout_path, "wb") if stdout_path else subprocess.PIPE
    try:
        proc = subprocess.run(
            argv, cwd=cwd, env=child_env(), stdout=sink,
            stderr=subprocess.PIPE, timeout=timeout,
        )
        wall_ms = int((time.monotonic() - started) * 1000)
        tail = proc.stderr.decode("utf-8", "replace").strip().splitlines()
        return proc.returncode, (tail[-1] if tail else ""), wall_ms
    except subprocess.TimeoutExpired:
        wall_ms = int((time.monotonic() - started) * 1000)
        return 124, f"exceeded {timeout}s budget", wall_ms
    finally:
        if stdout_path:
            sink.close()


def free_gb(path):
    usage = shutil.disk_usage(path)
    return usage.free / (1024 ** 3)


# ── selection ────────────────────────────────────────────────────────────────

def gh_query(lang):
    return [
        "gh", "search", "repos",
        f"--language={lang.gh_language}",
        "--stars=>=200",
        "--archived=false",
        "--size=5000..200000",
        "--sort=stars",
        "--limit=200",
        "--json", "fullName,stargazersCount,size,defaultBranch",
    ]


def refresh_pool(lang):
    argv = gh_query(lang)
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=120)
    if proc.returncode != 0:
        sys.exit(f"gh search failed for {lang.key}: {proc.stderr.strip()}")
    rows = json.loads(proc.stdout)
    kept = [
        row for row in rows
        if row["fullName"].lower() not in EXCLUDED_REPOS
        and row["fullName"].split("/")[0].lower() not in EXCLUDED_OWNERS
    ]
    out = HERE / f"POOL.{lang.key}.tsv"
    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    with open(out, "w") as fh:
        fh.write(f"# candidate pool for lang={lang.key}, built {stamp} UTC\n")
        fh.write(f"# query: {' '.join(argv)}\n")
        fh.write("# gh --size is KB, so 5000..200000 is the 5 MB to 200 MB band\n")
        fh.write(f"# {len(rows)} returned, {len(kept)} kept after excluding "
                 f"the tuning corpora and hafley66\n")
        fh.write("full_name\tstars\tsize_kb\tdefault_branch\n")
        for row in kept:
            fh.write(f"{row['fullName']}\t{row['stargazersCount']}\t"
                     f"{row['size']}\t{row['defaultBranch']}\n")
    print(f"{out.name}: {len(kept)} candidates")
    return kept


def read_pool(lang):
    path = HERE / f"POOL.{lang.key}.tsv"
    if not path.exists():
        sys.exit(f"no pool for {lang.key}; run: run.py pool --lang {lang.key}")
    names = []
    for line in path.read_text().splitlines():
        if line.startswith("#") or line.startswith("full_name\t") or not line.strip():
            continue
        names.append(line.split("\t")[0])
    return names


def seeded_order(names, seed):
    """The whole pool in one deterministic order. Selection walks this order and
    takes repos that pass the mechanical gate; a repo that fails is recorded as
    a skip, never silently replaced, so the walk stays reproducible."""
    ordered = list(names)
    random.Random(seed).shuffle(ordered)
    return ordered


# ── file sets ────────────────────────────────────────────────────────────────

def source_files(root, lang):
    found = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS and not d.startswith(".")]
        for name in filenames:
            if name.endswith(".d.ts"):
                continue
            if os.path.splitext(name)[1] in lang.exts:
                found.append(os.path.join(dirpath, name))
    return sorted(found)


def argv_budget():
    """Half of ARG_MAX minus the environment, so a file list can never be the
    thing that kills a run."""
    try:
        arg_max = os.sysconf("SC_ARG_MAX")
    except (ValueError, OSError):
        arg_max = 262144
    env_bytes = sum(len(k) + len(v) + 2 for k, v in os.environ.items())
    return max(64000, int((arg_max - env_bytes) * 0.5))


def fit_to_argv(files, repo):
    """Deterministically subsample when the file list will not fit in one argv.
    Seeded on the repo name, so the same repo always keeps the same files."""
    budget = argv_budget()
    total = sum(len(f) + 1 for f in files)
    if total <= budget:
        return files, False
    keep = max(1, int(len(files) * budget / total))
    picked = random.Random(repo).sample(files, keep)
    return sorted(picked), True


# ── normal form ──────────────────────────────────────────────────────────────

def relp(root, path):
    if not path:
        return ""
    return os.path.relpath(path, root) if path.startswith(root) else path


def ours_rows(jsonl_path, root):
    """resolved_edge -> the 4-column normal form. Mirrors
    plans/extract-bench-2026-08-29/normalize.py resolved_to_tsv."""
    rows = set()
    with open(jsonl_path) as fh:
        for line in fh:
            if not line.strip():
                continue
            fact = json.loads(line)
            if fact.get("record") != "resolved_edge":
                continue
            rows.add("\t".join([
                relp(root, fact["caller_path"]), fact.get("caller_name") or "",
                relp(root, fact["callee_path"]), fact.get("callee_name") or "",
            ]))
    return rows


def oracle_rows(jsonl_path, root):
    """scip_fn_edge joined through scip_def/scip_name to the 4-column normal
    form. Mirrors normalize.py scip_to_call_tsv. Also returns the scip_skip
    rows, which are a skipped measurement and never a zero."""
    sym_file, sym_name, edges, skips = {}, {}, [], []
    with open(jsonl_path) as fh:
        for line in fh:
            if not line.strip():
                continue
            fact = json.loads(line)
            record = fact.get("record")
            if record == "scip_def":
                sym_file[fact["symbol"]] = fact["file"]
            elif record == "scip_name":
                sym_name[fact["symbol"]] = fact["name"]
            elif record == "scip_fn_edge":
                edges.append((fact["caller"], fact["callee"]))
            elif record == "scip_skip":
                skips.append(fact)
    rows = set()
    for caller, callee in edges:
        caller_file, callee_file = sym_file.get(caller), sym_file.get(callee)
        if caller_file is None or callee_file is None:
            continue
        rows.add("\t".join([
            relp(root, caller_file), sym_name.get(caller, ""),
            relp(root, callee_file), sym_name.get(callee, ""),
        ]))
    return rows, skips


def scope(rows, corpus_files):
    """Both endpoints inside the extracted file set. The oracle indexes the
    whole project and we were handed a file list; without this the oracle is
    scored on files we were never shown."""
    kept = set()
    for row in rows:
        cols = row.split("\t")
        if cols[0] in corpus_files and cols[2] in corpus_files:
            kept.add(row)
    return kept


# ── scoring, ported from v6/sprefa-extract/tests/bench/mod.rs ────────────────

def pct(part, whole):
    return 0.0 if whole == 0 else part * 100.0 / whole


def score(ours, oracle):
    overlap = ours & oracle
    judged = {tuple(row.split("\t")[:2]) for row in oracle}
    matched = contradicted = unjudged = 0
    for row in ours:
        if row in oracle:
            matched += 1
        elif tuple(row.split("\t")[:2]) in judged:
            contradicted += 1
        else:
            unjudged += 1
    return {
        "recall": pct(len(overlap), len(oracle)),
        "precision": pct(len(overlap), len(ours)),
        "matched": matched,
        "contradicted": contradicted,
        "unjudged": unjudged,
        "ours": len(ours),
        "oracle_rows": len(oracle),
    }


# ── ledgers ──────────────────────────────────────────────────────────────────

def append_row(path, columns, values):
    fresh = not path.exists()
    with open(path, "a") as fh:
        if fresh:
            if path.name == "SCORES.tsv":
                fh.write("# one row per (repo, lang, family, tier, oracle). "
                         "DATA ROWS ONLY: no rollup, no total, no median lives here.\n")
                fh.write("# recall = matched/oracle_rows, precision = matched/ours, "
                         "percent; ours = matched + contradicted + unjudged.\n")
                fh.write("# oracle = the language's own SCIP indexer via "
                         "`extract --family scip ROOT`; both sides scoped to the "
                         "`files` extracted.\n")
            fh.write("\t".join(columns) + "\n")
        fh.write("\t".join(str(values[c]) for c in columns) + "\n")


def record_skip(repo, lang, stage, reason, detail):
    append_row(SKIPS, SKIP_COLUMNS, {
        "repo": repo, "lang": lang, "stage": stage,
        "reason": reason, "detail": detail.replace("\t", " ")[:200],
    })
    print(f"  SKIP {repo} [{stage}] {reason}: {detail[:120]}")


def already_scored(repo, lang):
    if not SCORES.exists():
        return False
    for line in SCORES.read_text().splitlines():
        if line.startswith("#") or line.startswith("repo\t"):
            continue
        cols = line.split("\t")
        if cols[0] == repo and cols[1] == lang:
            return True
    return False


# ── the measurement ──────────────────────────────────────────────────────────

def measure(repo, lang, root, corpus_class, sha):
    """One repo, both tiers. Returns the number of score rows written."""
    root = str(Path(root).resolve())
    files = source_files(root, lang)
    if len(files) < MIN_SOURCE_FILES:
        record_skip(repo, lang.key, "eligibility", "too_few_source_files",
                    f"{len(files)} files with {sorted(lang.exts)}, floor {MIN_SOURCE_FILES}")
        return 0
    if not any((Path(root) / marker).exists() for marker in lang.markers):
        record_skip(repo, lang.key, "eligibility", "no_project_marker",
                    f"none of {lang.markers} at the root")
        return 0

    files, subsampled = fit_to_argv(files, repo)
    corpus_files = {relp(root, f) for f in files}
    work = Path("/tmp/heldout-work") / repo.replace("/", "_")
    work.mkdir(parents=True, exist_ok=True)

    # Oracle first: a repo the indexer cannot handle costs nothing else.
    oracle_jsonl = work / "oracle.jsonl"
    rc, tail, oracle_ms = run(
        [str(EXTRACT), "--family", "scip", root],
        timeout=SCIP_BUDGET_SECS + 120, stdout_path=oracle_jsonl,
    )
    if rc != 0:
        record_skip(repo, lang.key, "oracle", "indexer_error", f"rc={rc} {tail}")
        return 0
    oracle_all, skips = oracle_rows(oracle_jsonl, root)
    if skips:
        first = skips[0]
        record_skip(repo, lang.key, "oracle", first.get("reason", "scip_skip"),
                    f"{first.get('bin', '?')}: {first.get('detail', '')}")
        return 0
    oracle_scoped = scope(oracle_all, corpus_files)
    if not oracle_scoped:
        record_skip(repo, lang.key, "oracle", "empty_oracle",
                    f"{len(oracle_all)} raw scip call edges, 0 inside the extracted file set")
        return 0

    written = 0
    for tier in lang.tiers:
        # ABSOLUTE paths: failure-modes 106, relative paths silently drop
        # crate-root import edges.
        argv = [str(EXTRACT), "--resolve", "--family", FAMILY]
        if tier == "checker":
            argv += ["--project-root", root, lang.checker]
        argv += files
        ours_jsonl = work / f"ours.{tier}.jsonl"
        rc, tail, wall_ms = run(argv, timeout=RESOLVE_BUDGET_SECS, stdout_path=ours_jsonl)
        if rc != 0:
            record_skip(repo, lang.key, f"ours.{tier}", "resolve_error", f"rc={rc} {tail}")
            continue
        ours = scope(ours_rows(ours_jsonl, root), corpus_files)
        result = score(ours, oracle_scoped)
        result.update({
            "repo": repo, "lang": lang.key, "corpus_class": corpus_class,
            "family": FAMILY, "tier": tier, "oracle": ORACLE,
            "measure_id": f"{lang.key}.{FAMILY}.{tier}.{ORACLE}",
            "recall": f"{result['recall']:.2f}", "precision": f"{result['precision']:.2f}",
            "files": len(files), "wall_ms": wall_ms,
            "sha": sha + ("+subsampled" if subsampled else ""),
        })
        append_row(SCORES, SCORE_COLUMNS, result)
        print(f"  {result['measure_id']:<28} {repo:<34} "
              f"recall {result['recall']:>6}% precision {result['precision']:>6}% "
              f"({result['matched']}/{result['oracle_rows']} oracle, "
              f"{result['matched']}/{result['ours']} ours, {len(files)}f, {wall_ms}ms)")
        written += 1
    shutil.rmtree(work, ignore_errors=True)
    return written


def clone_and_measure(repo, lang):
    CHECKOUTS.mkdir(parents=True, exist_ok=True)
    free = free_gb(CHECKOUTS)
    if free < DISK_FLOOR_GB:
        sys.exit(f"disk floor: {free:.1f}G free, floor {DISK_FLOOR_GB}G")
    dest = CHECKOUTS / repo.replace("/", "_")
    shutil.rmtree(dest, ignore_errors=True)
    rc, tail, _ = run(
        ["git", "clone", "--depth", "1", f"https://github.com/{repo}.git", str(dest)],
        timeout=CLONE_BUDGET_SECS,
    )
    if rc != 0:
        record_skip(repo, lang.key, "clone", "clone_failed", tail)
        shutil.rmtree(dest, ignore_errors=True)
        return 0
    sha_proc = subprocess.run(["git", "rev-parse", "HEAD"], cwd=dest,
                              capture_output=True, text=True)
    sha = sha_proc.stdout.strip()[:12]
    try:
        return measure(repo, lang, dest, "heldout", sha)
    finally:
        # A scored checkout is deleted immediately: 12 large repos left behind
        # is the disk the machine does not have.
        shutil.rmtree(dest, ignore_errors=True)


def cmd_run(args):
    lang = LANGS[args.lang]
    order = seeded_order(read_pool(lang), args.seed)
    scored = 0
    for repo in order:
        if scored >= args.count:
            break
        if already_scored(repo, lang.key):
            print(f"  have {repo}, skipping")
            scored += 1
            continue
        print(f"[{lang.key} seed={args.seed}] {repo} ({free_gb('/'):.0f}G free)")
        if clone_and_measure(repo, lang) > 0:
            scored += 1
    print(f"{lang.key}: {scored}/{args.count} repos scored")


def cmd_tuning(args):
    lang = LANGS[args.lang]
    if not lang.tuning_root:
        sys.exit(f"{lang.key} has no tuning corpus; it was never tuned against one")
    root = Path(lang.tuning_root)
    if not root.is_dir():
        sys.exit(f"tuning corpus missing: {root}")
    sha_proc = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root,
                              capture_output=True, text=True)
    sha = sha_proc.stdout.strip()[:12] or "unknown"
    print(f"[{lang.key} tuning] {root.name}")
    measure(root.name, lang, root, "tuning", sha)


# ── report ───────────────────────────────────────────────────────────────────

def read_scores():
    if not SCORES.exists():
        sys.exit("no SCORES.tsv yet")
    rows = []
    for line in SCORES.read_text().splitlines():
        if line.startswith("#") or line.startswith("repo\t") or not line.strip():
            continue
        rows.append(dict(zip(SCORE_COLUMNS, line.split("\t"))))
    return rows


def read_skips():
    if not SKIPS.exists():
        return []
    rows = []
    for line in SKIPS.read_text().splitlines():
        if line.startswith("#") or line.startswith("repo\t") or not line.strip():
            continue
        rows.append(dict(zip(SKIP_COLUMNS, line.split("\t"))))
    return rows


def gap_table(rows):
    tuning = {r["measure_id"]: r for r in rows if r["corpus_class"] == "tuning"}
    heldout = {}
    for row in rows:
        if row["corpus_class"] == "heldout":
            heldout.setdefault(row["measure_id"], []).append(row)
    out = ["| measure id | tuning corpus | held-out median | gap | n held-out |",
           "|---|---|---|---|---|"]
    for measure_id in sorted(set(tuning) | set(heldout)):
        held = heldout.get(measure_id, [])
        tuned = tuning.get(measure_id)
        left = (f"{float(tuned['recall']):.2f}% / {float(tuned['precision']):.2f}%"
                if tuned else "not measured")
        if held:
            med_recall = statistics.median(float(r["recall"]) for r in held)
            med_prec = statistics.median(float(r["precision"]) for r in held)
            right = f"{med_recall:.2f}% / {med_prec:.2f}%"
            gap = (f"{med_recall - float(tuned['recall']):+.2f} pt / "
                   f"{med_prec - float(tuned['precision']):+.2f} pt"
                   if tuned else "no tuning row")
        else:
            right, gap = "not measured", "-"
        out.append(f"| {measure_id} | {left} | {right} | {gap} | {len(held)} |")
    return out


def detail_table(rows):
    out = ["| measure id | repo | class | files | recall | precision | "
           "matched | contradicted | unjudged | ours | oracle | wall_ms | sha |",
           "|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for row in sorted(rows, key=lambda r: (r["measure_id"], r["corpus_class"], r["repo"])):
        out.append("| " + " | ".join([
            row["measure_id"], row["repo"], row["corpus_class"], row["files"],
            row["recall"] + "%", row["precision"] + "%", row["matched"],
            row["contradicted"], row["unjudged"], row["ours"], row["oracle_rows"],
            row["wall_ms"], row["sha"],
        ]) + " |")
    return out


def skip_table(skips):
    if not skips:
        return ["No repo skipped."]
    out = ["| repo | lang | stage | reason | detail |", "|---|---|---|---|---|"]
    for row in skips:
        out.append("| " + " | ".join([
            row["repo"], row["lang"], row["stage"], row["reason"], row["detail"],
        ]) + " |")
    return out


def cmd_report(args):
    rows, skips = read_scores(), read_skips()
    print("## Overfit gap, recall / precision\n")
    print("\n".join(gap_table(rows)))
    print("\n## Per repo\n")
    print("\n".join(detail_table(rows)))
    print("\n## Skipped\n")
    print("\n".join(skip_table(skips)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    pool = sub.add_parser("pool")
    pool.add_argument("--lang", required=True, choices=sorted(LANGS))

    runner = sub.add_parser("run")
    runner.add_argument("--lang", required=True, choices=sorted(LANGS))
    runner.add_argument("--seed", type=int, required=True)
    runner.add_argument("--count", type=int, default=3)

    tune = sub.add_parser("tuning")
    tune.add_argument("--lang", required=True, choices=sorted(LANGS))

    sub.add_parser("report")

    args = parser.parse_args()
    if not EXTRACT.exists():
        sys.exit(f"no extract binary at {EXTRACT}")
    if args.cmd == "pool":
        refresh_pool(LANGS[args.lang])
    elif args.cmd == "run":
        cmd_run(args)
    elif args.cmd == "tuning":
        cmd_tuning(args)
    elif args.cmd == "report":
        cmd_report(args)


if __name__ == "__main__":
    main()
