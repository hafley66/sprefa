#!/usr/bin/env bash
# run-capped.sh -- THE TIMEOUT GUN. Source this; do not execute it.
#
# Standing law (user 2026-07-31): every compute invocation in the toolchain runs
# under a budget with a NAMED timeout failure. No open-ended grind anywhere. A
# resource cliff is a named refusal, never a hang, never an OOM death.
#
# ── THE ONE PATTERN, and the anti-pattern it replaces ────────────────────────
#
# macOS ships no coreutils `timeout`, so this repository grew two answers.
#
#   ANTI-PATTERN, measured, do not reintroduce:
#     perl -e 'alarm 600; exec @ARGV' -- cmd ...
#   `exec` replaces the perl process with the command, so SIGALRM terminates
#   THAT process -- and every child it spawned survives, reparented to init.
#   v6/bench-cli/bench.sh's header records the receipt: at a bench timeout the
#   harness moved on to the next cell while the timed-out swipl kept burning a
#   core beside it, so every subsequent measurement was taken against a stolen
#   CPU. Ledger row perl_alarm_orphan.
#
#   THE PATTERN, `run_capped` below:
#     fork -> child calls setpgrp(0,0) then exec, so the command and everything
#     it spawns share ONE process group; parent arms SIGALRM and, on fire,
#     `kill -KILL -$pid` takes the whole group down before exiting 124.
#   Exit 124 is the coreutils convention and callers read it as "budget
#   exceeded" specifically, distinct from any other non-zero (a genuine crash).
#
# ── FUNCTIONS ────────────────────────────────────────────────────────────────
#
#   run_capped SECONDS CMD...
#       The primitive. Exit status is the command's, or 124 on timeout. Prints
#       nothing: callers that want a named line use `capped`.
#
#   capped SECONDS LABEL CMD...
#       run_capped plus the NAMED failure. On 124 it prints, to stderr,
#         TIMEOUT  <script>: <label> exceeded <SECONDS>s
#       and returns 124. LABEL names the leg ("stage 1 compile sweep"), the
#       script names itself from $0. A silent kill is the failure mode this
#       whole lane exists to delete, so the line is not optional.
#
#   cap_self SECONDS LABEL "$@"
#       WHOLE-SCRIPT budget: re-execs the calling script under run_capped and
#       exits with its status. CALL IT BEFORE THE SCRIPT'S FIRST `cd`, because
#       it re-runs `bash "$0"` and a relative $0 only resolves from the cwd the
#       script was invoked in. This is the honest muzzle for the served-engine
#       rails, where the expensive work is not a command the script waits on --
#       it is a background node server, the swipl and extractor subprocesses
#       that server spawns, and a poll loop over HTTP. Only a process-group cap
#       around the entire script covers all of that, and the setpgrp in
#       run_capped is exactly what makes the group exist.
#       Re-entry is guarded per LABEL through an exported marker, so a script
#       that re-invokes itself (atlas.sh's byte-stability leg) arms once and
#       the outer budget covers both runs, while a DIFFERENT capped script
#       called from inside still arms its own.
#
#   capped_curl SECONDS CURL_ARGS...
#       curl with --max-time. A poll loop whose own attempt counter is the only
#       bound is not bounded at all if a single curl inside it never returns:
#       the counter stops advancing. That is the shape of the devlog hang.
#
# ── BUDGETS ──────────────────────────────────────────────────────────────────
#
# Every call site reads an env var with a default measured off that site's real
# wall time with generous headroom, and every site states both numbers where it
# sets the default. A budget that trips on today's honest wall is a mis-set
# budget, not a finding.

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
  # `&& status=0 || status=$?` rather than a bare call followed by `$?`: most
  # callers run under `set -e`, which aborts the FUNCTION at the failing
  # command, so the named line below would never print on the one path it
  # exists for. Measured, not assumed -- the first fail-first receipt on
  # text_door_receipt.sh exited 124 in silence.
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
