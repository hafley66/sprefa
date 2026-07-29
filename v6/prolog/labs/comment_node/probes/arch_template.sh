#!/usr/bin/env bash
# The exact shell the ARCH marker host template runs, developed here so the
# .dl6 carries a template that is known to work rather than a guess.
# stdin: nothing. arg 1: the file.
grep -nE 'ARCH *\{' "$1" \
  | sed -E 's/^([0-9]+):.*"url":"([^"]*)".*/\1 \2/' \
  | awk '{ url=$2; parent=url; last=url;
           if (index(url, "/") > 0) { sub(/\/[^\/]*$/, "", parent); sub(/.*\//, "", last) }
           else { parent = "" }
           printf "{\"line\":%s,\"url\":\"%s\",\"parent\":\"%s\",\"last\":\"%s\"}\n", $1, url, parent, last }'
