#!/usr/bin/env bash
#
# DL7 deterministic compiler flamechart orchestrator.
#
#   ./v7/bench/2_compiler_flamechart.sh <fixture> [output-directory]
#
# Small non-interactive wrapper: resolve the fixture and output directory, then
# run exactly one bounded swipl process that compiles the fixture and writes the
# five profile artifacts. No analysis, network, package install, or browser
# launch happens here. The Prolog process owns directory creation and prints the
# precise `DL7-PROFILE-ERROR stage=<source|usage|compile|report>`; this script
# only adds `stage=timeout` when the 20-second cap fires and otherwise forwards
# the original exit status unchanged.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
profile_module="${script_dir}/1_compiler_profile.pl"

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    printf 'usage: %s <fixture> [output-directory]\n' "$0" >&2
    printf 'DL7-PROFILE-ERROR stage=usage\n' >&2
    exit 2
fi

fixture="$1"
case "$fixture" in
    /*) ;;
    *)  if [ -f "${repo_root}/${fixture}" ]; then
            fixture="${repo_root}/${fixture}"
        fi
        ;;
esac

if [ "$#" -eq 2 ]; then
    output_directory="$2"
else
    output_directory="${repo_root}/v7/out/compiler-profile"
fi

set +e
timeout 20 swipl -q -s "$profile_module" -g profile_main -t halt -- \
    "$fixture" "$output_directory"
status=$?
set -e

if [ "$status" -eq 124 ]; then
    printf 'DL7-PROFILE-ERROR stage=timeout\n' >&2
    exit 124
fi

if [ "$status" -ne 0 ]; then
    exit "$status"
fi

printf 'DL7-PROFILE-OK out=%s\n' "$output_directory"
