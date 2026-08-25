#!/usr/bin/env bash
# One move per commit. Load check after each; stop and rewind on the first failure.
set -u
cd "$(git rev-parse --show-toplevel)"
P=v6/prolog
moves=(
# pass 1: parse front
"$P/use_resolve.pl $P/next/0_parse/use_resolve.pl"
"$P/compile/parse_dl_dcg.pl $P/next/0_parse/parse_dl_dcg.pl"
"$P/0_cst_query.pl $P/next/0_parse/0_cst_query.pl"
# pass 2: expanders
"$P/0_body_walk.pl $P/next/1_expand/0_body_walk.pl"
"$P/0_relation_pattern.pl $P/next/1_expand/0_relation_pattern.pl"
"$P/0_annotation_expand.pl $P/next/1_expand/0_annotation_expand.pl"
"$P/0_anonymous_expand.pl $P/next/1_expand/0_anonymous_expand.pl"
"$P/0_ast_expand.pl $P/next/1_expand/0_ast_expand.pl"
"$P/0_coalesce_expand.pl $P/next/1_expand/0_coalesce_expand.pl"
"$P/0_dot_expand.pl $P/next/1_expand/0_dot_expand.pl"
"$P/0_enum_expand.pl $P/next/1_expand/0_enum_expand.pl"
"$P/0_generic_expand.pl $P/next/1_expand/0_generic_expand.pl"
"$P/0_match_expand.pl $P/next/1_expand/0_match_expand.pl"
"$P/0_negated_guard_expand.pl $P/next/1_expand/0_negated_guard_expand.pl"
"$P/0_option_expand.pl $P/next/1_expand/0_option_expand.pl"
"$P/0_relation_edge_expand.pl $P/next/1_expand/0_relation_edge_expand.pl"
"$P/0_seq_expand.pl $P/next/1_expand/0_seq_expand.pl"
"$P/1_expansion.pl $P/next/1_expand/1_expansion.pl"
"$P/1_host_expand.pl $P/next/1_expand/1_host_expand.pl"
# pass 3: analysis + lowering
"$P/0_graph.pl $P/next/2_lower/0_graph.pl"
"$P/0_program_check.pl $P/next/2_lower/0_program_check.pl"
"$P/analyze.pl $P/next/2_lower/analyze.pl"
"$P/strat.pl $P/next/2_lower/strat.pl"
"$P/lower.pl $P/next/2_lower/lower.pl"
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
