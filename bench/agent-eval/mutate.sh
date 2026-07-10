#!/usr/bin/env bash
# mutate.sh — apply ONE C2 seeded-fault mutation to a fresh git worktree, or
# clean that worktree up. This is the anti-grift protocol's ground-truth
# authority (plans/2026-07-10-agent-eval-harness.md #1): it NEVER calls dl.
# The answer key (expected_json) is computed by plain text tools (a small
# stdlib-only python script, mutate_apply.py) against the file AFTER
# mutation, so a model under test cannot win by reverse-engineering dl's own
# graph — the key comes from the mutation itself, not from re-querying dl.
#
# Usage:
#   mutate.sh apply   <task.json> <repo_root> <worktrees_dir>
#       Applies the task's mutation to a fresh worktree at
#       <worktrees_dir>/<id>, writes <worktrees_dir>/<id>/task.json (the
#       original task fields plus expected_json + worktree_root), and prints
#       that path on stdout.
#   mutate.sh cleanup <id> <repo_root> <worktrees_dir>
#       Removes the worktree and prunes. Safe to call twice / on a missing
#       worktree (idempotent, since run.sh always cleans up after scoring
#       even on a failed cell).
#
# task.json (one line of gen-tasks' packed tasks.jsonl) shape:
#   {"id","class","mutation_kind","site_path","site_line","site_detail","prompt"}
#
# Mutation kinds (see gen-tasks.dl header for the candidate-site regexes):
#   import-break   corrupt a `use crate::a::b;` path with a one-token typo —
#                  same line, answer = (site_path, site_line).
#   export-orphan  strip the `pub ` prefix off a top-level `pub fn name(` —
#                  same line, answer = (site_path, site_line).
#   duplicate-sym  duplicate a whole top-level fn's body right after itself
#                  (brace-counted) — answer = the SECOND (newly inserted)
#                  definition's line, downstream of site_line.
#   off-by-one     nudge a `for x in 0..N {` loop's literal upper bound by
#                  +1 — same line, answer = (site_path, site_line).
#
# ANTI-SHORTCUT NOTE: the mutation lands as an UNCOMMITTED edit in the fresh
# worktree, and `git diff`/`git log` against that history would trivially
# reveal the exact line to any cell with a Bash tool regardless of model or
# dl — a shortcut that would swamp the experiment's signal (both cells would
# hit ~100% via `git status`, testing nothing). So `apply` deletes the
# worktree's `.git` pointer file AFTER mutating: the tree becomes a plain,
# historyless source snapshot ("here's a checkout with a bug in it"), and
# `cmd_cleanup`'s `git worktree remove` fallback + always-run `worktree
# prune` handle the resulting stale worktree-admin entry on the main repo
# side (verified by `git worktree list` staying clean — see run.sh's
# end-of-run check).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    echo "usage: $0 apply <task.json> <repo_root> <worktrees_dir>" >&2
    echo "       $0 cleanup <id> <repo_root> <worktrees_dir>" >&2
    exit 2
}

cmd="${1:-}"; [ -n "$cmd" ] || usage

cmd_cleanup() {
    local task_id="$1" repo_root="$2" worktrees_dir="$3"
    local wt="$worktrees_dir/$task_id"
    if [ -d "$wt" ]; then
        git -C "$repo_root" worktree remove --force "$wt" 2>/dev/null || rm -rf "$wt"
    fi
    git -C "$repo_root" worktree prune 2>/dev/null || true
}

cmd_apply() {
    local task_json_path="$1" repo_root="$2" worktrees_dir="$3"

    local task_id mutation_kind site_path site_line
    IFS=$'\t' read -r task_id mutation_kind site_path site_line <<< "$(
        python3 "$HERE/task_json.py" fields "$task_json_path"
    )"

    mkdir -p "$worktrees_dir"
    local wt="$worktrees_dir/$task_id"
    [ -d "$wt" ] && cmd_cleanup "$task_id" "$repo_root" "$worktrees_dir"
    # Fresh throwaway worktree, detached at HEAD; the mutation lands only here.
    git -C "$repo_root" worktree add --detach -q "$wt" HEAD

    local target_file="$wt/$site_path"
    [ -f "$target_file" ] || { echo "mutate.sh: $target_file missing in worktree" >&2; exit 1; }

    # The one text-tool mutation. Prints "<answer_file>\t<answer_line>" (the
    # answer key, computed post-mutation) on stdout; never invokes dl.
    local answer answer_file answer_line
    answer="$(python3 "$HERE/mutate_apply.py" "$mutation_kind" "$target_file" "$site_line" "$site_path")"
    answer_file="$(printf '%s' "$answer" | cut -f1)"
    answer_line="$(printf '%s' "$answer" | cut -f2)"

    # Anti-shortcut: no git history in the served tree (see header note).
    rm -f "$wt/.git"

    python3 "$HERE/task_json.py" write "$task_json_path" "$wt/task.json" "$wt" "$answer_file" "$answer_line"

    echo "$wt/task.json"
}

case "$cmd" in
    apply)   [ $# -eq 4 ] || usage; cmd_apply "$2" "$3" "$4" ;;
    cleanup) [ $# -eq 4 ] || usage; cmd_cleanup "$2" "$3" "$4" ;;
    *) usage ;;
esac
