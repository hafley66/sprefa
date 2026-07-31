#!/usr/bin/env bash
# run-capped.sh -- bounded process-group execution. Source this; do not execute it.
#
# Every compute invocation in the toolchain runs
# under a budget with a named timeout failure.
#
# `run_capped` forks a child into its own process group. On timeout the parent
# kills the group before returning 124.
#     fork -> child calls setpgrp(0,0) then exec, so the command and everything
#     it spawns share ONE process group; parent arms SIGALRM and, on fire,
#     `kill -KILL -$pid` takes the whole group down before exiting 124.
#   Exit 124 is the coreutils convention and callers read it as "budget
#   exceeded" specifically, distinct from any other non-zero (a genuine crash).
#
# Functions:
#
#   run_capped SECONDS CMD...
#       The primitive. Exit status is the command's, or 124 on timeout. Prints
#       nothing: callers that want a named line use `capped`.
#
#   capped SECONDS LABEL CMD...
#       run_capped plus the NAMED failure. On 124 it prints, to stderr,
#         TIMEOUT  <script>: <label> exceeded <SECONDS>s
#       and returns 124 with a named stderr line.
#
#   cap_self SECONDS LABEL "$@"
#       Whole-script budget: re-execs the calling script under run_capped and
#       exits with its status. Call it before the script's first `cd`, because
#       it re-runs `bash "$0"` and a relative $0 only resolves from the cwd the
#       script was invoked in. This is the honest muzzle for the served-engine
#       rails, where the expensive work is not a command the script waits on --
#       it is a background node server, the swipl and extractor subprocesses
#       that server spawns, and a poll loop over HTTP. Only a process-group cap
#       around the entire script covers all of that, and the setpgrp in
#       run_capped is exactly what makes the group exist.
#       Re-entry is guarded per label through an exported marker.
#
#   capped_curl SECONDS CURL_ARGS...
#       curl with --max-time. A poll loop's attempt counter does not bound a
#       curl that never returns.
#
#       Served scripts use separate poll and load budgets.
#       A poll (`/ticks`, `/idb/<rel>`) answers in milliseconds and gets a short
#       cap. `POST /program` is not a poll: it holds the connection open for the
#       WHOLE COMPILE, so its cap must sit above the compile's own budget. That
#       Load POSTs read *_LOAD_BUDGET_S; polls read *_HTTP_BUDGET_S.
#
# Call sites choose defaults with headroom for their measured wall time.

# run_capped SECONDS CMD... -- exit 124 on timeout, whole process group killed.
run_capped() {
  local limit="$1"; shift
  perl -e '
    my $limit = shift;
    my $pid = fork();
    if ($pid == 0) { setpgrp(0, 0); exec @ARGV; exit 127; }
    $SIG{ALRM} = sub { kill("KILL", -$pid); waitpid($pid, 0); exit 124; };
    alarm $limit;
    waitpid($pid, 0);
    alarm 0;
    exit($? >> 8);
  ' "$limit" "$@"
}

# capped SECONDS LABEL CMD... -- run_capped with the named failure line.
capped() {
  local limit="$1" label="$2"; shift 2
  # Capture failures without letting set -e skip the named timeout line.
  local status
  run_capped "$limit" "$@" && status=0 || status=$?
  if [ "$status" -eq 124 ]; then
    printf 'TIMEOUT  %s: %s exceeded %ss\n' "$(basename "${0:-run-capped}")" "$label" "$limit" >&2
  fi
  return "$status"
}

# cap_self SECONDS LABEL "$@" -- re-exec THIS script under one process-group cap.
cap_self() {
  local limit="$1" label="$2"; shift 2
  local marker
  marker="CAPPED_SELF_$(printf '%s' "$label" | tr -c '[:alnum:]' '_')"
  [ -n "${!marker:-}" ] && return 0
  export "$marker=$limit"
  local status
  run_capped "$limit" bash "$0" "$@" && status=0 || status=$?
  if [ "$status" -eq 124 ]; then
    printf 'TIMEOUT  %s: whole run (%s) exceeded %ss; the process group was killed\n' \
      "$(basename "$0")" "$label" "$limit" >&2
  fi
  exit "$status"
}

# capped_curl SECONDS CURL_ARGS... -- curl that cannot outlive its budget.
capped_curl() {
  local limit="$1"; shift
  curl --max-time "$limit" "$@"
}

# EXECUTED, not sourced: `run-capped.sh SECONDS CMD...` is `run_capped` as a
# command. That form exists for the one caller that cannot source anything --
# an `sh` host template inside a .dl6 program, which is a single shell line the
# engine runs. Templates reach it through an exported path variable, the same
# way dataflow-atlas.dl6's hosts reach DL_EXTRACT_BIN and ATLAS_XREF_FACTS
# (fillTemplate escapes `$` in any value it splices, so a path arriving as a
# rel column could never expand).
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  if [ "$#" -lt 2 ]; then
    printf 'usage: run-capped.sh SECONDS COMMAND [ARG...]\n' >&2
    exit 2
  fi
  run_capped "$@"
fi
