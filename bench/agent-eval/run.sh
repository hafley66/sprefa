#!/usr/bin/env bash
# run.sh — the ONE command: generate -> mutate -> run cells -> score -> emit
# report (plans/2026-07-10-agent-eval-harness.md protocol #7). No manual step
# between generation and the committed report.
#
# Usage:
#   bench/agent-eval/run.sh [experiment.toml] [options]
#
# Options:
#   --dry-run            Stub every cell (fixtures/stub_cell.py, no `claude`
#                         call, no cost) — exercises the full pipeline incl.
#                         real mutation + real scoring. For CI / iteration.
#   --tasks-limit N       Only the first N sym-hash-ordered tasks (a
#                         SUBSET, not a re-sample — never use this for a
#                         headline number, only smoke/dry-run scoping).
#   --cells a,b,c         Only these cell names (from experiment.toml).
#   --reps N              Override experiment.toml's `reps` (e.g. a 1-rep
#                         smoke run) — always echoed in the run banner so a
#                         results file is never mistaken for the preregistered
#                         cadence.
#   --out-suffix TAG       Appended to the results filename (e.g. "smoke").
#
# Output: results/<date>-<sha>-<version>[-<out-suffix>].md + .jsonl, never
# overwritten (score.py refuses if the target already exists).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

TIMEOUT_BIN="$(command -v timeout || command -v gtimeout || true)"

usage() {
    echo "usage: $0 [experiment.toml] [--dry-run] [--tasks-limit N] [--cells a,b] [--out-suffix TAG]" >&2
    exit 2
}

EXPERIMENT_TOML=""
DRY_RUN=0
TASKS_LIMIT=""
CELLS_FILTER=""
OUT_SUFFIX=""
REPS_OVERRIDE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=1; shift ;;
        --tasks-limit) TASKS_LIMIT="$2"; shift 2 ;;
        --cells) CELLS_FILTER="$2"; shift 2 ;;
        --reps) REPS_OVERRIDE="$2"; shift 2 ;;
        --out-suffix) OUT_SUFFIX="$2"; shift 2 ;;
        -h|--help) usage ;;
        -*) echo "unknown option: $1" >&2; usage ;;
        *)
            if [ -z "$EXPERIMENT_TOML" ]; then EXPERIMENT_TOML="$1"; else echo "unexpected arg: $1" >&2; usage; fi
            shift ;;
    esac
done
[ -n "$EXPERIMENT_TOML" ] || EXPERIMENT_TOML="$HERE/experiment.toml"
EXPERIMENT_TOML="$(cd "$(dirname "$EXPERIMENT_TOML")" && pwd)/$(basename "$EXPERIMENT_TOML")"

CONFIG_JSON_FILE="$(mktemp -t ae-config-XXXX).json"
python3 "$HERE/toml2json.py" "$EXPERIMENT_TOML" > "$CONFIG_JSON_FILE"

VERSION="$(jq -r '.version' "$CONFIG_JSON_FILE")"
REPO_ROOT_REL="$(jq -r '.repo_root' "$CONFIG_JSON_FILE")"
REPS="$(jq -r '.reps' "$CONFIG_JSON_FILE")"
[ -n "$REPS_OVERRIDE" ] && REPS="$REPS_OVERRIDE"
TIMEOUT_SECS="$(jq -r '.timeout_secs' "$CONFIG_JSON_FILE")"
PARALLELISM="$(jq -r '.parallelism' "$CONFIG_JSON_FILE")"

# Task class selects the generation + worktree strategy. "C2" (default) is the
# seeded-fault path (gen-tasks.dl -> select_tasks.py -> mutate.sh, one mutated
# worktree per task shared across cells). "CTF" is the v2 capture-the-flag path
# (ctf_tasks.py over a committed MANIFEST.json, no mutation; each cell gets its
# own fixture copy plus a per-cell SKILL.md at the root). `fixture` / `manifest`
# are relative to bench/agent-eval/ (this dir) and only read for CTF.
TASK_CLASS="$(jq -r '.class // "C2"' "$CONFIG_JSON_FILE")"
FIXTURE_REL="$(jq -r '.fixture // ""' "$CONFIG_JSON_FILE")"
MANIFEST_REL="$(jq -r '.manifest // ""' "$CONFIG_JSON_FILE")"

