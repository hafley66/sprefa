#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=en_US.UTF-8
root=$(cd "$(dirname "$0")/../.." && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

swipl -q -g run_tests -t halt "$root/v6/prolog/compile/test/plunit_tests.pl"
cargo build --quiet --manifest-path "$root/v6/dd-runner/Cargo.toml"

grade() {
  local fixture_file=$1
  local fixture_name=$2
  local plan="$scratch/$fixture_name.json"
  local oracle="$scratch/$fixture_name.oracle.jsonl"
  local runner="$scratch/$fixture_name.runner.jsonl"
  swipl -q -l "$root/v6/prolog/compile/6_emit_dd_plan.pl" \
    -g "emit_dd_plan:emit_fixture_dd_plan_json('$fixture_file',$fixture_name,'$plan')" -g halt
  swipl -q -l "$root/v6/prolog/conformance/ticklog.pl" -g "emit($fixture_name)" -g halt > "$oracle"
  "$root/v6/dd-runner/target/debug/dd-runner" "$plan" > "$runner"
  diff -u "$oracle" "$runner"
  printf '%s: byte-diff clean\n' "$fixture_name"
}

grade "$root/v6/prolog/conformance/fixtures/engine_core.pl" retraction_only_tick_retracts_level_view
grade "$root/v6/prolog/conformance/fixtures/5_value_plane.pl" float_exact_join_has_no_epsilon
grade "$root/v6/prolog/conformance/fixtures/5_value_plane.pl" float_avg_is_grouped
