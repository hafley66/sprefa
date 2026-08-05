#!/usr/bin/env bash
# The four `sh` host bodies of goldens/comment_rail_golden/ (design: its README).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DL_COMMENT_NODE="${DL_COMMENT_NODE:-$SCRIPT_DIR/comment_node.py}"

die() { printf 'comment-budget-feed: %s\n' "$*" >&2; exit 1; }

[ -n "${DL_EXTRACT_BIN:-}" ] || die "DL_EXTRACT_BIN is unset"
[ -x "$DL_EXTRACT_BIN" ] || die "DL_EXTRACT_BIN is not executable: $DL_EXTRACT_BIN"
[ -f "$DL_COMMENT_NODE" ] || die "comment_node.py is missing: $DL_COMMENT_NODE"

# claude-research/bin/comment-prod is_exempt_path, clause for clause.
is_exempt_path() {
  case "$1" in
    *.md|*.markdown|*.txt|*.json|*.lock|*.jsonl|*.csv|*.svg) return 0 ;;
    *test*|*spec*|*fixture*|*golden*|*conformance*) return 0 ;;
    */LICENSE*|*/node_modules/*|*/target/*|*/dist/*) return 0 ;;
  esac
  return 1
}

json_string() { python3 -c 'import json,sys; sys.stdout.write(json.dumps(sys.argv[1]))' "$1"; }

staged() {
  local path oid flag
  git diff --cached --name-only --diff-filter=ACM | while IFS= read -r path; do
    [ -n "$path" ] || continue
    oid="$(git rev-parse ":$path" 2>/dev/null)" || continue
    [ -n "$oid" ] || continue
    if is_exempt_path "$path"; then flag=1; else flag=0; fi
    printf '{"file_path":%s,"blob_digest":"%s","exempt_flag":%d}\n' \
      "$(json_string "$path")" "$oid" "$flag"
  done
}

# The @@ header walk of the bash tool's added_lines_from_diff, line numbers only.
added() {
  git diff --cached --unified=0 -- "$1" | awk '
    /^@@/ { split($3, header, ","); line = substr(header[1], 2) - 1; next }
    /^\+\+\+/ { next }
    /^\+/ { line++; printf "{\"line\":%d}\n", line; next }
    /^-/ { next }
    /^ / { line++ }
  '
}

# Grades the STAGED BLOB, never the worktree file, under the path's own
# extension because the cst family selects its grammar from the suffix.
with_staged_blob() {
  local path="$1" blob="$2" body="$3" suffix work status
  case "$path" in
    *.*) suffix=".${path##*.}" ;;
    *) suffix="" ;;
  esac
  work="$(mktemp -d "${TMPDIR:-/tmp}/comment-budget.XXXXXX")" || die "mktemp failed"
  if ! git cat-file blob "$blob" >"$work/staged$suffix" 2>/dev/null; then
    rm -rf -- "$work"
    return 0
  fi
  "$body" "$work/staged$suffix"
  status=$?
  rm -rf -- "$work"
  return "$status"
}

nodes_body() {
  "$DL_EXTRACT_BIN" --family cst "$1" 2>/dev/null \
    | python3 "$DL_COMMENT_NODE" comments "$1" \
    | python3 -c '
import json, sys
for raw in sys.stdin:
    if not raw.strip():
        continue
    row = json.loads(raw)
    print(json.dumps({"line": row["line"], "end_line": row["end_line"], "kind": row["kind"]},
                     separators=(",", ":")))
'
}

comment_lines_body() {
  "$DL_EXTRACT_BIN" --family cst "$1" 2>/dev/null \
    | python3 "$DL_COMMENT_NODE" comment-lines "$1" \
    | python3 -c '
import json, sys
for raw in sys.stdin:
    if not raw.strip():
        continue
    row = json.loads(raw)
    print(json.dumps({"line": row["line"], "prose_flag": row["prose_flag"], "prose_seq": row["prose_seq"]},
                     separators=(",", ":")))
'
}

markers_body() {
  grep -n '@comment-ok:' "$1" 2>/dev/null | awk -F: '{ printf "{\"line\":%d}\n", $1 }'
}

raw_lines_body() {
  python3 -c '
import json, sys
with open(sys.argv[1], "rb") as handle:
    for number, raw in enumerate(handle, 1):
        text = raw.decode("utf-8", "replace").rstrip("\n")
        print(json.dumps({"line": number, "line_text": text}, separators=(",", ":")))
' "$1"
}

case "${1:-}" in
  staged) staged ;;
  added) [ $# -eq 2 ] || die "usage: added <path>"; added "$2" ;;
  nodes) [ $# -eq 3 ] || die "usage: nodes <path> <blob>"; with_staged_blob "$2" "$3" nodes_body ;;
  comment-lines) [ $# -eq 3 ] || die "usage: comment-lines <path> <blob>"; with_staged_blob "$2" "$3" comment_lines_body ;;
  markers) [ $# -eq 3 ] || die "usage: markers <path> <blob>"; with_staged_blob "$2" "$3" markers_body ;;
  raw-lines) [ $# -eq 3 ] || die "usage: raw-lines <path> <blob>"; with_staged_blob "$2" "$3" raw_lines_body ;;
  *) die "usage: comment-budget-feed.sh staged | added <path> | nodes <path> <blob> | comment-lines <path> <blob> | markers <path> <blob> | raw-lines <path> <blob>" ;;
esac
