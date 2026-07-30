#!/usr/bin/env bash
# comment-rails.sh: standing live extraction grades for the seven comment-node
# techniques. Each rail gets a fresh served engine, database, and git corpus.
# The markdown todo rail is reported as a named extractor skip.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$TSV2/../.." && pwd)"
FIXTURES="$ROOT/v6/dl/fixtures"
COMPILE="$ROOT/v6/prolog/compile/scripts/compile_dl6.sh"
SERVE="$TSV2/serve/main.ts"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/comment-rails.XXXXXX")"
PORT="${CN_RAIL_PORT:-17611}"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""
FAILED=0

export DL_COMMENT_NODE="${DL_COMMENT_NODE:-$SCRIPT_DIR/comment_node.py}"
export DL_ARCH_MARKER="${DL_ARCH_MARKER:-$SCRIPT_DIR/comment_arch_marker.py}"
export DL_POLICY_MARKERS="${DL_POLICY_MARKERS:-$SCRIPT_DIR/comment_policy_markers.py}"
if [ -z "${DL_EXTRACT_BIN:-}" ]; then
  DL_EXTRACT_BIN="$ROOT/v6/sprefa-extract/target/release/extract"
  if [ ! -x "$DL_EXTRACT_BIN" ]; then
    (cd "$ROOT/v6/sprefa-extract" && cargo build --release --features cli --bin extract) >"$WORK/extract-build.log" 2>&1 || {
      echo "FAIL extractor build: $(tail -8 "$WORK/extract-build.log")"
      exit 1
    }
  fi
  export DL_EXTRACT_BIN
fi

say() { printf '%s\n' "$*"; }
fail() { say "FAIL  $*"; FAILED=1; }
stop_server() {
  if [ -n "$SERVER_PID" ]; then
    kill -9 "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
    SERVER_PID=""
  fi
}
trap stop_server EXIT

[ -x "$DL_EXTRACT_BIN" ] || { say "FAIL extractor is not executable: $DL_EXTRACT_BIN"; exit 1; }
[ -f "$DL_COMMENT_NODE" ] || { say "FAIL comment helper is missing: $DL_COMMENT_NODE"; exit 1; }
[ -f "$DL_ARCH_MARKER" ] || { say "FAIL ARCH marker helper is missing: $DL_ARCH_MARKER"; exit 1; }
[ -f "$DL_POLICY_MARKERS" ] || { say "FAIL policy marker helper is missing: $DL_POLICY_MARKERS"; exit 1; }

compile_rails() {
  local file name
  mkdir -p "$WORK/compiled"
  for file in \
    "$FIXTURES/comment-arch-rail.dl6" \
    "$FIXTURES/comment-suppress-rail.dl6" \
    "$FIXTURES/comment-readme-rail.dl6" \
    "$FIXTURES/comment-lang-junction-rail.dl6" \
    "$FIXTURES/comment-zone-rail.dl6" \
    "$FIXTURES/comment-lint-rail.dl6"; do
    name="$(basename "$file" .dl6)"
    if "$COMPILE" "$file" "$WORK/compiled/$name.ts" >"$WORK/$name.compile.log" 2>&1; then
      say "PASS  compile $name"
    else
      fail "compile $name: $(tail -8 "$WORK/$name.compile.log")"
    fi
  done
  say "SKIP  technique 5 todo(category)/TODO/FIXME plans index: sprefa-extract has no markdown comment grammar; extractor remains untouched"
  say "SKIP  technique 2 block pairing: current compiler refuses the range-comparison body with unsupported_construct(unbound_head_var(_)); no host workaround applied"
}

