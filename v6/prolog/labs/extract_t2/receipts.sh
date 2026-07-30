#!/usr/bin/env bash
# receipts.sh : everything the extract-t2 lab claims, run end to end.
#
#   bash v6/prolog/labs/extract_t2/receipts.sh
#
# Exit 0 only if every leg lands the way the verdict says it does.
#
# Legs:
#   1. compile gate      every shipped .dl6 is accepted by `bop check`
#   2. two-door grading  five reifiers, reference engine vs served tsv2 engine
#   3. Q1 + Q5           construct census and round trip over the real Petstore
#   4. Q3                the cross-repo join's answers, asserted exactly
#   5. sabotage          four probes that must FAIL, so that leg 2 is not vacuous
#
# Hermetic: SPREFA_CONFIG points at a path that does not exist, DL_NO_DAEMON=1,
# every served database is :memory:. Nothing reads or writes ~/.local/state, no
# daemon is spoken to, and no network is used -- corpus/ is vendored, and
# corpus/regen.sh (which does reach the network via npx) is NOT run here.

set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
TSV2="$REPO/v6/tsv2"
export SPREFA_CONFIG=/nonexistent/extract-t2.toml
export DL_NO_DAEMON=1

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

PASS=0
FAIL=0
ok()  { printf 'PASS %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf 'FAIL %s\n' "$1"; FAIL=$((FAIL + 1)); }

command -v swipl   >/dev/null || { echo "swipl is required";   exit 2; }
command -v curl    >/dev/null || { echo "curl is required";    exit 2; }
command -v python3 >/dev/null || { echo "python3 is required"; exit 2; }

oracle_facts() {
  # $1 program, $2 schedule -> oracle tick log on stdout
  ( cd "$HERE" && swipl -q -l t2_oracle.pl -g "oracle('$1','$2')" -g halt ) 2>/dev/null
}

count_rows() {
  # stdin: tick log; $1: rel -> number of added rows for that rel
  python3 -c '
import json, sys
rel = sys.argv[1]
total = 0
for raw in sys.stdin:
    raw = raw.strip()
    if raw:
        total += len(json.loads(raw)["deltas"].get(rel, {}).get("add", []))
print(total)
' "$1"
}

echo "── leg 1: compile gate (bop check) ──────────────────────────────────────"
for program in openapi.dl6 proto.dl6 avro.dl6 xrepo.dl6 graphql.dl6; do
  if ( cd "$TSV2" && npm run --silent bop -- check "$HERE/$program" ) 2>&1 \
       | grep -q 'refusal:'; then
    bad "bop check $program"
  else
    ok "bop check $program"
  fi
done

echo
echo "── leg 2: two-door grading over real schema documents ───────────────────"
two_door() {
  local label="$1"; shift
  if bash "$HERE/two_door.sh" "$@" 2>/dev/null | tail -1 | grep -q '^IDENTICAL'; then
    ok "$label"
  else
    bad "$label"
    bash "$HERE/two_door.sh" "$@" 2>/dev/null | sed -n 1,20p
  fi
}
two_door "openapi  (Swagger Petstore, 17,106 B)"  "$HERE/openapi.dl6"  "$HERE/openapi.schedule.json"
two_door "proto    (google/protobuf/struct)"      "$HERE/proto.dl6"    "$HERE/proto.schedule.json"
two_door "avro     (apache/avro interop)"         "$HERE/avro.dl6"     "$HERE/avro.schedule.json"
two_door "xrepo    (three-repo federation)"       "$HERE/xrepo.dl6"    "$HERE/xrepo.schedule.json"
two_door "graphql  (swapi introspection, 110 KB)" "$HERE/graphql.dl6"  "$HERE/graphql.schedule.json" 1.5 --except introspection

echo
echo "── leg 3: Q1 construct census + Q5 round trip ───────────────────────────"
oracle_facts openapi.dl6 openapi.schedule.json >"$SCRATCH/openapi.jsonl"
if python3 "$HERE/fidelity.py" "$HERE/corpus/openapi-petstore.json" "$SCRATCH/openapi.jsonl" \
     >"$SCRATCH/fidelity.txt" 2>&1; then
  ok "round trip EXACT over the claimed subset"
  sed -n '/Q1 construct census/,$p' "$SCRATCH/fidelity.txt" | sed 's/^/    /'
else
  bad "round trip"
  sed -n 1,40p "$SCRATCH/fidelity.txt"
fi

echo
echo "── leg 4: Q3, the cross-repo join's answers ─────────────────────────────"
oracle_facts xrepo.dl6 xrepo.schedule.json >"$SCRATCH/xrepo.jsonl"
assert_count() {
  local rel="$1" want="$2" got
  got="$(count_rows "$rel" <"$SCRATCH/xrepo.jsonl")"
  if [ "$got" = "$want" ]; then ok "$rel = $want"; else bad "$rel = $got, want $want"; fi
}
assert_count calls_shape          5
assert_count undeclared_shape_dep 2
assert_count dangling_shape       1
assert_count depends_on           2

echo
echo "── leg 5: sabotage, four probes that MUST fail ──────────────────────────"

# (a) UNANCHORED DESCENT. proto-unanchored.dl6 is the first draft of proto.dl6,
#     with `**` not anchored on protobufjs's `nested` container. It mints enum
#     variants for a FIELD MAP that is not an enum. The anchored program must
#     mint none of them.
unanchored="$(oracle_facts proto-unanchored.dl6 proto.schedule.json | count_rows enum_variant)"
anchored="$(oracle_facts proto.dl6 proto.schedule.json | count_rows enum_variant)"
if [ "$unanchored" -gt "$anchored" ] && [ "$anchored" = "1" ]; then
  ok "(a) unanchored descent invents $((unanchored - anchored)) phantom variants; anchored mints $anchored"
else
  bad "(a) unanchored=$unanchored anchored=$anchored (want unanchored > anchored = 1)"
fi

# (b) HETEROGENEOUS HOLE. avro-heterogeneous.dl6 binds avro's `type` slot, which
#     holds strings AND objects AND arrays, to ONE variable. It must DIVERGE, or
#     leg 2's identical results prove nothing about structured values.
if bash "$HERE/two_door.sh" "$HERE/avro-heterogeneous.dl6" \
     "$HERE/avro-heterogeneous.schedule.json" 2>/dev/null | tail -1 | grep -q '^IDENTICAL'; then
  bad "(b) the heterogeneous hole was IDENTICAL -- the two-door gate is inert"
else
  ok "(b) heterogeneous hole diverges across the doors, as finding D2 says"
fi

# (c) THE VANISHING ROW. hole_json.dl6 over a document whose only value is the
#     scalar string "int": the reference engine derives one row, the emitted
#     engine derives NONE and raises nothing.
hole_oracle="$(oracle_facts hole_json.dl6 hole-scalar.schedule.json | count_rows bound)"
hole_served="$(bash "$HERE/two_door.sh" "$HERE/hole_json.dl6" "$HERE/hole-scalar.schedule.json" 2>/dev/null \
                 | grep -c '^+bound' || true)"
if [ "$hole_oracle" = "1" ] && [ "$hole_served" = "0" ]; then
  ok "(c) json column silently drops the scalar row (oracle 1, served 0, no error)"
else
  bad "(c) oracle=$hole_oracle served-added=$hole_served (want 1 and 0)"
fi

# (d) THE LINT IS NOT INERT. Remove pet-dashboard's declared dependency and the
#     undeclared-shape lint must grow by exactly its three cross-repo uses.
mkdir -p "$SCRATCH/pet-dashboard"
python3 -c '
import json, sys
manifest = json.load(open(sys.argv[1]))
manifest["dependencies"] = {}
json.dump(manifest, open(sys.argv[2], "w"))
' "$HERE/corpus/xrepo/pet-dashboard/package.json" "$SCRATCH/pet-dashboard/package.json"
( cd "$HERE" && python3 mkschedule.py \
    "spec:pet-contracts,@corpus/xrepo/pet-contracts/openapi.json" \
    "spec:pet-dashboard,@corpus/xrepo/pet-dashboard/openapi.json" \
    "spec:pet-billing,@corpus/xrepo/pet-billing/openapi.json" \
    "manifest:pet-contracts,@corpus/xrepo/pet-contracts/package.json" \
    "manifest:pet-dashboard,@$SCRATCH/pet-dashboard/package.json" \
    "manifest:pet-billing,@corpus/xrepo/pet-billing/package.json" \
    "contract_file:pet-contracts,pet-contracts/openapi.json" ) >"$SCRATCH/sabotage.schedule.json"
sabotaged="$(oracle_facts xrepo.dl6 "$SCRATCH/sabotage.schedule.json" | count_rows undeclared_shape_dep)"
if [ "$sabotaged" = "5" ]; then
  ok "(d) dropping one declared dependency takes the lint 2 -> 5"
else
  bad "(d) undeclared_shape_dep = $sabotaged after sabotage, want 5"
fi

echo
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] && echo "EXTRACT T2 LAB RECEIPTS HOLD"
exit $(( FAIL > 0 ))
