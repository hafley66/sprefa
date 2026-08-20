#!/usr/bin/env bash
# Compiles each shared_frontier fixture both arms, prints the oracle tick log
# for the fixture-term cases, then runs the parity gate.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
fixtures="$here/tests/shared_frontier"
terms="$fixtures/retraction.fixtures.pl"
mkdir -p "$here/gen_emitted"
for source in "$fixtures"/*.dl6; do
  name="$(basename "$source" .dl6)"
  swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "compile_dl6('$source','$here/gen_emitted/${name}_per.ts',[])" -g halt
  swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "compile_dl6('$source','$here/gen_emitted/${name}_shared.ts',[frontier(shared)])" -g halt
done
# The retraction cases compile from a fixture TERM, so the oracle and both
# arms read one source.
for name in sf_retract_current sf_retract_stale sf_negation_support sf_two_rule_support; do
  swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "default_intern_mode(M), compile_fixture($name,'$terms','$here/gen_emitted/${name}_per.ts',emit_ts:emit_program,[intern(M)])" -g halt
  swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "default_intern_mode(M), compile_fixture($name,'$terms','$here/gen_emitted/${name}_shared.ts',emit_ts:emit_program,[intern(M),frontier(shared)])" -g halt
  swipl -q -l "$root/v6/prolog/conformance/ticklog.pl" \
    -g "ensure_loaded('$terms')" -g "emit($name)" -g halt > "$here/gen_emitted/${name}.oracle.jsonl"
done
cd "$here"
node --experimental-transform-types scripts/shared-frontier-gate.ts
