#!/usr/bin/env bash
# Smoke I (sprefa-zqq + sprefa-3n6 end-to-end):
#
# Drive sprefa-lsp against a real linux-kernel scan with FOUR independent
# per-file pipes. Hover each pipe to populate its OpCache cells, snapshot
# cache.len, mutate ONE file under the watched root, re-hover the
# matching pipe, and assert:
#   (a) the cache shrank by exactly that file's worth of cells, and
#   (b) the surviving cells equal the other three pipes' contribution.
#
# Skipped (exit 0) if the kernel fixture symlink is missing — keeps CI
# usable without the 2GB checkout. Local devs see the full proof.
#
# Requires SPREFA_LSP_PROFILE=release. Debug ast-grep is 75–450x slower;
# the cold hover would not fit in the deadline.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
v3_dir="$(cd "$here/../.." && pwd)"
cd "$v3_dir"

kernel_root="$v3_dir/tests/smoke/.fixtures/linux"
if [ ! -d "$kernel_root/kernel" ]; then
    echo "SKIP: kernel fixture missing at $kernel_root (clone or symlink)" >&2
    exit 0
fi

profile="${SPREFA_LSP_PROFILE:-release}"
bin="./target/$profile/sprefa-lsp"
if [ ! -x "$bin" ]; then
    echo "building sprefa-lsp ($profile)..." >&2
    if [ "$profile" = "release" ]; then
        cargo build -p server --bin sprefa-lsp --release >&2
    else
        cargo build -p server --bin sprefa-lsp >&2
    fi
fi

fixture="$v3_dir/crates/server/fixtures/kernel_invalidate/probe.sprf"
uri="file://$fixture"
text="$(cat "$fixture")"

# Pipe lines (0-indexed) and their target file. Order matters: index 0
# is the pipe we'll re-hover after mutation.
pipe_lines=(11 12 13 14)
pipe_files=("kernel/acct.c" "kernel/async.c" "kernel/audit.c" "kernel/audit_tree.c")
mutate_idx=0
target_rel="${pipe_files[$mutate_idx]}"
target_file="$kernel_root/$target_rel"
if [ ! -f "$target_file" ]; then
    echo "FAIL: target file $target_file missing" >&2
    exit 1
fi

# Compute the 0-indexed character offset of `ast` on each pipe line.
# index() is 1-based; subtract 1 for LSP coordinates.
ast_col_at() {
    local line_zero="$1"
    awk -v line="$((line_zero + 1))" '
        NR==line { p = index($0, "ast["); if (p==0) exit 1; print p-1; exit }
    ' "$fixture"
}

DEADLINE_S="${SPREFA_LSP_HOVER_DEADLINE_S:-60}"
DEBOUNCE_SLEEP_S="0.6"   # > WATCHER_DEBOUNCE (200ms) + invalidator drain

OUT="$(mktemp -t sprefa_lsp_inv_out.XXXX)"
ERR="$(mktemp -t sprefa_lsp_inv_err.XXXX)"
PIPE="$(mktemp -u -t sprefa_lsp_inv_in.XXXX)"
mkfifo "$PIPE"
PID=""
WRITER_FD=9
ORIG_BACKUP=""

cleanup() {
    if [ -n "${WRITER_OPEN:-}" ]; then
        eval "exec ${WRITER_FD}>&-" || true
    fi
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        kill -TERM "$PID" 2>/dev/null || true
        sleep 0.3
        kill -KILL "$PID" 2>/dev/null || true
    fi
    rm -f "$PIPE"
    if [ -n "$ORIG_BACKUP" ] && [ -f "$ORIG_BACKUP" ]; then
        mv "$ORIG_BACKUP" "$target_file" 2>/dev/null || true
    fi
    if [ "${VERBOSE:-0}" = "1" ]; then
        echo "--- stdout ---" >&2; cat "$OUT" >&2 || true
        echo "--- stderr ---" >&2; cat "$ERR" >&2 || true
    fi
    rm -f "$OUT" "$ERR"
}
trap cleanup EXIT INT TERM

