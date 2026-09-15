#!/usr/bin/env bash
# check_outputs.sh [chapter.md...]: every console block opening with `$ <command>` reruns from v8/ and must print its body.
set -uo pipefail

book=$(cd "$(dirname "$0")" && pwd)
v8=$(cd "$book/.." && pwd)
export DL8=${DL8:-${CARGO_TARGET_DIR:-$v8/target}/release/dl8}
checked=0

chapters=("$@")
[ ${#chapters[@]} -gt 0 ] || chapters=("$book"/src/*.md)

for chapter in "${chapters[@]}"; do
  name=$(basename "$chapter")
  inside=0
  while IFS= read -r line || [ -n "$line" ]; do
    if [ $inside -eq 0 ]; then
      [ "$line" = '```console' ] && inside=1 && command="" && body=""
      continue
    fi
    if [ "$line" = '```' ]; then
      inside=0
      [ -n "$command" ] || continue
      got=$(cd "$v8" && bash -c "$command" 2>&1)
      shown=$(printf '%s' "$body")
      if [ "$got" != "$shown" ]; then
        echo "$name: \$ $command" >&2
        diff <(printf '%s\n' "$shown") <(printf '%s\n' "$got") >&2
        exit 1
      fi
      checked=$((checked + 1))
    elif [ -z "$command" ] && [ -z "$body" ] && [[ "$line" == '$ '* ]]; then
      command=${line#\$ }
    else
      body+="$line"$'\n'
    fi
  done < "$chapter"
done

echo "outputs $checked identical"
