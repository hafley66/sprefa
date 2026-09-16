#!/usr/bin/env bash
# Each dl7 or prolog block opens with `; fixture: <path>[:first-last]` (`%` for prolog) and equals that file or range; trailing newlines aside, since a fence cannot show a file without one.
set -uo pipefail

book=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$book/.." && pwd)
checked=0

fail() {
  echo "$1: $2" >&2
  exit 1
}

compare() {
  local chapter=$1 marker=$2 body=$3
  local spec=${marker#* fixture: }
  [ "$spec" != "$marker" ] || fail "$chapter" "block without a fixture marker: $marker"
  local path=$spec range=""
  if [[ "$spec" =~ ^(.*):([0-9]+)-([0-9]+)$ ]]; then
    path=${BASH_REMATCH[1]}
    range="${BASH_REMATCH[2]},${BASH_REMATCH[3]}p"
  fi
  [ -f "$root/$path" ] || fail "$chapter" "$path does not exist"
  local expected
  if [ -n "$range" ]; then
    expected=$(sed -n "$range" "$root/$path")
  else
    expected=$(cat "$root/$path")
  fi
  local shown
  shown=$(printf '%s' "$body")
  [ "$expected" = "$shown" ] || {
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$shown") >&2
    fail "$chapter" "$spec differs"
  }
  checked=$((checked + 1))
}

for chapter in "$book"/src/*.md; do
  name=$(basename "$chapter")
  inside=0
  while IFS= read -r line || [ -n "$line" ]; do
    if [ $inside -eq 0 ]; then
      case "$line" in
        '```dl7' | '```prolog')
          inside=1
          marker=""
          header=1
          body=""
          ;;
      esac
      continue
    fi
    if [ "$line" = '```' ]; then
      compare "$name" "$marker" "$body"
      inside=0
    elif [ -z "$marker" ]; then
      marker=$line
    elif [ $header -eq 1 ] && [[ "$line" == "; diagnostic: "* || "$line" == "; compile: "* ]]; then
      continue
    else
      header=0
      body+="$line"$'\n'
    fi
  done < "$chapter"
done

echo "blocks $checked diff-clean"