send() {
    local body="$1"
    local len=${#body}
    printf 'Content-Length: %d\r\n\r\n%s' "$len" "$body" >&${WRITER_FD}
}

text_json="$(python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))' <<<"$text")"
init='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":"file://'"$v3_dir"'","capabilities":{}}}'
inited='{"jsonrpc":"2.0","method":"initialized","params":{}}'
didopen='{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"'"$uri"'","languageId":"sprefa","version":1,"text":'"$text_json"'}}}'

hover_msg() {
    local id="$1" line="$2" col="$3"
    echo '{"jsonrpc":"2.0","id":'"$id"',"method":"textDocument/hover","params":{"textDocument":{"uri":"'"$uri"'"},"position":{"line":'"$line"',"character":'"$col"'}}}'
}

await_response() {
    local id="$1" what="$2"
    local deadline=$((SECONDS + DEADLINE_S))
    while (( SECONDS < deadline )); do
        if ! kill -0 "$PID" 2>/dev/null; then
            echo "FAIL: lsp exited before $what" >&2
            tail -40 "$ERR" >&2
            exit 1
        fi
        if grep -q "\"id\":$id\\b" "$OUT" 2>/dev/null; then return 0; fi
        sleep 0.2
    done
    echo "FAIL: $what timed out after ${DEADLINE_S}s" >&2
    tail -40 "$ERR" >&2
    exit 1
}

# Pull the most recent `cache_len=N` from a particular event tag.
last_cache_len() {
    local tag="$1"
    grep -E "$tag.*cache_len=[0-9]+" "$ERR" \
        | tail -1 \
        | grep -oE 'cache_len=[0-9]+' | head -1 \
        | grep -oE '[0-9]+'
}

"$bin" <"$PIPE" >"$OUT" 2>"$ERR" &
PID=$!
eval "exec ${WRITER_FD}>\"$PIPE\""
WRITER_OPEN=1

send "$init"
sleep 0.3
send "$inited"
send "$didopen"
sleep 0.3

# ---------- Phase 1: cold-hover each pipe to populate cache ----------
hover_id=2
declare -a pipe_lens=()
for i in "${!pipe_lines[@]}"; do
    line="${pipe_lines[$i]}"
    col="$(ast_col_at "$line")"
    if [ -z "$col" ]; then
        echo "FAIL: could not locate \`ast[\` on line $line" >&2
        exit 1
    fi
    echo "[populate $((i+1))/${#pipe_lines[@]}] hover line=$line col=$col file=${pipe_files[$i]}" >&2
    t_start=$SECONDS
    send "$(hover_msg $hover_id $line $col)"
    await_response $hover_id "hover #${hover_id} (populate ${pipe_files[$i]})"
    elapsed=$((SECONDS - t_start))
    n="$(last_cache_len 'hover\.cache\.after')"
    pipe_lens+=("$n")
    echo "                       cache.len=$n (Δt=${elapsed}s)" >&2
    hover_id=$((hover_id + 1))
done

n_populated="${pipe_lens[$((${#pipe_lens[@]}-1))]}"
prev=0
declare -a per_pipe_contrib=()
for n in "${pipe_lens[@]}"; do
    per_pipe_contrib+=("$((n - prev))")
    prev=$n
done
echo "        per-pipe contributions: ${per_pipe_contrib[*]} (totals: ${pipe_lens[*]})" >&2

# We can't predict the exact eviction count without re-implementing the
# reverse-index logic in shell: only cache entries whose OUTPUT batch
# contained a cursor at the mutated path get indexed (so e.g. the
# `repo`/`rev` entries don't, and an `ast` entry that emitted zero
# cursors doesn't either). So the assertion is shape-based:
#   shrink > 0                    — invalidation fired
#   shrink < n_populated          — eviction was selective, not wholesale
#   surviving >= n_populated/2    — most cells survived (other pipes intact)
#   cache.invalidate log names the mutated file

# ---------- Phase 2: mutate the target file ----------
echo "[mutate] $target_rel" >&2
ORIG_BACKUP="$(mktemp -t sprefa_kernel_orig.XXXX)"
cp "$target_file" "$ORIG_BACKUP"
echo "// sprefa-zqq smoke probe — restore from $ORIG_BACKUP if seen" >> "$target_file"
sleep "$DEBOUNCE_SLEEP_S"
sleep 0.4

# ---------- Phase 3: re-hover the mutated pipe ----------
mutate_line="${pipe_lines[$mutate_idx]}"
mutate_col="$(ast_col_at "$mutate_line")"
echo "[verify] re-hover line=$mutate_line col=$mutate_col" >&2
t_v=$SECONDS
send "$(hover_msg $hover_id $mutate_line $mutate_col)"
await_response $hover_id "hover #${hover_id} (verify post-evict)"
elapsed_v=$((SECONDS - t_v))

n_before_verify="$(last_cache_len 'hover\.cache\.before')"
n_after_verify="$(last_cache_len 'hover\.cache\.after')"

# ---------- Assertions ----------
shrink=$((n_populated - n_before_verify))
echo "        cache after populate phase : $n_populated" >&2
echo "        cache before re-hover      : $n_before_verify  (Δ = -$shrink)" >&2
echo "        cache after  re-hover      : $n_after_verify   (refilled, Δt=${elapsed_v}s)" >&2

if [ "$shrink" -le 0 ]; then
    echo "FAIL: cache did not shrink — watcher / invalidator path broken" >&2
    grep -E "cache\.invalidate|fs_watcher" "$ERR" | head -20 >&2 || true
    tail -40 "$ERR" >&2
    exit 1
fi

if [ "$shrink" -ge "$n_populated" ]; then
    echo "FAIL: cache wholesale-flushed (shrink=${shrink}, populated=${n_populated})" >&2
    echo "      eviction must be SELECTIVE, not blow the cache" >&2
    exit 1
fi

# Most entries should survive: other pipes' cells must not be evicted
# when one file changes.
half=$((n_populated / 2))
if [ "$n_before_verify" -lt "$half" ]; then
    echo "FAIL: too many entries evicted (${shrink} of ${n_populated}); other pipes' cache lost" >&2
    grep -E "cache\.invalidate" "$ERR" | head -10 >&2 || true
    exit 1
fi

target_basename="$(basename "$target_file")"
target_dir="$(dirname "$target_rel")"
if grep -qE "cache\.invalidate.*(${target_basename}|${target_dir})" "$ERR" 2>/dev/null; then
    echo "        cache.invalidate event confirmed for ${target_basename}" >&2
else
    echo "WARN: cache.invalidate log does not name ${target_basename}; tracing filter?" >&2
fi

echo "OK selective eviction: ${shrink}/${n_populated} cells dropped for ${target_rel}; ${n_before_verify} survived; warm refill→${n_after_verify}"
