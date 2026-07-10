---
name: reference_scip_multilang_indexers
description: "SCIP indexer install + quirks for Go/Python/TS Tier-1 flow (scip-go moved repo, scip-python crashes in bare tmp); both emit method-level is_implementation -> scip_impl"
metadata: 
  node_type: memory
  type: reference
  originSessionId: 7322e02c-67ee-4fd7-8304-4c7ef80db5d0
---

Adding a language to the sprefa flow/dispatch surface = Tier 1 (SCIP, compiler-backed)
is nearly free; Tier 2 (native TypeLang extractor in typegraph.rs) is the Kotlin-sized
lift. For OpenAPI dispatch specifically Tier 1 is the whole job — Go interface
satisfaction is structural and only the index carries it. See [[project_v5_dl_engine]],
[[reference_scip_name_not_dl_split]].

Proven on real tools 2026-06-29 (commit b50174b). `bench/flow/dispatch_flow.dl` is
language-blind: reads only openapi.yaml + scip_* relations, runs unchanged over
Kotlin/Go/Python indexes. multi_op = function on the call-graph path of >=2 ops,
crossing the interface->impl hop.

`scip_impl(impl, iface)` (src/scip_import.rs) = SCIP `SymbolInformation.relationships`
is_implementation. BOTH scip-go and scip-python emit it METHOD-level, not just
type-level: `petClient#getPet().` -> `PetAPI#getPet.` (Go), `PetClient#getPet().` ->
`PetAPI#getPet().` (Py). That hop is invisible to occurrence/ref graphs.

Indexer install/run gotchas (toolchains installed in ~/go/bin and global npm here):
- scip-go: repo MOVED. `go install github.com/scip-code/scip-go/cmd/scip-go@latest`
  (the old github.com/sourcegraph/scip-go path 404s the go.mod). Run `scip-go --output
  index.scip` in the module dir. Copying fixture to /tmp works fine.
- scip-python: `npm i -g @sourcegraph/scip-python`; `scip-python index . --project-name
  X --output <idx>`. CRASHES in a bare /tmp copy (`TypeError: indexOf of undefined`,
  indexer.ts:252) — it walks PARENT dirs for python/env config. Fix in tests: index
  IN PLACE (current_dir = in-repo fixture), write index to a temp path, feed dl via
  SPREFA_SCIP_INDEX env, keep --db in temp. flow_py_dispatch.rs does this; flow_go_dispatch
  copies to tmp.

Loader fix the real Python monikers exposed (hand-built index hid it): scip-python emits
parameter symbols `getPet().(id)` that contain '(' but end ')' not ').'. Old fn-def
interval index keyed on contains('(') -> mis-registered params as enclosing fns ->
body call attributed to nearest param, dead-ending dispatch. `is_callable_def` now keys
on the `).` terminal descriptor (every indexer ends callables that way). RA/scip-go inline
params so only Py was bitten.

Residual: multi_op also surfaces the receiver TYPE (petClient#/PetAPI#) since both impls
reference it — true shared node, just not a function. Filter on `().` if you want fns only.
