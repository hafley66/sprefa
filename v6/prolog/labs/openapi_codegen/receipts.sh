#!/usr/bin/env bash
# receipts.sh -- everything this lab claims, run end to end.
#
#   bash v6/prolog/labs/openapi_codegen/receipts.sh
#
# 1. emit    facts -> openapi.json, and assert the checked-in artifact is
#            byte-identical to what the emitter produces right now (the
#            generated-artifact staleness class: gen_staleness_gate, and the
#            same check tests/bopCommandInventory.test.ts makes of
#            cli/0_inventory.ts).
# 2. validate  real OpenAPI 3.1 validation, Redocly CLI (pinned).
# 3. green   the parity gate, all four sources incl. a live server.
# 4. red     the same gate against a spec with one route fact dropped.
# 5. green   again, proving the red was the sabotage and not a leftover.
#
# Exit 0 only if every leg lands the way it says.

set -uo pipefail

LAB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${LAB_DIR}/../../../.." && pwd)"
REDOCLY_VERSION="2.41.1"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT

fail() { printf 'RECEIPT FAILED: %s\n' "$1" >&2; exit 1; }

cd "${REPO_ROOT}"

# ── 1. emit + staleness ──────────────────────────────────────────────────────
printf '== 1. emit ==\n'
OPENAPI_LAB_OUT="${SCRATCH}/fresh.json" \
  swipl -q -l "${LAB_DIR}/emit_openapi.pl" -g emit_openapi -g halt \
  || fail "emitter exited nonzero"
diff -u "${LAB_DIR}/openapi.json" "${SCRATCH}/fresh.json" \
  || fail "checked-in openapi.json is stale; re-run emit_openapi"
printf 'openapi.json is current with the facts (%s bytes)\n' "$(wc -c < "${LAB_DIR}/openapi.json" | tr -d ' ')"

# ── 2. validate ──────────────────────────────────────────────────────────────
printf '\n== 2. validate (Redocly CLI %s) ==\n' "${REDOCLY_VERSION}"
( cd "${LAB_DIR}" \
  && pnpm --package="@redocly/cli@${REDOCLY_VERSION}" dlx redocly lint --config redocly.yaml openapi.json ) \
  || fail "redocly lint rejected the emitted spec"

# ── 3. parity gate, green ────────────────────────────────────────────────────
printf '\n== 3. parity gate GREEN ==\n'
node --test "${LAB_DIR}/2_parity.mjs" || fail "parity gate red on the honest spec"

# ── 4. parity gate, red under sabotage ───────────────────────────────────────
printf '\n== 4. parity gate RED (readRelation fact removed) ==\n'
OPENAPI_LAB_DROP=readRelation OPENAPI_LAB_OUT="${SCRATCH}/lying.json" \
  swipl -q -l "${LAB_DIR}/emit_openapi.pl" -g emit_openapi -g halt \
  || fail "sabotage emit exited nonzero"
# The lying spec must still be a VALID OpenAPI document -- a gate that only
# catches malformed JSON would catch nothing. It lies by omission, not shape.
( cd "${LAB_DIR}" \
  && pnpm --package="@redocly/cli@${REDOCLY_VERSION}" dlx redocly lint --config redocly.yaml "${SCRATCH}/lying.json" ) \
  || fail "the sabotaged spec is malformed; the receipt would prove nothing"
if OPENAPI_LAB_SPEC="${SCRATCH}/lying.json" node --test "${LAB_DIR}/2_parity.mjs" > "${SCRATCH}/red.log" 2>&1; then
  fail "parity gate PASSED a spec missing GET /idb/:rel -- the gate is inert"
fi
printf 'gate went red as required:\n'
grep -E '^(✖ emitted|ℹ (tests|pass|fail))' "${SCRATCH}/red.log" | sort -u || true

# ── 5. parity gate, green again ──────────────────────────────────────────────
printf '\n== 5. parity gate GREEN again ==\n'
node --test "${LAB_DIR}/2_parity.mjs" || fail "gate stayed red after the sabotage was withdrawn"

printf '\nOPENAPI CODEGEN LAB RECEIPTS HOLD\n'
