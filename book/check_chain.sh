#!/usr/bin/env bash
# The chapter mermaid blocks form one chain: each entry id of chapter N is declared, with an identical label, in N and in the chapter whose exit it is.
set -uo pipefail

src=$(cd "$(dirname "$0")/src" && pwd)

# chapter | entry id:chapter it exits from ... | exit ids
chain=(
  "1|dl7:0|compile run emit"
  "2|dl7:0|colon"
  "3|colon:2|rules"
  "4|rules:3|strata"
  "5|strata:4|kernel"
  "6|kernel:5|kernel"
  "7|kernel:6|fold"
  "8|dl7:0|colon"
  "9|colon:8|program"
  "10|program:9|effect"
  "11|effect:10|answers"
  "12|answers:11|store"
  "13|program:9|ddl"
  "14|dl7:0|diag"
  "15|store:12|demo"
  "16|store:12 effect:10|nb"
)

fail() {
  echo "$1: $2" >&2
  exit 1
}

chapter_file() {
  local file
  file=$(ls "$src/$1"_*.md 2>/dev/null | head -1)
  [ -n "$file" ] || fail "chapter $1" "no file"
  echo "$file"
}

block() {
  awk '/^```mermaid$/{inside=1; next} inside && /^```$/{exit} inside' "$1"
}

# The label of the first shape declaring id $2 in chapter $1; surrounding quotes dropped.
label() {
  block "$(chapter_file "$1")" | ID=$2 perl -ne '
    my $id = quotemeta $ENV{ID};
    if (/(?<![\w-])$id(?:\[\((.*?)\)\]|\(\[(.*?)\]\)|\["(.*?)"\]|\[([^\]"]*)\])/) {
      print grep { defined } ($1, $2, $3, $4);
      exit;
    }'
}

links=0
for row in "${chain[@]}"; do
  IFS='|' read -r chapter entries exits <<<"$row"
  name=$(basename "$(chapter_file "$chapter")")
  [ -n "$(block "$(chapter_file "$chapter")")" ] || fail "$name" "no mermaid block"
  for entry in $entries; do
    id=${entry%%:*}
    from=${entry##*:}
    here=$(label "$chapter" "$id")
    there=$(label "$from" "$id")
    [ -n "$here" ] || fail "$name" "entry $id is not declared"
    [ -n "$there" ] || fail "$(basename "$(chapter_file "$from")")" "exit $id is not declared"
    [ "$here" = "$there" ] || fail "$name" "entry $id is [$here], chapter $from exits [$there]"
    links=$((links + 1))
  done
  for id in $exits; do
    [ -n "$(label "$chapter" "$id")" ] || fail "$name" "exit $id is not declared"
  done
done

echo "chain $links links continuous"
