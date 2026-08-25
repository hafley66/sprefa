#!/usr/bin/env bash
# One move per commit, in DFS import order from compile.pl (v6/dl/prolog_graph/import_graph.dl6).
# Load check after each; stop and rewind on the first failure.
set -u
cd "$(git rev-parse --show-toplevel)"
P=v6/prolog
moves=(
"$P/0_dot_expand.pl $P/0_dot_expand/0_dot_expand.pl"
"$P/compile/registry.pl $P/0_dot_expand/registry.pl"
"$P/0_type_plane.pl $P/0_dot_expand/0_type_plane.pl"
"$P/conformance/body.pl $P/0_dot_expand/body.pl"
"$P/0_body_walk.pl $P/0_dot_expand/0_body_walk.pl"
"$P/1_expansion.pl $P/1_expansion/1_expansion.pl"
"$P/0_enum_expand.pl $P/1_expansion/0_enum_expand.pl"
"$P/0_program_check.pl $P/1_expansion/0_program_check.pl"
"$P/0_option_expand.pl $P/1_expansion/0_option_expand.pl"
"$P/0_type_ids.pl $P/1_expansion/0_type_ids.pl"
"$P/0_generic_expand.pl $P/1_expansion/0_generic_expand.pl"
"$P/compile/0_trace.pl $P/1_expansion/0_trace.pl"
"$P/0_anonymous_expand.pl $P/1_expansion/0_anonymous_expand.pl"
"$P/0_annotation_expand.pl $P/1_expansion/0_annotation_expand.pl"
"$P/0_compiler_relations.pl $P/1_expansion/0_compiler_relations.pl"
"$P/0_generic_expand/0_expand.pl $P/1_expansion/generic_expand/0_expand.pl"
"$P/0_generic_expand/0a_type_apply_requests.pl $P/1_expansion/generic_expand/0a_type_apply_requests.pl"
"$P/0_generic_expand/0b_expansion_pipeline.pl $P/1_expansion/generic_expand/0b_expansion_pipeline.pl"
"$P/0_generic_expand/1_annotations.pl $P/1_expansion/generic_expand/1_annotations.pl"
"$P/0_generic_expand/2_compiler_plane.pl $P/1_expansion/generic_expand/2_compiler_plane.pl"
"$P/0_generic_expand/3_enum_templates.pl $P/1_expansion/generic_expand/3_enum_templates.pl"
"$P/0_generic_expand/4_type_views.pl $P/1_expansion/generic_expand/4_type_views.pl"
"$P/0_generic_expand/5_type_freeze.pl $P/1_expansion/generic_expand/5_type_freeze.pl"
"$P/0_generic_expand/5a_type_projection.pl $P/1_expansion/generic_expand/5a_type_projection.pl"
"$P/0_generic_expand/5b_type_graph.pl $P/1_expansion/generic_expand/5b_type_graph.pl"
"$P/0_generic_expand/6_type_conformance.pl $P/1_expansion/generic_expand/6_type_conformance.pl"
"$P/0_generic_expand/7_generic_instances.pl $P/1_expansion/generic_expand/7_generic_instances.pl"
"$P/0_generic_expand/8_type_rewrite.pl $P/1_expansion/generic_expand/8_type_rewrite.pl"
"$P/0_generic_expand/8a_key_wrappers.pl $P/1_expansion/generic_expand/8a_key_wrappers.pl"
"$P/compile_messages.pl $P/1_expansion/compile_messages.pl"
"$P/1_host_expand.pl $P/2_host_expand/1_host_expand.pl"
"$P/0_cst_query.pl $P/2_host_expand/0_cst_query.pl"
"$P/analyze.pl $P/3_analyze/analyze.pl"
"$P/0_rel_record.pl $P/3_analyze/0_rel_record.pl"
"$P/3_clock_check.pl $P/4_clock_check/3_clock_check.pl"
"$P/0_graph.pl $P/4_clock_check/0_graph.pl"
"$P/2_subscribe.pl $P/5_subscribe/2_subscribe.pl"
"$P/strat.pl $P/6_strat/strat.pl"
"$P/lower.pl $P/7_lower/lower.pl"
"$P/use_resolve.pl $P/7_lower/use_resolve.pl"
"$P/compile/parse_dl_dcg.pl $P/7_lower/parse_dl_dcg.pl"
"$P/executor_modules.pl $P/7_lower/executor_modules.pl"
"$P/compile/0_storage_projection.pl $P/7_lower/0_storage_projection.pl"
"$P/emit_ts.pl $P/8_emit_ts/emit_ts.pl"
"$P/compile/scripts/0_json_arrival.pl $P/9_json_arrival/0_json_arrival.pl"
"$P/diag.pl $P/10_diag/diag.pl"
)
STATE=$(mktemp -d); n=0
for m in "${moves[@]}"; do
  set -- $m; src=$1; dst=$2; n=$((n+1))
  if [ ! -f "$src" ] && [ -f "$dst" ]; then echo "MOVE $n already done"; continue; fi
  good=$(git rev-parse --short HEAD)
  out=$(timeout 60 extract move "$src" "$dst" --commit --state "$STATE" 2>&1); rc=$?
  reps=$(grep -c "^replace" <<<"$out")
  if [ $rc -ne 0 ]; then echo "MOVE $n FAIL rc=$rc $src: $(tail -3 <<<"$out")"; git reset -q --hard $good; git clean -qfd v6/prolog; exit 1; fi
  load=$(cd v6/prolog && { timeout 60 swipl -g halt -t halt -l compile.pl -l emit_rust.pl 2>&1; timeout 60 swipl -g halt -t halt -l compile.pl -l emit_ts.pl 2>&1; timeout 60 swipl -g halt -t halt -l print_dl.pl -l ARCH.pl 2>&1; } | grep -E "ERROR|Warning" | head -5)
  if [ -n "$load" ]; then echo "MOVE $n LOAD FAIL $src -> $dst (replaces=$reps)"; echo "$load"; git reset -q --hard $good; git clean -qfd v6/prolog; exit 1; fi
  [ -f "$dst" ] || { echo "MOVE $n NO FILE at $dst"; git reset -q --hard $good; git clean -qfd v6/prolog; exit 1; }
  git add -A v6/prolog >/dev/null && git commit -qm "rehome($n): $src -> $dst ($reps importers), by extract move" && echo "MOVE $n ok $src -> $dst replaces=$reps $(git rev-parse --short HEAD)"
done
echo "ALL $n MOVES OK"
