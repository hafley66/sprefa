---
name: project_scip_turnkey_xrepo
description: dl index/doctor turnkey SCIP + explosion guard (landed); cross-repo dataflow + kwargs-as-language-level (design; built form became flow-services.dl, no xrepo_link rel exists)
metadata: 
  node_type: memory
  type: project
  originSessionId: 654d463f-1b2c-4b37-911f-766a0aa07344
---

**Landed on main + pushed 2026-07-01** (e99909b feat + c471788 docs): `dl index` + `dl doctor`
subcommands in `v5/src/scip_setup.rs` (pre-clap intercept like setup/examples).
`dl index` = detect lang by marker file → run indexer (rust-analyzer/scip-
typescript/scip-python/scip-go/scip-java/scip-clang) with cwd=root → place
`<root>/.dl/index.scip` (gitignored) → confirm scip_* row counts. Polyglot merge
via new `scip_import::merge_files` (unions N SCIP indexes; symbols are tool/pkg-
namespaced so no collision). `index_path` now auto-loads `.dl/index.scip`.
`dl doctor` = langs/indexer-installed/freshness(mtime vs HEAD)/path-join/row
counts. `--install` runs install cmd, `--rev` prints worktree-and-index recipe
(SCIP = working-tree only). CHANGELOG.md created (v5/).

**EXPLOSION GUARD** (the 500-repo-daemon concern): generation is EXPLICIT +
SINGLE-ROOT. Nothing auto-generates (daemon/reload-gate/`scan("*")` never do — the
daemon only IMPORTS). `dl index` refuses the XDG serving home OR a dir of nested
git repos (`nested_repos()` = one-level `.git` scan) unless `--force`. Lazy =
load-if-present, never generate-on-demand.

**kwargs = should be language-level, currently is NOT.** Named/`_`-elided args
exist only as OUTPUT sugar on match/ast_yaml/sg/comment (`parse_kwarg_terms` +
`assign_outputs`, parse.rs:788/817; unknown name = parse error = the typo guard
`diag` columns still lack). `scan` is hard-positional (= papercut #1). Universal
version = the decided [[project_cons_calling_unification]] Route A (one call
parser behind every surface); grammar change, not a bolt-on.

**Cross-repo dataflow vision** (user, API1→API2 / API→UI routes / SQL→query
tuning): rides existing primitives. merge_files = multi-index; [[project_interproc_flow]]
flow_edge/closure = intra-repo flow; the bridge = `xrepo_link(sym_a,sym_b,kind∈
http|sql|event)` seeded from a transport spec (OpenAPI source op / json extract),
unioned into flow_edge so closure crosses the network boundary. REALIZED FORM
(verified 2026-07-03): examples/flow-services.dl — spec-seeded service_op +
name-keyed wire hops straight into flow_edge; NO xrepo_link rel was ever built
and none is needed for the OpenAPI case (assert service_op facts for spec-less
transports); LSP notes = an
annotation rel through the existing diag/--lsp renderer (diag is already "one rel,
two renderers"); SQL patterns = schema/migration extract → table/column rels.
Full design in chat_log/20260701.0.dl-scip-turnkey-xrepo-dataflow-kwargs.md.
Changelog automation = @changelog comment-pin + gen (op-table.dl shape,
[[feedback_never_edit_autogen_zones]]).
