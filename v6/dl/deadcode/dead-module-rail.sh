#!/usr/bin/env bash
# dead-module-rail.sh -- compile the rail through the rust door and print its
# findings. Argument 1 is the tree to read, argument 2 the glob to seed.
# Argument 3 is a comma-separated root list; with none the crawl reaches nothing
# and every file reads as unreachable.
#   bash v6/dl/deadcode/dead-module-rail.sh ~/projects/hafley-rs 'crates/*/src/*.rs' 'crates/boop/src/main.rs'
# In a git pathspec `*` crosses `/` but `**` demands a directory level, so
# `crates/*/src/*.rs` matches 82 files where `crates/*/src/**/*.rs` matches 17.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
TARGET="${1:-$ROOT}"
GLOB="${2:-crates/boop-acp/src/*.rs}"
ROOTS="${3:-}"
# With no roots the crawl reaches nothing and every file reads as unreachable,
# so derive them rather than report a vacuous sheet. Every bin and lib target
# is an entry point by definition; hand-typing one crate's main.rs made every
# OTHER crate's entry read as dead.
if [ -z "$ROOTS" ] && [ -f "$TARGET/Cargo.toml" ] && command -v cargo >/dev/null; then
  ROOTS="$( (cd "$TARGET" && cargo metadata --format-version 1 --no-deps 2>/dev/null) | python3 -c '
import json,os,sys
try: document = json.load(sys.stdin)
except Exception: sys.exit(0)
root = document["workspace_root"]
kinds = {"bin", "lib", "proc-macro", "cdylib", "rlib"}
seen = []
for package in document["packages"]:
    for target in package["targets"]:
        if set(target["kind"]) & kinds:
            path = os.path.relpath(target["src_path"], root)
            if path not in seen:
                seen.append(path)
print(",".join(sorted(seen)))
' )"
  [ -n "$ROOTS" ] && printf 'roots from cargo metadata: %s\n' "$(printf '%s' "$ROOTS" | tr ',' ' ' | wc -w | tr -d ' ')" >&2
fi
if [ -z "$ROOTS" ]; then
  printf 'WARN no roots: the crawl reaches nothing and rail_unreachable_module names every file\n' >&2
fi
WORK="$(mktemp -d "${TMPDIR:-/tmp}/dead-module.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
  -g "compile_dl6('$HERE/dead-module-rail.dl6','$WORK/rail.rs',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 || fail "compile: $(tail -20 "$WORK/compile.log")"

cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
  >"$WORK/build.log" 2>&1 || fail "cargo build: $(tail -5 "$WORK/build.log")"

python3 -c "
import json,sys
seed=[{'rel':'want','sign':'add','row':[sys.argv[2]]}]
seed += [{'rel':'root_file','sign':'add','row':[r]} for r in sys.argv[3].split(',') if r]
json.dump([seed], open(sys.argv[1],'w'))
" "$WORK/schedule.json" "$GLOB" "$ROOTS"

( cd "$TARGET" && DL_ADAPTERS_DIR="$HERE" \
    DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
    "$ENGINE/target/release/emit_rust_harness" "$WORK/rail.rs" "$WORK/schedule.json" --live-hosts ) \
  >"$WORK/ticks.jsonl" 2>"$WORK/err" || fail "run: $(tail -20 "$WORK/err")"

python3 - "$WORK/ticks.jsonl" <<'PYTHON'
import json,sys
rows={}
for line in open(sys.argv[1]):
    if not line.strip(): continue
    for rel,delta in json.loads(line)["deltas"].items():
        if rel not in ("rail_dead_module","rail_unreachable_module","module_reach","rail_root_not_a_source","rail_unproven_module"): continue
        live=rows.setdefault(rel,{})
        for r in delta.get("add",[]): live[json.dumps(r,sort_keys=True)]=r
        for r in delta.get("del",[]): live.pop(json.dumps(r,sort_keys=True),None)
bad=sorted(rows.get("rail_root_not_a_source",{}).values())
for r in bad: print(f"WARN root not in the glob: {r[0]}")
dead=sorted(rows.get("rail_dead_module",{}).values(), key=lambda r:-int(r[1]))
unre=sorted(rows.get("rail_unreachable_module",{}).values(), key=lambda r:-int(r[1]))
reach=sorted(rows.get("module_reach",{}).values(), key=lambda r:-int(r[1]))
print("== rail_dead_module (defs>=5, zero called from another file) ==")
for r in dead: print(f"  {r[1]:>4} defs  {r[0]}")
print("== rail_unreachable_module (the crawl, from declared roots) ==")
for r in unre: print(f"  {r[1]:>4} defs  {r[0]}")
unpr=sorted(rows.get("rail_unproven_module",{}).values(), key=lambda r:-int(r[1]))
print("== rail_unproven_module (reached only through ambiguous names) ==")
for r in unpr: print(f"  {r[1]:>4} defs  {r[0]}")
print("== module_reach (defs / used-across) ==")
for r in reach: print(f"  {r[1]:>4} / {r[2]:<4} amb={r[3]:<4} {r[0]}")
print(f"findings={len(dead)} unproven={len(unpr)} unreachable={len(unre)}")
PYTHON
