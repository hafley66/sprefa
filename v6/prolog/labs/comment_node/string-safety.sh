#!/usr/bin/env bash
# string-safety.sh -- DELIVERABLE 1's string-literal-safety witness.
#
# The claim under test is route (c)'s cost, stated as a fact and not an
# intuition: a `grep`-shaped host that never joins the grammar reports comment
# rows that ARE NOT COMMENTS, and the corpus this lab dogfoods (v6/prolog and
# v6/tsv2, our own sources) already contains them.
#
# Method: for every line the naive scanner flags, ask whether the grammar
# (route a/b's cst comment spans, string-literal-safe by construction) puts a
# comment node on that line. A flagged line with no comment node on it is a
# FALSE POSITIVE and is printed with its source text.
#
# Both comment syntaxes are witnessed: `%` over the prolog corpus and `//`
# over the TypeScript corpus, since route c's loss is a property of the
# technique and not of one language.
set -uo pipefail
LAB="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$LAB/../../../.." && pwd)"
EX="${DL_EXTRACT_BIN:-$ROOT/v6/sprefa-extract/target/release/extract}"
OUT="${1:-$LAB/out}"
mkdir -p "$OUT"

status=0

witness() {
  local label="$1" dir="$2" glob="$3" token="$4"
  local files="$OUT/ss-$label-files.txt"
  find "$dir" -name "$glob" | sort > "$files"
  : > "$OUT/ss-$label-false.txt"
  local flagged=0 false=0
  while read -r file; do
    # grammar truth: the 1-based lines carrying a comment node
    nice -n 19 "$EX" --family cst "$file" 2>/dev/null \
      | python3 "$LAB/comment_lines.py" "$file" > "$OUT/ss-lines.txt"
    while IFS=: read -r lineno text; do
      flagged=$((flagged + 1))
      if ! grep -qx "$lineno" "$OUT/ss-lines.txt"; then
        false=$((false + 1))
        printf '%s:%s:%s\n' "$file" "$lineno" "$text" >> "$OUT/ss-$label-false.txt"
      fi
    done < <(grep -n -- "$token" "$file")
  done < "$files"
  printf '%-4s scanner-flagged lines %-7s FALSE POSITIVES %s\n' "$label" "$flagged" "$false"
  if [ "$false" -gt 0 ]; then
    echo "  --- witnesses (first 5) ---"
    head -5 "$OUT/ss-$label-false.txt" | sed 's/^/  /'
  else
    echo "  NO false positive found in this corpus -- route c's loss is UNWITNESSED here"
    status=1
  fi
}

witness "pct"   "$ROOT/v6/prolog" '*.pl' '%'
witness "slash" "$ROOT/v6/tsv2"   '*.ts' '//'

exit "$status"
