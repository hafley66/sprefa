#!/usr/bin/env bash
# pairs.sh : the COMPILED leg of the composition grade, over the real .dl6 text
# door. Each pair is the same program written twice -- once with `:=`, once
# with the expression in argument position -- and the two emitted TypeScript
# modules are diffed after normalizing only the program NAME (which is the
# input file's basename and cannot be equal by construction).
#
# Byte identity of the module grades BOTH emitter modes at once: one module
# carries insertSql/supportSql (incremental) and recomputeSql (naive snapshot
# referee) side by side, and SPREFA_TSV2_EMITTER_MODE only selects which the
# runtime executes.
#
# Pairs are DECLARED (`rel name(col: type, ...)`) on purpose. Without a decl
# the head column name is drawn from the surface variable, so the `:=` spelling
# names the column after the bound variable and the expression spelling falls
# back to positional `col2`. That is a naming difference, not a semantic one,
# and a decl removes it -- which is itself the measured answer to "what does
# `:=` buy": one column name, on undeclared rels only.

set -uo pipefail
LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$LAB/out"

identical=0
asymmetric=0
refused_both=0

grade_pair() {
  local label="$1" bind="$2" expr="$3"
  local bind_out="$LAB/out/$bind.ts" expr_out="$LAB/out/$expr.ts"
  local bind_ok=1 expr_ok=1

  bash "$LAB/../../compile/scripts/compile_dl6.sh" "$LAB/probes/$bind.dl6" "$bind_out" \
    >/dev/null 2>"$LAB/out/$bind.err" || bind_ok=0
  bash "$LAB/../../compile/scripts/compile_dl6.sh" "$LAB/probes/$expr.dl6" "$expr_out" \
    >/dev/null 2>"$LAB/out/$expr.err" || expr_ok=0

  if [ "$bind_ok" = 1 ] && [ "$expr_ok" = 1 ]; then
    sed "s/$bind/PROGRAM/g" "$bind_out" > "$LAB/out/$label.a"
    sed "s/$expr/PROGRAM/g" "$expr_out" > "$LAB/out/$label.b"
    if diff -q "$LAB/out/$label.a" "$LAB/out/$label.b" >/dev/null; then
      echo "BYTE_IDENTICAL     $label"
      identical=$((identical + 1))
    else
      echo "DIFFERING          $label"
      diff "$LAB/out/$label.a" "$LAB/out/$label.b" | head -6
    fi
  elif [ "$bind_ok" = 0 ] && [ "$expr_ok" = 0 ]; then
    local a b
    a=$(grep -o 'reason=[a-z_]*' "$LAB/out/$bind.err" | head -1)
    b=$(grep -o 'reason=[a-z_]*' "$LAB/out/$expr.err" | head -1)
    if [ "$a" = "$b" ]; then
      echo "REFUSED_BOTH_SAME  $label  ($a)"
      refused_both=$((refused_both + 1))
    else
      echo "REFUSED_BOTH_DIFF  $label  bind=$a expr=$b"
    fi
  else
    local a b
    a=$( [ "$bind_ok" = 1 ] && echo compiled || grep -o 'reason=[a-z_]*' "$LAB/out/$bind.err" | head -1 )
    b=$( [ "$expr_ok" = 1 ] && echo compiled || grep -o 'reason=[a-z_]*' "$LAB/out/$expr.err" | head -1 )
    echo "ASYMMETRY          $label  bind=$a  expr=$b"
    asymmetric=$((asymmetric + 1))
  fi
}

grade_pair map_arithmetic        P11_declared_head_bind  P11_declared_head_expr
grade_pair map_concat            F1_flagship_bind        F1_flagship_head
grade_pair map_two_expressions   P7_two_exprs_bind       P7_two_exprs_expr
grade_pair chained_binds         X1_chained_bind         X1_chained_head
grade_pair naming_for_reuse      C1_reuse_bind           C1_reuse_repeat
grade_pair json_braces_value     J1_json_bind            J1_json_expr
grade_pair edge_head_arithmetic  A1_edge_head_bind       A1_edge_head_expr

echo
echo "RESULT byte_identical=$identical refused_both=$refused_both asymmetric=$asymmetric"
