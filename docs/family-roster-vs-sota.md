# The extract family roster against the field (2026-09-01)

Survey only. No design proposal, no implementation. Every SOTA row carries the
doc or schema it was read from; every sprefa row carries a `path:line` under
`v6/sprefa-extract/`.

Two questions asked, both answered here:

1. Is there one shared row format all eight families could share?
2. What relation families does the state of the art publish that this roster
   does not?

## Contents

1. [The roster today](#the-roster-today)
2. [One shared format: two already exist](#one-shared-format-two-already-exist)
3. [Why five families cannot use the bench format](#why-five-families-cannot-use-the-bench-format)
4. [System by system](#system-by-system)
5. [The 18 gaps](#the-18-gaps)
6. [Cheap: new edge kinds inside families we have](#cheap-new-edge-kinds-inside-families-we-have)
7. [Expensive: genuinely new families](#expensive-genuinely-new-families)
8. [Not an edge at all](#not-an-edge-at-all)
9. [Where the vector/RAG plane sits](#where-the-vectorrag-plane-sits)
10. [Sources](#sources)

## The roster today

`types.rs:88-97`, `enum FamilyTag`:

```
Df   Flow   Call   Type   Module   Cst   Cfg   Data
```

Edge kinds inside them, same file:

| family | edge kinds | count |
|---|---|---|
| `Cst` | Child | 1 |
| `Data` | Child | 1 |
| `Df` | Direct | 1 |
| `Flow` | ArgToParam, RetToCallRes, LambdaElem, LambdaRet | 4 |
| `Type` | Field, Variant, Impl, Generic, Param, Returns, Uses, DocRef | 8 |
| `Call` | NameResolve, ScipOverride, ValueRef, ImportResolve, Implements, ScipMacro, CheckerResolve | 7 |
| `Cfg` | Next, Arm, Jump, Exit | 4 |
| `Module` | collapsed, `types.rs:1437-1450` | 0 |

Two whole-project modes sit beside the per-file mask: `scip` decodes a real
indexer's `index.scip`, `diet_scip` is this crate's own front ends plus name
matching (`family.rs:6-29`).

## One shared format: two already exist

| altitude | shape | carries | lossy |
|---|---|---|---|
| engine, `types.rs:1585` `ProjectEdge<F>` and `types.rs:1141` `FlowEdge` | `(blob, span) -> (blob, span) + kind` | all eight | no |
| bench tsv, `plans/extract-bench-2026-08-29/COMMON.md` | `src_path  src_name  dst_path  dst_name` | call, module, type | drops kind and span |

The bench format is a lossy projection of the engine's own edge shape: span
collapsed to a name, edge kind dropped entirely. `rust.oracle.type.typedecl.tsv`
mixes `Field`, `Impl` and `Uses` into one undifferentiated file for that reason.

The widening that carries all eight is the engine shape written out:

```
src_path  src_start  src_end  dst_path  dst_start  dst_end  kind
```

It is a superset as a relation, NOT as a score. `score` (`tests/bench/mod.rs:821`)
is a set intersection over `BTreeSet<String>` and `load_tsv` (`:777`) dedups the
file on load, so multiplicity is already discarded today:

| oracle file | lines | set elements | collapsed |
|---|---|---|---|
| `ts5.oracle.call.tsv` | 84,958 | 59,356 | 25,602 (30.1%) |
| every other `*.call.tsv` in that dir | - | - | 0 |

The 18 floors are preserved ONLY if scoring continues to project to the 4-tuple
and dedup before intersecting. Scoring in the widened space changes both
denominators and moves every `ts5` call row immediately. The projection is
therefore part of the format contract, named per case beside `Projection::Raw`,
never left implicit.

## Why five families cannot use the bench format

The bench format is name-addressed. Node identity in the other five is a span.

| family | src -> dst under the 4-column form | verdict |
|---|---|---|
| `call` | `A.rs:foo -> B.rs:bar` | FITS |
| `module` | `A.rs:'' -> B.rs:''`, path is the id | FITS |
| `type` | `A.rs:Foo -> B.rs:Bar` | FITS |
| `cst` | one file, edge kind `Child`, sibling nodes share a name | BREAKS |
| `data` | same shape, edge kind `Child` | BREAKS |
| `df` | node id is a span, `Direct` between value nodes | BREAKS |
| `cfg` | basic blocks carry no name | BREAKS |
| `flow` | an argument position has an index, not a name | BREAKS |

## System by system

| system | relation vocabulary it publishes | sprefa family covering it | verdict |
|---|---|---|---|
| SCIP | `SymbolRole`: Definition, Import, WriteAccess, ReadAccess, Generated, Test, ForwardDefinition. `Relationship`: is_reference, is_implementation, is_type_definition, is_definition. `SymbolInformation`: kind (~87), documentation, signature_documentation, enclosing_symbol. `Package{manager,name,version}`. `SyntaxKind` (48) | `Call::NameResolve`, `ScipOverride`, `Implements`, `Type::DocRef`, `Cst` | PARTIAL: no read-vs-write role, no Generated/Test role, no package version node |
| LSIF 0.6.0 | vertices: document, resultSet, range, hoverResult, definitionResult, declarationResult, referenceResult, implementationResult, typeDefinitionResult, foldingRangeResult, documentLinkResult, documentSymbolResult, diagnosticResult, semanticTokensResult, moniker, packageInformation, project. edges: `textDocument/*`, contains, next, item, moniker, attach, packageInformation | `Call::NameResolve`, `Type::Impl`, `Cst` | PARTIAL: no hover/doc result, no diagnostic, no moniker/package |
| Kythe | ~45 edges: childof, defines, defines/binding, defines/implicit, ref, ref/call, ref/call/direct, ref/call/implicit, ref/doc, ref/expands, ref/expands/transitive, ref/file, ref/imports, ref/init, ref/writes, property/reads, property/writes, typed, param, tparam, extends, overrides, overrides/root, overrides/transitive, satisfies, specializes, instantiates, bounded/upper, bounded/lower, aliases, aliases/root, annotatedby, denotes, depends, documents, exports, generates, influences, imputes, named, narrows, tagged, undefines, completedby | `Call::*`, `Type::*`, `Cst::Child` | GAP: widest vocabulary of the ten, ~20 edge kinds unmatched |
| Glean `codemarkup.31` | EntityKind, EntityInfo, EntityLocation, EntityUses, EntityReferences, EntitySource, FileCall, EntityToAnnotations, EntityVisibility, EntityModifiers, EntityIsDefinition, ExtendsParentEntity, ExtendsChildEntity, ContainsParentEntity, ContainsChildEntity, SearchInheritedEntities, SearchRelatedEntities, EntityComments, EntityModuleName, GeneratedEntityToIdlEntity, FileEntityDigest | `Call`, `Type`, `Module` (collapsed) | GAP: visibility, modifiers, comments, inherited-member search, generated-to-IDL |
| CodeQL | `Stmt`/`Expr`, `Callable`/`Call`/`Callable.getAReference()`, `Type`/`RefType` hierarchy, `MetricCallable`, DataFlow local vs global, TaintTracking, `PointsTo.qll` / `pointstoinfo` (Steensgaard, field-sensitive) | `Call`, `Type`, `Df::Direct`, `Flow::*` | GAP: taint, points-to, metrics |
| Joern / CPG | edges: AST, CONDITION, CFG, DOMINATE, POST_DOMINATE, CDG, REACHING_DEF, CALL, ARGUMENT, RECEIVER, REF, EVAL_TYPE, INHERITS_FROM, BINDS, BINDS_TO, ALIAS_OF, CONTAINS, PARAMETER_LINK, TAGGED_BY, SOURCE_FILE. nodes include FINDING, TAG, ANNOTATION, MODIFIER | `Cst`, `Cfg`, `Call`, `Df`, `Flow` | GAP: CDG, DOMINATE, POST_DOMINATE, REACHING_DEF, ALIAS_OF, TAGGED_BY |
| stack-graphs | name binding only: a reference to all possible definitions, symbol stack plus scope stack. Dataflow and type inference are named as open questions | `Call::NameResolve` | COVERED |
| Semgrep / ast-grep | semgrep taint mode: `pattern-sources`, `pattern-propagators`, `pattern-sanitizers`, `pattern-sinks`, `by-side-effect`. ast-grep: syntactic tree-sitter matching only | `Cst` for ast-grep, nothing for taint | GAP: source/sink/sanitizer/propagator roles |
| Sourcegraph, GitHub code nav | go to definition, find references, find implementations, precise vs search-based. Sourcegraph Own: CODEOWNERS ingestion plus inferred signals from commits, team structure, PR review activity | `Call::NameResolve`, `Call::Implements` | GAP: ownership plane absent |
| Embedding / RAG | "context items", ranked by similarity threshold, positions of ranked items not used | none | NOT A FAMILY, see below |

## The 18 gaps

| # | gap | published by | lands as |
|---|---|---|---|
| 1 | control dependence, dominance, reaching-def | CPG `CDG`, `DOMINATE`, `POST_DOMINATE`, `REACHING_DEF` | edge kinds in `Cfg` |
| 2 | exception flow | Soot `ExceptionalUnitGraph` | edge kind in `Cfg` |
| 3 | inheritance closure | Kythe `overrides/root`, `overrides/transitive`, `satisfies`, `narrows`; Glean `SearchInheritedEntities` | edge kinds in `Type` |
| 4 | generics instantiation | Kythe `tapp`, `tparam`, `tvar`, `instantiates`, `specializes`, `bounded/*` | edge kinds in `Type` |
| 5 | annotations, modifiers, visibility | CPG `TAGGED_BY`; Kythe `annotatedby`, `tagged`; Glean `EntityVisibility`, `EntityModifiers` | edge kind in `Type` plus Aux |
| 6 | read vs write access | SCIP `ReadAccess`/`WriteAccess`; Kythe `ref/writes`, `property/reads`, `property/writes` | edge kinds in `Call` |
| 7 | macro expansion tree | Kythe `ref/expands`, `ref/expands/transitive`, node kind `macro` | edge kinds in `Call` |
| 8 | points-to / alias | CodeQL `PointsTo.qll`; CPG `ALIAS_OF`; Kythe `aliases`, `aliases/root` | NEW family |
| 9 | taint / security dataflow | Semgrep taint mode; CodeQL TaintTracking | NEW family |
| 10 | generated code and IDL source | Kythe `generates`; Glean `GeneratedEntityToIdlEntity`; SCIP role `Generated` | NEW family |
| 11 | package graph at version resolution | SCIP `Package{manager,name,version}`; LSIF `packageInformation` + `moniker` + `attach`; Kythe `depends` | NEW family, revive `ModuleF` |
| 12 | build-target graph | Bazel query `deps`, `rdeps`, `allpaths`, `somepath`, `kind`, `attr`, `buildfiles`, `rbuildfiles`, `siblings`, `tests` | NEW family |
| 13 | ownership / blame | Sourcegraph Own, CODEOWNERS; Kythe node kind `vcs` | NEW family, VCS plane |
| 14 | diagnostics / findings | LSIF `diagnosticResult`; SCIP `Occurrence.diagnostics`; Kythe node kind `diagnostic`; CPG `FINDING` | NEW family or Aux |
| 15 | influence / impact | Kythe `influences`, `imputes`, `completedby`, `undefines` | NEW family |
| 16 | documentation text | Kythe `documents`, `ref/doc`, node kind `doc`; Glean `EntityComments`; SCIP `documentation`; LSIF `hoverResult` | Aux payload, not an edge |
| 17 | test-to-code | SCIP `SymbolRole` bit `Test`; Bazel query `tests` | node role, not an edge |
| 18 | metrics | CodeQL `MetricCallable`, `getMetrics()` | Aux, never a family |

## Cheap: new edge kinds inside families we have

Seven of the eighteen need no new family. Each connects two nodes the roster
already carries.

| gap | family | why the existing family holds it |
|---|---|---|
| control dependence, dominance, reaching-def | `Cfg` | CPG puts CDG, DOMINATE, POST_DOMINATE and REACHING_DEF over the same `CFG_NODE` set the CFG edges use. `Cfg` today is Next, Arm, Jump, Exit only |
| exception flow | `Cfg` | Soot defines it as extra edges in the same unit graph, throw unit to handler unit |
| inheritance closure | `Type` | `overrides/root`, `overrides/transitive`, `satisfies`, `narrows` all connect two declared entities, as `Type::Impl` does. `Implements` is one direction, one hop, and does not split satisfies from extends |
| generics instantiation | `Type` | `tapp`, `tparam`, `instantiates`, `specializes`, `bounded/*` all relate declared types. `Type::Generic` is one kind against eight |
| annotations, modifiers | `Type` | Kythe uses an edge (`annotatedby`), Glean uses attribute predicates; the annotation target is a declared entity |
| read vs write | `Call` | SCIP splits one occurrence's role bits, Kythe splits `ref` into `ref/writes` and `property/reads`. `Call::ValueRef` is undifferentiated |
| macro expansion | `Call` | `ref/expands` and `ref/expands/transitive` join the same anchor-to-node shape `Call::ScipMacro` uses. `ScipMacro` is one flat edge with no expansion tree and no transitivity |

Gaps 1 and 2 land inside `Cfg`, whose rows the current bench format cannot
express at all. The widened span format above is a prerequisite for measuring
them.

## Expensive: genuinely new families

| gap | why it cannot be an edge kind |
|---|---|
| points-to / alias | nodes are abstract memory objects, not spans. CodeQL collapses pointers into equivalence classes, which no span-pair edge expresses. sprefa's `AliasChain` is a `ResolutionOrigin` (`types.rs:1534`), not an edge |
| taint | sources, sinks, sanitizers and propagators are node ROLES plus a non-value-preserving step relation, distinct from `Df::Direct` |
| generated code and IDL | `generates` and `GeneratedEntityToIdlEntity` cross the file-and-language boundary `CallF` resolution assumes. This is also the cross-language and FFI link the field actually ships |
| package graph at version | `Package{manager,name,version}` is a node with its own identity. `package_edge` already exists on the wire with `ModuleF` collapsed (`types.rs:1437-1450`) |
| build-target graph | nodes are BUILD targets, not spans. Bazel states the graph is defined by rule declarations in BUILD files |
| ownership / blame | inputs are commits, teams and CODEOWNERS files, not source spans |
| diagnostics / findings | LSIF and Kythe both give it its own vertex or node kind, not an edge. sprefa decodes SCIP diagnostics at `types.rs:2259-2275` with no family |
| influence / imputes | Kythe scopes it to build-and-effect reasoning across units, with no span-pair analogue |

## Not an edge at all

| gap | shape the field uses |
|---|---|
| documentation text | SCIP puts it on `SymbolInformation`, Kythe uses a `doc` NODE. `Type::DocRef` points AT a type from a doc, it does not carry doc text as a relation |
| test-to-code | SCIP spells it as a `SymbolRole` bit on the symbol. No system in this set publishes a test-covers-code edge |
| metrics | CodeQL exposes it as delegate classes over existing AST classes, a per-node attribute plane |

## Where the vector/RAG plane sits

A different index, not a family.

| claim | source |
|---|---|
| its unit is a "context item", not a typed node with typed edges | Cody context paper, arXiv 2408.05344 |
| it publishes no edge vocabulary; ranking is threshold-based over a flat list, "we do not take into account the positions of the ranked items" | same paper |
| it CONSUMES families rather than being one; "code graph analysis" is listed as one retrieval technique alongside keyword and embedding search | same paper |
| the vendor direction is away from a standalone vector index | Sourcegraph removed embeddings from Cody's retrieval stack in favour of its search platform, Feb 2024 |

The eight families are the graph plane. A vector index is a second index over
the same bytes with no relation vocabulary to compare against.

## Sources

- SCIP: `https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto`
- LSIF 0.6.0: `https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/`
- Kythe schema: `https://kythe.io/docs/schema/`
- Glean `codemarkup.angle`: `https://github.com/facebookincubator/Glean/blob/main/glean/schema/source/codemarkup.angle`
- CodeQL data flow: `https://codeql.github.com/docs/writing-codeql-queries/about-data-flow-analysis/`
- CodeQL Java library: `https://codeql.github.com/docs/codeql-language-guides/codeql-library-for-java/`
- CodeQL `PointsTo.qll`: `https://codeql.github.com/codeql-standard-libraries/cpp/semmle/code/cpp/pointsto/PointsTo.qll/module.PointsTo.html`
- CPG spec: `https://cpg.joern.io/`
- stack-graphs: `https://docs.rs/stack-graphs/latest/stack_graphs/` and `https://github.blog/open-source/introducing-stack-graphs/`
- Semgrep taint mode: `https://docs.semgrep.dev/writing-rules/data-flow/taint-mode`
- ast-grep: `https://ast-grep.github.io/guide/introduction.html`
- Sourcegraph code navigation: `https://sourcegraph.com/docs/code-search/code-navigation/features`
- Sourcegraph Own: `https://sourcegraph.com/docs/own`
- GitHub code navigation: `https://docs.github.com/en/repositories/working-with-files/using-files/navigating-code-on-github`
- Bazel query language: `https://bazel.build/query/language`
- Soot `ExceptionalUnitGraph`: `https://soot-oss.github.io/soot/docs/4.3.0/jdoc/soot/toolkits/graph/ExceptionalUnitGraph.html`
- Cody context retrieval: `https://arxiv.org/html/2408.05344v1`
- Cody embeddings removal: `https://sourcegraph.com/blog/how-cody-understands-your-codebase`

sprefa side read from `v6/sprefa-extract/src/types.rs:88-97, 185, 251, 457,
1078, 1107, 1141, 1290, 1416, 1437-1450, 1534, 1585, 2259-2275`,
`src/family.rs:6-29`, `tests/bench/mod.rs:162-194`,
`plans/extract-bench-2026-08-29/COMMON.md`.