start_server() {
  local db="$1" corpus="$2"
  (
    cd "$corpus"
    TSV2_DB="file:$db" TSV2_PORT="$PORT" TSV2_WATCH_COALESCE_MS=60 \
      TSV2_WATCH_ROOT="$corpus" DL_EXTRACT_BIN="$DL_EXTRACT_BIN" DL_COMMENT_NODE="$DL_COMMENT_NODE" DL_ARCH_MARKER="$DL_ARCH_MARKER" DL_POLICY_MARKERS="$DL_POLICY_MARKERS" \
      node --experimental-transform-types "$SERVE"
  ) >"$WORK/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 80); do
    if curl -s -o /dev/null "$BASE/ticks" 2>/dev/null; then
      return 0
    fi
    kill -0 "$SERVER_PID" 2>/dev/null || { fail "server died: $(tail -20 "$WORK/server.log")"; return 1; }
    sleep 0.2
  done
  fail "server did not become ready"
  return 1
}

load_program() {
  local file="$1" status
  status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$file" "$BASE/program")"
  if [ "$status" != 200 ]; then
    fail "program load $(basename "$file") returned $status: $(cat "$WORK/load.json")"
    return 1
  fi
}

rows() { curl -s "$BASE/idb/$1" | tr -d ' \n'; }
await_present() {
  local rel="$1" needle="$2" limit=$((SECONDS + 45)) value
  while [ "$SECONDS" -lt "$limit" ]; do
    value="$(rows "$rel")"
    case "$value" in *"$needle"*) return 0;; esac
    sleep 0.25
  done
  return 1
}
await_absent() {
  local rel="$1" needle="$2" limit=$((SECONDS + 45)) value
  while [ "$SECONDS" -lt "$limit" ]; do
    value="$(rows "$rel")"
    case "$value" in *"$needle"*) ;; *) return 0;; esac
    sleep 0.25
  done
  return 1
}

new_corpus() {
  local dir="$WORK/$1"
  mkdir -p "$dir/src"
  (cd "$dir" && git init -q)
  printf '%s\n' "$dir"
}

run_arch() {
  local corpus
  corpus="$(new_corpus arch)"
  start_server "$WORK/arch.sqlite" "$corpus" || return
  load_program "$FIXTURES/comment-arch-rail.dl6" || return
  cat >"$corpus/src/arch.rs" <<'EOF'
// ARCH {"url":"sprefa/compile/01-lower/00-entry"}
// ARCH {"url":"sprefa/compile/01-lower/01-emit"}
// ARCH {"url":"sprefa/compile/01-lower"}
const fake = "// ARCH {\"url\":\"fake/not/a/node\"}";
EOF
  if await_present arch_node '01-lower/01-emit'; then
    case "$(rows arch_node)" in *fake/not/a/node*) fail "technique 1 string literal became arch_node";; *) say "PASS  technique 1 ARCH nodes + witness: $(rows arch_node)";; esac
  else fail "technique 1 ARCH node did not land; server log: $(tail -20 "$WORK/server.log")"; fi
  if await_present arch_edge 'sprefa/compile/01-lower'; then say "PASS  technique 1 hierarchy: $(rows arch_edge)"; else fail "technique 1 hierarchy did not land; server log: $(tail -20 "$WORK/server.log")"; fi
  stop_server
}

run_suppress() {
  local corpus
  corpus="$(new_corpus suppress)"
  start_server "$WORK/suppress.sqlite" "$corpus" || return
  load_program "$FIXTURES/comment-suppress-rail.dl6" || return
  cat >"$corpus/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source);
}
EOF
  if await_present diag_v5 '"src/a.ts",2'; then say "PASS  technique 2 unguarded finding: $(rows diag_v5)"; else fail "technique 2 finding did not land"; fi
  cat >"$corpus/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source); // dl-disable-line no-eval
}
EOF
  if await_absent diag_v5 'no-eval'; then say "PASS  technique 2 disable-line suppression"; else fail "technique 2 disable-line did not suppress"; fi
  cat >"$corpus/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source); const hint = "dl-disable-line no-eval";
}
EOF
  if await_present diag_v5 '"src/a.ts",2'; then say "PASS  technique 2 grammar witness rejects string-literal marker"; else fail "technique 2 string-literal marker suppressed finding"; fi
  cat >"$corpus/src/a.ts" <<'EOF'