# repo_root is relative to the harness's OWN repo root (bench/agent-eval/../..),
# not to cwd or to the experiment.toml's directory — so a committed
# `repo_root = "."` always means "this repo" regardless of where run.sh is
# invoked from, and a foreign-corpus experiment.toml (plan item 5) can point
# it at any other checkout with an absolute path.
HARNESS_REPO_ROOT="$(cd "$HERE/../.." && pwd)"
case "$REPO_ROOT_REL" in
    /*) REPO_ROOT="$(cd "$REPO_ROOT_REL" && pwd)" ;;
    *)  REPO_ROOT="$(cd "$HARNESS_REPO_ROOT/$REPO_ROOT_REL" && pwd)" ;;
esac

DL_BIN="${DL_BIN:-$REPO_ROOT/target/debug/dl}"
if [ ! -x "$DL_BIN" ] && [ -x "$REPO_ROOT/target/release/dl" ]; then
    DL_BIN="$REPO_ROOT/target/release/dl"
fi
[ -x "$DL_BIN" ] || { echo "run.sh: build dl first (cargo build --bin dl), or set DL_BIN" >&2; exit 1; }
MCP_PROG="$HERE/mcp-server.dl"
GEN_PROG="$HERE/gen-tasks.dl"

if [ "$DRY_RUN" = "0" ] && ! command -v claude >/dev/null 2>&1; then
    echo "run.sh: 'claude' not found in PATH and --dry-run not set; nothing to run" >&2
    exit 1
fi
if [ "$DRY_RUN" = "0" ] && [ -z "$TIMEOUT_BIN" ]; then
    echo "run.sh: neither 'timeout' nor 'gtimeout' found; cells will run without a hard wall-clock cap" >&2
fi

TS="$(date +%Y%m%d-%H%M%S)"
SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo nogit)"
RUN_TAG="${TS}-${SHA}-${VERSION}"
[ -n "$OUT_SUFFIX" ] && RUN_TAG="${RUN_TAG}-${OUT_SUFFIX}"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/agent-eval.XXXXXX")"
RAW_DIR="$WORKDIR/raw"; mkdir -p "$RAW_DIR"
WORKTREES_DIR="$WORKDIR/worktrees"
RESULTS_DIR="$HERE/results"; mkdir -p "$RESULTS_DIR"
OUT_PREFIX="$RESULTS_DIR/${RUN_TAG}"

echo "== agent-eval run $RUN_TAG =="
echo "   experiment  = $EXPERIMENT_TOML (version=$VERSION)"
echo "   repo_root   = $REPO_ROOT"
echo "   dl          = $DL_BIN"
echo "   workdir     = $WORKDIR"
echo "   dry_run     = $DRY_RUN"
echo "   reps        = $REPS$( [ -n "$REPS_OVERRIDE" ] && echo " (overridden, NOT the preregistered cadence)" )"

: > "$WORKDIR/task_ids.txt"
cleanup_all() {
    if [ "$TASK_CLASS" = "CTF" ]; then
        # CTF worktrees are plain fixture copies (no git worktree admin), so a
        # recursive remove is all that is needed.
        rm -rf "$WORKTREES_DIR" 2>/dev/null || true
        return 0
    fi
    if [ -s "$WORKDIR/task_ids.txt" ]; then
        while IFS= read -r task_id; do
            "$HERE/mutate.sh" cleanup "$task_id" "$REPO_ROOT" "$WORKTREES_DIR" >/dev/null 2>&1 || true
        done < "$WORKDIR/task_ids.txt"
    fi
}
trap cleanup_all EXIT

# ---- 1. generate ------------------------------------------------------------
echo "-- generate --"
if [ "$TASK_CLASS" = "CTF" ]; then
    [ -n "$FIXTURE_REL" ] || { echo "run.sh: CTF class needs a `fixture` in the toml" >&2; exit 1; }
    [ -n "$MANIFEST_REL" ] || { echo "run.sh: CTF class needs a `manifest` in the toml" >&2; exit 1; }
    FIXTURE_DIR="$HERE/$FIXTURE_REL"
    MANIFEST_PATH="$HERE/$MANIFEST_REL"
    [ -d "$FIXTURE_DIR" ] || { echo "run.sh: fixture dir $FIXTURE_DIR missing" >&2; exit 1; }
    [ -f "$MANIFEST_PATH" ] || { echo "run.sh: manifest $MANIFEST_PATH missing" >&2; exit 1; }
    python3 "$HERE/ctf_tasks.py" "$MANIFEST_PATH" "$WORKDIR/tasks.jsonl"
else
    ( cd "$REPO_ROOT" && "$DL_BIN" "$GEN_PROG" --no-daemon --db "$WORKDIR/gen.db" --query-json ) \
        2>"$WORKDIR/gen.log" | tail -1 > "$WORKDIR/candidates.json"
    python3 "$HERE/select_tasks.py" "$WORKDIR/candidates.json" "$EXPERIMENT_TOML" "$WORKDIR/tasks.jsonl"
fi

if [ -n "$TASKS_LIMIT" ]; then
    head -n "$TASKS_LIMIT" "$WORKDIR/tasks.jsonl" > "$WORKDIR/tasks.jsonl.tmp"
    mv "$WORKDIR/tasks.jsonl.tmp" "$WORKDIR/tasks.jsonl"
fi
N_TASKS=$(wc -l < "$WORKDIR/tasks.jsonl" | tr -d ' ')
echo "   tasks = $N_TASKS"
[ "$N_TASKS" -gt 0 ] || { echo "run.sh: zero tasks generated" >&2; exit 1; }

mkdir -p "$WORKDIR/tasks"
python3 - "$WORKDIR/tasks.jsonl" "$WORKDIR/tasks" <<'PY'
import json, os, sys
tasks_jsonl, out_dir = sys.argv[1:3]
with open(tasks_jsonl) as f:
    for line in f:
        t = json.loads(line)
        with open(os.path.join(out_dir, t["id"] + ".json"), "w") as g:
            json.dump(t, g)
PY

# ---- 2. per-task setup (C2: mutate one shared worktree; CTF: record the
#         manifest-sliced expected set — the per-cell fixture copy happens in
#         run_one_job so each cell can carry its own SKILL.md) -----------------
if [ "$TASK_CLASS" = "CTF" ]; then
    echo "-- expected (CTF, no mutation) --"
    for task_file in "$WORKDIR"/tasks/*.json; do
        task_id="$(basename "$task_file" .json)"
        echo "$task_id" >> "$WORKDIR/task_ids.txt"
        python3 - "$task_file" "$RAW_DIR/$task_id.expected.json" <<'PY'
import json, sys
task = json.load(open(sys.argv[1]))
json.dump({"class": task["class"], "concept": task.get("concept"),
           "abstraction": task.get("abstraction"),
           "expected_json": task["expected_json"]}, open(sys.argv[2], "w"))
PY
    done
else
    echo "-- mutate --"
    for task_file in "$WORKDIR"/tasks/*.json; do
        task_id="$(basename "$task_file" .json)"
        echo "$task_id" >> "$WORKDIR/task_ids.txt"
        mutated_task_json="$("$HERE/mutate.sh" apply "$task_file" "$REPO_ROOT" "$WORKTREES_DIR")"
        python3 - "$mutated_task_json" "$RAW_DIR/$task_id.expected.json" <<'PY'
import json, sys
task = json.load(open(sys.argv[1]))
json.dump({"class": task["class"], "mutation_kind": task["mutation_kind"],
           "expected_json": task["expected_json"]}, open(sys.argv[2], "w"))
PY
    done
fi

# ---- 3. cells ----------------------------------------------------------------
mapfile -t CELL_NAMES < <(jq -r '.cells[].name' "$CONFIG_JSON_FILE")
if [ -n "$CELLS_FILTER" ]; then
    IFS=',' read -ra WANT <<< "$CELLS_FILTER"
    FILTERED=()
    for c in "${CELL_NAMES[@]}"; do
        for w in "${WANT[@]}"; do [ "$c" = "$w" ] && FILTERED+=("$c"); done
    done
    CELL_NAMES=("${FILTERED[@]}")
fi
echo "   cells = ${CELL_NAMES[*]}"
[ "${#CELL_NAMES[@]}" -gt 0 ] || { echo "run.sh: zero cells selected" >&2; exit 1; }

run_one_job() {
    local task_id="$1" cell_name="$2" rep="$3"
    local task_file="$WORKDIR/tasks/$task_id.json"
    local out_base="$RAW_DIR/${task_id}__${cell_name}__rep${rep}"

    if [ "$DRY_RUN" = "1" ]; then
        if python3 "$HERE/fixtures/stub_cell.py" "$RAW_DIR" "$task_id" "$cell_name" "$rep" \
                > "$out_base.json" 2>"$out_base.stub.log"; then
            rm -f "$out_base.stub.log"
        else
            mv "$out_base.stub.log" "$out_base.error"
            rm -f "$out_base.json"
        fi
        return 0
    fi

    local model claude_model allowed_tools mcp_flag prompt worktree_root
    model="$(jq -r --arg n "$cell_name" '.cells[] | select(.name==$n) | .model' "$CONFIG_JSON_FILE")"
    claude_model="$(jq -r --arg m "$model" '.models[] | select(.id==$m) | .claude_model' "$CONFIG_JSON_FILE")"
    allowed_tools="$(jq -r --arg n "$cell_name" '.cells[] | select(.name==$n) | .allowed_tools' "$CONFIG_JSON_FILE")"
    mcp_flag="$(jq -r --arg n "$cell_name" '.cells[] | select(.name==$n) | .mcp' "$CONFIG_JSON_FILE")"
    prompt="$(jq -r '.prompt' "$task_file")"
    if [ "$TASK_CLASS" = "CTF" ]; then
        # Each (task, cell, rep) gets its own fresh fixture copy so the per-cell
        # SKILL.md at the root differs by cell and a model write can't leak
        # across cells/reps. No git (dl roots at cwd when no .git is found).
        worktree_root="$WORKTREES_DIR/$task_id/$cell_name/rep${rep}"
        rm -rf "$worktree_root"
        mkdir -p "$worktree_root"
        cp -R "$FIXTURE_DIR/." "$worktree_root/"
        # NEVER ship the answer key or the skill sources into the tree the model
        # sees — MANIFEST.json is the ground truth; skills/ holds both cells'
        # SKILL.md sources (the cell's own is placed at the root below).
        rm -rf "$worktree_root/skills" "$worktree_root/MANIFEST.json"
        local skill_file
        skill_file="$(jq -r --arg n "$cell_name" '.cells[] | select(.name==$n) | .skill_file // ""' "$CONFIG_JSON_FILE")"
        if [ -n "$skill_file" ]; then
            cp "$HERE/$skill_file" "$worktree_root/SKILL.md"
        fi
    else
        worktree_root="$WORKTREES_DIR/$task_id"
    fi

    # `--allowedTools` only pre-approves (skips the permission prompt); it does
    # NOT restrict which tools EXIST in the session — Edit/Write/Agent/etc. stay
    # available regardless (verified empirically: a haiku cell asked to use Edit
    # with only "Bash,Read,Grep,Glob,mcp__dl__dl.rows" in --allowedTools used it
    # without complaint). `--tools` is the actual availability allowlist ("the
    # built-in set" only — MCP tools are separately gated by which servers
    # --mcp-config attaches, so they survive `--tools` untouched). Both flags
    # are required for a genuinely symmetric, non-mutating cell (protocol #2)
    # — a cell with a stray Edit/Write could tamper with the mutated worktree
    # across reps. builtin_tools strips any `mcp__...` entries out of
    # allowed_tools for the `--tools` list.
    local builtin_tools
    builtin_tools="$(echo "$allowed_tools" | tr ',' '\n' | grep -v '^mcp__' | paste -sd, -)"

    local mcp_config_args=()
    if [ "$mcp_flag" = "true" ]; then
        mkdir -p "$WORKDIR/dbs" "$WORKDIR/mcp-configs"
        local mcp_config_path="$WORKDIR/mcp-configs/${task_id}.${cell_name}.rep${rep}.json"
        local scratch_db="$WORKDIR/dbs/${task_id}.${cell_name}.rep${rep}.db"
        python3 - "$DL_BIN" "$MCP_PROG" "$scratch_db" "$mcp_config_path" <<'PY'
import json, sys
dl_bin, mcp_prog, db, out = sys.argv[1:5]
cfg = {"mcpServers": {"dl": {"command": dl_bin,
                              "args": [mcp_prog, "--mcp", "--no-daemon", "--db", db]}}}
json.dump(cfg, open(out, "w"))
PY
        mcp_config_args=(--mcp-config "$mcp_config_path" --strict-mcp-config)
    fi

    local runner=()
    [ -n "$TIMEOUT_BIN" ] && runner=("$TIMEOUT_BIN" "${TIMEOUT_SECS}s")

    set +e
    ( cd "$worktree_root" && "${runner[@]}" claude -p "$prompt" \
        --model "$claude_model" \
        --output-format json \
        --tools "$builtin_tools" \
        --allowedTools "$allowed_tools" \
        --permission-mode bypassPermissions \
        "${mcp_config_args[@]}" \
        > "$out_base.json" 2>"$out_base.stderr.log" )
    local status=$?
    set -e
    if [ "$status" -ne 0 ]; then
        echo "exit $status" >> "$out_base.stderr.log"
        mv "$out_base.stderr.log" "$out_base.error"
        rm -f "$out_base.json"
    else
        rm -f "$out_base.stderr.log"
    fi
    return 0
}

echo "-- run cells (parallelism=$PARALLELISM, reps=$REPS) --"
JOB_COUNT=0
for cell_name in "${CELL_NAMES[@]}"; do
    for task_file in "$WORKDIR"/tasks/*.json; do
        task_id="$(basename "$task_file" .json)"
        for rep in $(seq 1 "$REPS"); do
            run_one_job "$task_id" "$cell_name" "$rep" &
            JOB_COUNT=$((JOB_COUNT + 1))
            if [ "$JOB_COUNT" -ge "$PARALLELISM" ]; then
                wait -n || true
                JOB_COUNT=$((JOB_COUNT - 1))
            fi
        done
    done
done
wait || true

# ---- 4. score ----------------------------------------------------------------
echo "-- score --"
python3 "$HERE/score.py" "$RAW_DIR" "$EXPERIMENT_TOML" "$OUT_PREFIX"

echo "== done =="
echo "results: $OUT_PREFIX.md"
echo "         $OUT_PREFIX.jsonl"
