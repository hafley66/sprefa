#!/usr/bin/env bash
set -euo pipefail

args=()
for arg in "$@"; do
  if [[ "$arg" == "gpt-5.6-sol[high]" ]]; then
    args+=("gpt-5.6-sol")
  else
    args+=("$arg")
  fi
done

exec npx --yes acpx@0.13.1 "${args[@]}"