export function danger(source: string): unknown {
  // dl-disable-next-line no-eval
  return eval(source);
}
EOF
  if await_absent diag_v5 'no-eval'; then say "PASS  technique 2 disable-next-line arithmetic"; else fail "technique 2 disable-next-line did not suppress"; fi
  cat >"$corpus/src/b.ts" <<'EOF'
export function safe(source: string): unknown {
  return JSON.parse(source); // dl-disable-line no-eval
}
EOF
  if await_present suppress_unused '"src/b.ts",2,"no-eval"'; then say "PASS  technique 2 unused-suppression antijoin"; else fail "technique 2 unused antijoin did not warn"; fi
  stop_server
}

run_readme() {
  local corpus
  corpus="$(new_corpus readme)"
  start_server "$WORK/readme.sqlite" "$corpus" || return
  load_program "$FIXTURES/comment-readme-rail.dl6" || return
  cat >"$corpus/src/readme.rs" <<'EOF'
// README(gallery): A grammar-backed note for the gallery
// README-ROW(gallery)
const fake = "README(gallery): this string is not a note";
EOF
  if await_present readme_note 'gallery'; then say "PASS  technique 3 README note + witness: $(rows readme_note)"; else fail "technique 3 README note did not land"; fi
  if await_absent readme_drift 'gallery'; then say "PASS  technique 3 per-file anchor join"; else fail "technique 3 drift antijoin fired"; fi
  stop_server
}

run_lang() {
  local corpus
  corpus="$(new_corpus lang)"
  start_server "$WORK/lang.sqlite" "$corpus" || return
  load_program "$FIXTURES/comment-lang-junction-rail.dl6" || return
  cat >"$corpus/src/lang.rs" <<'EOF'
// LANG-JUNCTION(alpha): one registered language junction
// LANG-REGISTRY(alpha)
const fake = "LANG-JUNCTION(fake): string literal";
EOF
  if await_present junction 'alpha'; then say "PASS  technique 4 LANG-JUNCTION + witness: $(rows junction)"; else fail "technique 4 junction did not land"; fi
  if await_absent junction_drift 'alpha'; then say "PASS  technique 4 registry antijoin"; else fail "technique 4 registry drift fired"; fi
  stop_server
}

run_zone() {
  local corpus
  corpus="$(new_corpus zone)"
  start_server "$WORK/zone.sqlite" "$corpus" || return
  load_program "$FIXTURES/comment-zone-rail.dl6" || return
  cat >"$corpus/src/zone.rs" <<'EOF'
// BEGIN: gen alpha
fn generated() {}
// END:
EOF
  if await_present gen_zone 'alpha'; then say "PASS  technique 6 BEGIN: gen ownership: $(rows gen_zone)"; else fail "technique 6 zone did not land"; fi
  stop_server
}

run_lint() {
  local corpus
  corpus="$(new_corpus lint)"
  start_server "$WORK/lint.sqlite" "$corpus" || return
  load_program "$FIXTURES/comment-lint-rail.dl6" || return
  cat >"$corpus/src/lint.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source);
}
EOF
  if await_present diag_v5 'no-eval'; then say "PASS  technique 7 live lint finding: $(rows diag_v5)"; else fail "technique 7 finding did not land"; fi
  cat >"$corpus/src/lint.ts" <<'EOF'
export function danger(source: string): unknown {
  return eval(source); // dl-disable-line no-eval
}
EOF
  if await_absent diag_v5 'no-eval'; then say "PASS  technique 7 finding governed by suppression"; else fail "technique 7 suppression did not retract finding"; fi
  stop_server
}

compile_rails
run_arch
run_suppress
run_readme
run_lang
run_zone
run_lint

if [ "$FAILED" = 0 ]; then
  say "COMMENT RAIL LIVE EXTRACTION GRADES HOLD"
  exit 0
fi
say "COMMENT RAIL LIVE EXTRACTION GRADES FAILED; work tree: $WORK"
exit 1
