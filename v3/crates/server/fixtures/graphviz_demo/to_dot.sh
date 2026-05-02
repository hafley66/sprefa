#!/usr/bin/env bash
# to_dot.sh — fold sprefa-run cursor output into a graphviz .dot.
#
# Reads stdin lines of the shape:
#   sprefa@HEAD <file>: ITEM=<x>, MOD=<y>
# and emits one edge per line:
#   "<file>" -> "<y>";
#
# Headers + closing brace are added once. Paths are basename'd to keep
# nodes legible; flip BASENAME=0 to get full paths.
set -euo pipefail
BASENAME="${BASENAME:-1}"

printf 'digraph imports {\n'
printf '  rankdir=LR;\n'
printf '  node [shape=box, fontname="Helvetica", fontsize=10];\n'

awk -v BN="$BASENAME" '
  /^[[:space:]]+[^@]+@[^:]+:/ {
    sub(/^[[:space:]]+/, "")
    split($0, hdr, ": ")
    loc = hdr[1]; kvs = hdr[2]
    sub(/^[^ ]+ /, "", loc)        # strip "sprefa@HEAD "
    if (BN == "1") { sub(/.*\//, "", loc) }
    n = split(kvs, parts, ", ")
    mod = ""
    for (i = 1; i <= n; i++) {
      if (parts[i] ~ /^MOD=/) { mod = parts[i]; sub(/^MOD=/, "", mod) }
    }
    if (mod != "") {
      printf "  \"%s\" -> \"%s\";\n", loc, mod
    }
  }
'

printf '}\n'
