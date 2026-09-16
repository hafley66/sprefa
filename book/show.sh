#!/usr/bin/env bash
# show.sh compile|eval|run|emit <file.dl7 or program.json> [dl8 flags], run from the repository root; DL8 overrides the binary, paths print relative to it.
set -uo pipefail

verb=${1:?compile, eval, run or emit}
source=${2:?a .dl7 file}
shift 2

here=$(cd "$(dirname "$0")/.." && pwd)
dl8=${DL8:-${CARGO_TARGET_DIR:-$here/target}/release/dl8}
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

render='
def short: sub("^\($ENV.PWD)/"; "");
def names: ($compiled[0].program.names // {}) | to_entries
  | map({key: (.value | tojson), value: .key}) | from_entries;
def cell($n):
  if type == "string" then short
  elif type == "array" then "[" + (map(cell($n)) | join(" ")) + "]"
  elif type != "object" then tostring
  elif has("s") then (.s | short | tojson)
  elif has("a") then (.a | short)
  elif .f == "const" then (.args[0] | cell($n))
  elif .f == "ref" and ($n[tojson] != null) then $n[tojson]
  elif .f == "owner" and ($n[{args: [.], f: "ref"} | tojson] != null) then $n[{args: [.], f: "ref"} | tojson]
  elif .f == "ref" and (.args[0] | type == "object" and .f == "kernel") then .args[0].args[0].a
  elif .f == "ref" and (.args[0] | type == "object" and has("a")) then .args[0].a
  else .f + "(" + (.args | map(cell($n)) | join(", ")) + ")" end;
def own: .rel.f == "ref" and (.rel.args[0] | type == "object")
  and ((.rel.args[0].f == "owner" and .rel.args[0].args[0].f == "file")
    or (.rel.args[0].f == "kernel" and .rel.args[0].args[0].a == "effect")
    or (.rel.args[0] | has("a")));
def rows: if has("compiler_rows")
  then [.compiler_rows[] | select(.f == "call") | {rel: .args[0], args: .args[1]}]
  else (.closure // []) end;
names as $n
| (rows[] | select(own) | "(" + ([(.rel | cell($n))] + (.args | map(cell($n))) | join(" ")) + ")"),
  ((.views // [])[] | "view \(.relation) stratum \(.stratum)"),
  ((.diagnostics // [])[] | "diagnostic " + (if has("payload") then .payload else . end | cell($n))),
  (if has("ticks") then "ticks \(.ticks)" else empty end)
'

compile_flags=()
[ "$verb" = compile ] && compile_flags=("$@")
if [[ "$source" == *.json ]]; then
  cp "$source" "$scratch/compiled.json"
  code=0
else
  "$dl8" compile "$source" "${compile_flags[@]}" > "$scratch/compiled.json" 2> "$scratch/compile.err"
  code=$?
fi
if [ "$verb" = compile ] || [ "$code" -ne 0 ]; then
  jq -r --slurpfile compiled "$scratch/compiled.json" "$render" "$scratch/compiled.json"
  echo "exit $code"
  exit 0
fi

case "$verb" in
  emit) "$dl8" emit "$@" "$scratch/compiled.json" > "$scratch/out.json" 2> "$scratch/out.err" ;;
  *) "$dl8" "$verb" "$scratch/compiled.json" "$@" > "$scratch/out.json" 2> "$scratch/out.err" ;;
esac
code=$?
jq -r --slurpfile compiled "$scratch/compiled.json" "$render" "$scratch/out.json"
echo "exit $code"
