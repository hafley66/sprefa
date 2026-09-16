#!/usr/bin/env python3
"""Function-grain co-change on engine.rs.

The file-grain experiment missed the 44 db_only extraction target because
the partition lives BELOW file granularity — all 110 methods are in engine.rs.
This script maps git diffs to function names and builds a function-level
co-change matrix.

Ground truth: the 10 unanimous consensus clusters (god-object study) and
the dl field-coverage partition (db_only=44 / stateful=48 / pure_fn=18).

Question: do db_only methods co-change with EACH OTHER (not with stateful)?
If yes, the partition is evolutionarily real. If no, co-change can't see it
either and we need a different signal.

Method:
  - For each commit touching engine.rs, get the diff.
  - Git's hunk header (@@ ... @@) includes function context for Rust.
  - Parse the function name from each hunk header.
  - Build co-occurrence: which functions changed in the same commit.
"""
import subprocess, collections, re, os

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(ROOT)))
ENGINE = "src/engine.rs"

# Known partitions from god-object study
DB_ONLY = {
    "count_rows", "column_exists", "insert_spine_files", "insert_spine_strings",
    "insert_source_rows_for_paths", "load_edges", "load_file_meta", "load_rel_digest",
    "query_sql", "rebuild_legacy_module_rels", "rebuild_legacy_type_rels",
    "rebuild_legacy_call_rels", "refresh_module_rels", "refresh_type_rels",
    "refresh_call_rels", "ensure_meta", "flush_node_spine", "observe_ref",
    "rebuild_legacy_spine_rels", "refresh_spine_rels", "refresh_node_rels",
    "refresh_dataflow_rels", "refresh_doc_rels", "refresh_scip_rels",
    "refresh_changed_rels", "refresh_propose_rels",
    "save_file_meta", "save_rel_digest", "seed_rel_digests", "prune_unchanged_by_digest",
    "source_rule_digests", "rel_digest",
    "insert_module_rows", "insert_module_spans", "module_files_by_rev",
    "module_rows_for_rev", "hunk_new_range", "scip_name_defs",
    "rebuild_legacy_dataflow_rels", "rebuild_legacy_doc_rels",
    "rebuild_legacy_scip_rels", "rebuild_legacy_changed_rels",
    "rebuild_legacy_propose_rels", "collect_manifests",
}

# Sample of stateful methods (the core that touches rels/root/repos)
STATEFUL = {
    "new", "tick", "tick_paths", "parse_file", "reconcile_sources",
    "declare", "declare_all", "declare_builtins", "create_auto_indexes",
    "refresh_rel", "refresh_builtins", "rebuild_derived", "rebuild_closures",
    "eval_closure_seed_rule", "eval_scc_rule", "run_query", "query_one_sql",
    "refresh_cond_cache", "run_reaches_point", "any_closure_empty",
    "definition_targets", "hover", "diags", "module_imports",
    "same_package_uses", "source_paths", "rel_rows", "repo_relation",
    "run_gen", "run_gens", "apply_splices", "apply_cursors",
    "resolve_rev", "resolve_repo", "resolve_scan_repos", "resolve_scan_bindings",
    "ensure_cloned", "ensure_cloned_or_missing",
    "retract_path", "retract_paths",
    "set_query_json", "set_prime_tick", "set_root_implicit", "set_repos",
    "located_spans", "span_at", "string_spans", "work_file_id",
}

def get_commits_with_diffs():
    """Yield (commit, [fn names changed in that commit]) for engine.rs."""
    out = subprocess.check_output(
        ["git", "-C", REPO, "log", "--format=COMMIT %H", "--", ENGINE],
        text=True,
    )
    commits = [l.split()[1] for l in out.splitlines() if l.startswith("COMMIT ")]
    for commit in commits:
        diff = subprocess.check_output(
            ["git", "-C", REPO, "show", commit, "--format=", "--", ENGINE],
            text=True, stderr=subprocess.DEVNULL,
        )
        if not diff.strip():
            continue
        fns = set()
        for line in diff.splitlines():
            # Hunk header: @@ -old,count +new,count @@ <context>
            # Git's rust diff driver includes fn name in context
            if line.startswith("@@"):
                # Extract function context after second @@
                parts = line.split("@@", 2)
                if len(parts) >= 3:
                    ctx = parts[2].strip()
                    # Rust context: "fn function_name" or "impl Foo"
                    m = re.search(r'\bfn\s+([a-z_][a-z0-9_]*)', ctx)
                    if m:
                        fns.add(m.group(1))
                    m2 = re.search(r'\bimpl\s+\w+', ctx)
                    if m2 and not fns:
                        pass  # impl-level change, no specific fn
            # Also check added/removed lines for fn defs
            elif line.startswith(("+", "-")) and not line.startswith(("+++", "---")):
                m = re.search(r'\bfn\s+([a-z_][a-z0-9_]*)', line)
                if m:
                    fns.add(m.group(1))
        yield commit, fns

