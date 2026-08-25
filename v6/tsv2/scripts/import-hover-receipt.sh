#!/usr/bin/env bash
# receipt for the import-statement + hover_note bridge; details in REPORT.md
set -uo pipefail
TSV2="$(cd "$(dirname "$0")/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
P="$(cd "$V6/.." && pwd)"
PROLOG="$V6/prolog"
DCG="$PROLOG/7_lower/parse_dl_dcg.pl"
COMPILE_SH="$PROLOG/compile/scripts/compile_dl6.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/import-hover.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

fail() { printf 'FAIL  %s\n' "$*"; exit 1; }

# ---- piece 1: the import statement parses and records its span --------------
cat >"$WORK/probe.pl" <<PL
:- use_module('$DCG', [parse_dl_source/5]).
main :-
    read_file_to_codes('$V6/dl/fixtures/import-mini.dl6', Codes, []),
    catch(parse_dl_source('import-mini.dl6', Codes, Prog, _, _),
      E, (print_message(error, E), fail)),
    Prog = prog(Decls, _),
    ( member(import_decl(File, Line, Col, EndLine, EndCol), Decls)
    -> format('IMPORT_DECL  ~w  ~w ~w ~w ~w~n', [File, Line, Col, EndLine, EndCol])
    ; format('NO_IMPORT_DECL~n', []), fail ).
PL
swipl -q -l "$WORK/probe.pl" -g main -t halt >"$WORK/p1.out" 2>/dev/null \
  || fail "piece 1: import did not parse: $(cat "$WORK/p1.out")"
grep -q "IMPORT_DECL" "$WORK/p1.out" \
  || fail "piece 1: no import_decl span: $(cat "$WORK/p1.out")"
sed 's/^/  /' "$WORK/p1.out"
printf 'PASS  piece 1  import statement parses with a recorded span\n'

# ---- piece 2: openapi converter emits expansion-as-data ---------------------
node --experimental-transform-types --input-type=module -e '
import { OpenapiToDl6 } from "'"$TSV2"'/scripts/openapi_to_dl6.ts";
const doc = { components: { schemas: {
  AbilityDetail: { type: "object", properties: {
    names: { type: "array", items: { $ref: "#/components/schemas/AbilityName" } },
    meta: { type: "object", properties: { slot: { type: "integer" } } },
    tags: { type: "array", items: { type: "string" } } } },
  AbilityName: { type: "object", properties: { name: { type: "string" } } } } } };
const c = new OpenapiToDl6(doc, "full");
const exp = c.expansionDl6();
const ok = exp.startsWith("rel schema_expansion(source: text, rel: text, decl: text).")
  && /schema_expansion\('\''AbilityDetail'\'', '\''ability_detail'\'', /.test(exp)
  && /schema_expansion\('\''AbilityDetail'\'', '\''ability_detail__meta'\'', /.test(exp);
if (!ok) { console.error(exp); process.exit(1); }
console.log("  expansion:", exp.split("\n").length - 1, "facts; schema_expansion rel; lifted rel carries its source");
' >"$WORK/p2.out" 2>&1 || fail "piece 2: expansion emit broken: $(cat "$WORK/p2.out")"
sed 's/^/  /' "$WORK/p2.out"
printf 'PASS  piece 2  converter emits per-schema expansion as data\n'

# ---- piece 3: a program heads hover_note with v5'\''s 6-column schema ---------
if ! bash "$COMPILE_SH" "$V6/dl/fixtures/import-hover-rail.dl6" "$WORK/rail.ts" \
  >"$WORK/p3.out" 2>&1; then
  fail "piece 3: hover-rail did not compile: $(cat "$WORK/p3.out")"
fi
grep -q 'CREATE TABLE "hover_note" ("path" INTEGER NOT NULL, "line" INTEGER NOT NULL, "col" INTEGER NOT NULL, "end_line" INTEGER NOT NULL, "end_col" INTEGER NOT NULL, "md" INTEGER' "$WORK/rail.ts" \
  || fail "piece 3: hover_note table missing its 6-column schema"
grep -q '```dl6' "$WORK/rail.ts" || fail "piece 3: fenced-block markdown missing"
grep -q '| native | dl6 |' "$WORK/rail.ts" || fail "piece 3: markdown table missing"
grep -q '<b>raw html</b>' "$WORK/rail.ts" || fail "piece 3: raw-HTML note missing"
grep -oE 'INSERT OR IGNORE INTO "hover_note" \("path", "line", "col", "end_line", "end_col", "md"\) SELECT b0\."path", b0\."line", b0\."col", b0\."end_line", b0\."end_col", b1\."decl" FROM "import_stmt" b0, "schema_expansion" b1' "$WORK/rail.ts" \
  | sed 's/^/  derive: /'
printf 'PASS  piece 3  hover_note sink compiles and derives from expansion data\n'

printf 'IMPORT-HOVER RECEIPT HOLDS\n'
