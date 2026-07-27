#!/usr/bin/env bash
# Shared wrapper: run a prolog engine over pure_reach.pl under /usr/bin/time -l,
# merge its 6-field CSV (stdout) with measured RSS, emit the harness 8-field
# CSV on stderr. Usage: pure_wrap.sh <engine-cmd...> -- <layers> <width>
set -uo pipefail
dir="$(cd "$(dirname "$0")/.." && pwd)"
cmd=()
while [[ "$1" != "--" ]]; do cmd+=("$1"); shift; done
shift; layers="$1"; width="$2"
tmp=$(mktemp)
line=$(/usr/bin/time -l "${cmd[@]}" "$dir/pure_reach.pl" -g "main($layers,$width),halt" 2>"$tmp" | grep '^CSV,' | head -1)
rss=$(awk '/maximum resident set size/ {print $1}' "$tmp")
rm -f "$tmp"
[[ -z "$line" ]] && exit 1
rss_mb=$(awk -v b="${rss:-0}" 'BEGIN{printf "%.1f", b/1048576}')
echo "${line},0,${rss_mb}" >&2