def main():
    commit_fns = list(get_commits_with_diffs())
    print(f"=== {len(commit_fns)} commits touching engine.rs ===\n")

    # Per-commit function change sets
    all_fns = set()
    for _, fns in commit_fns:
        all_fns |= fns

    # Classify
    db_in = all_fns & DB_ONLY
    st_in = all_fns & STATEFUL
    other = all_fns - DB_ONLY - STATEFUL
    print(f"Functions seen in diffs: {len(all_fns)}")
    print(f"  db_only (of 44):    {len(db_in)}")
    print(f"  stateful (of 48):    {len(st_in)}")
    print(f"  other/pure_fn:       {len(other)}")
    if db_in:
        print(f"  db_only names: {sorted(db_in)}")
    print()

    # Co-change matrix
    pair_counts = collections.Counter()
    fn_counts = collections.Counter()
    for _, fns in commit_fns:
        for f in fns:
            fn_counts[f] += 1
        fl = sorted(fns)
        for i in range(len(fl)):
            for j in range(i+1, len(fl)):
                pair_counts[(fl[i], fl[j])] += 1

    # The key question: do db_only methods co-change with EACH OTHER?
    print("=== db_only <-> db_only co-change ===")
    db_pairs = [(p, n) for p, n in pair_counts.most_common()
                if p[0] in DB_ONLY and p[1] in DB_ONLY]
    for (a, b), n in db_pairs[:15]:
        print(f"  {n:2d}  {a} <-> {b}")
    if not db_pairs:
        print("  (none)")
    print()

    # db_only <-> stateful co-change
    print("=== db_only <-> stateful co-change ===")
    ds_pairs = [(p, n) for p, n in pair_counts.most_common()
                if (p[0] in DB_ONLY and p[1] in STATEFUL) or
                   (p[0] in STATEFUL and p[1] in DB_ONLY)]
    for (a, b), n in ds_pairs[:15]:
        tag = "db<->st"
        print(f"  {n:2d}  {a} <-> {b}  [{tag}]")
    if not ds_pairs:
        print("  (none)")
    print()

    # stateful <-> stateful co-change
    print("=== stateful <-> stateful co-change (top 15) ===")
    ss_pairs = [(p, n) for p, n in pair_counts.most_common()
                if p[0] in STATEFUL and p[1] in STATEFUL]
    for (a, b), n in ss_pairs[:15]:
        print(f"  {n:2d}  {a} <-> {b}")
    if not ss_pairs:
        print("  (none)")
    print()

    # Summary stats
    db_db = sum(n for (a,b), n in pair_counts.items() if a in DB_ONLY and b in DB_ONLY)
    db_st = sum(n for (a,b), n in pair_counts.items()
                if (a in DB_ONLY and b in STATEFUL) or (a in STATEFUL and b in DB_ONLY))
    st_st = sum(n for (a,b), n in pair_counts.items() if a in STATEFUL and b in STATEFUL)
    print("=== partition signal summary ===")
    print(f"  db_only  <-> db_only   co-change events: {db_db}")
    print(f"  db_only  <-> stateful  co-change events: {db_st}")
    print(f"  stateful <-> stateful  co-change events: {st_st}")
    if db_db + db_st > 0:
        ratio = db_db / (db_db + db_st)
        print(f"  db_only internal cohesion rate: {ratio:.2f} (1.0 = always with each other, 0.0 = always with stateful)")
        print(f"  -> {'PARTITION VALIDATED' if ratio > 0.5 else 'PARTITION NOT VISIBLE (co-change cant see it)'}")

    # Top 20 all-function co-change pairs
    print("\n=== top 20 function co-change pairs (any) ===")
    for (a, b), n in pair_counts.most_common(20):
        tag = ""
        if a in DB_ONLY and b in DB_ONLY: tag = "[db-db]"
        elif a in STATEFUL and b in STATEFUL: tag = "[st-st]"
        elif (a in DB_ONLY and b in STATEFUL) or (a in STATEFUL and b in DB_ONLY): tag = "[db-st]"
        print(f"  {n:2d}  {a} <-> {b}  {tag}")

if __name__ == "__main__":
    main()
