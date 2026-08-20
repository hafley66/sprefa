#!/usr/bin/env bash
# Compiles each shared_frontier fixture both arms, then runs the parity gate.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$here/gen_emitted"
for source in "$here"/tests/shared_frontier/*.dl6; do
  name="$(basename "$source" .dl6)"
  swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "compile_dl6('$source','$here/gen_emitted/${name}_per.ts',[])" -g halt
  swipl -q -l "$root/v6/prolog/compile.pl" \
    -g "compile_dl6('$source','$here/gen_emitted/${name}_shared.ts',[frontier(shared)])" -g halt
done
cd "$here"
node --experimental-transform-types scripts/shared-frontier-gate.ts
