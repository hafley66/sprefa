# CPG spec research report

Anchor: `plans/2026-08-16-joern-cpg-striking-distance.md`.
Scope: four questions, each claim cited. All CPG spec citations point at the
authoritative source, the schema DSL that generates the published spec
(`cpg.joern.io` renders this same schema). Local clones used for citation:

- `codepropertygraph` (commit bd34f99, 2026-07-22): the schema is under
  `schema/src/main/scala/io/shiftleft/codepropertygraph/schema/`.
- `flatgraph` (the proto generator, dep of codepropertygraph).
- `tree-sitter-graph` (commit b930fb5).
- grammar crates in the local cargo registry:
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/{tree-sitter-go-0.23.4,tree-sitter-kotlin-sg-0.4.1,tree-sitter-rust-0.24.2,tree-sitter-typescript-0.23.2}`.

## TOC

1. Joern CPG spec vocabulary: edge kinds and node kinds
2. tree-sitter-graph candidate analysis
3. CPG protobuf import feasibility
4. kind_role census for rust / go / kotlin / ts
5. What this changes in the anchor doc

---

## 1. Joern CPG spec vocabulary

The CPG schema is a Scala DSL (one file per layer) that generates both the
docs and the protobuf. Edge kinds are the complete set of `addEdgeType` calls.
There are 34 edge kinds, not the seven the anchor doc's table groups. Every
edge below is cited as `schema/.../<Layer>.scala:<line>` inside the
codepropertygraph clone.

### 1a. Edge kinds (all 34)

| Edge | one-line semantic | source |
|---|---|---|
| AST | parent to child in the syntax tree | Ast.scala:423 |
| CONDITION | control structure to the expression(s) holding its condition | Ast.scala:428 |
| TRUE_BODY | control structure to its true branch/body | Ast.scala:434 |
| FALSE_BODY | control structure to its false branch/body | Ast.scala:439 |
| DO_BODY | do-while control structure to its body | Ast.scala:445 |
| FOR_INIT | for-loop to its initialization expression(s) | Ast.scala:450 |
| FOR_UPDATE | for-loop to its update/step expression(s) | Ast.scala:457 |
| FOR_BODY | for-loop to its body | Ast.scala:463 |
| TRY_BODY | try control structure to its try body | Ast.scala:467 |
| CATCH_BODY | try control structure to its catch/handler bodies | Ast.scala:472 |
| FINALLY_BODY | try control structure to its finally body | Ast.scala:479 |
| JUMP_ARGUMENT | jump-like control structure to the node encoding its jump target | Ast.scala:486 |
| CFG | control flow from source to destination node | Cfg.scala:55 |
| DOMINATE | source node immediately dominates the destination | Dominators.scala:26 |
| POST_DOMINATE | source node immediately post-dominates the destination | Dominators.scala:33 |
| CDG | destination is control dependent on the source | Pdg.scala:38 |
| REACHING_DEF | a variable produced at the source reaches the destination without being reassigned on the way (VARIABLE property names it) | Pdg.scala:43 |
| CALL | call site (a CALL node) to the METHOD it invokes; optional, auto-created from METHOD_FULL_NAME on load | CallGraph.scala:142 |
| ARGUMENT | call site to its arguments, and RETURN to the expressions it returns | CallGraph.scala:155 |
| RECEIVER | call site to its receiver argument (the object assigned to `this`) | CallGraph.scala:165 |
| REF | source identifier denotes access to the destination node (e.g. identifier to a local) | Base.scala:162 |
| EVAL_TYPE | a node to its evaluation type | Shortcuts.scala:44 |
| CONTAINS | a node to the method that contains it | Shortcuts.scala:48 |
| PARAMETER_LINK | method input parameter to the corresponding output parameter | Shortcuts.scala:52 |
| CAPTURE | capturing of a variable into a closure | Hidden.scala:72 |
| CAPTURED_BY | a captured LOCAL to the corresponding CLOSURE_BINDING | Hidden.scala:88 |
| IMPORTS | imports to dependencies | Hidden.scala:266 |
| IS_CALL_FOR_IMPORT | a CALL in the AST to the IMPORT | Hidden.scala:270 |
| TAGGED_BY | nodes to the tags they are tagged by | Tags.scala:48 |
| BINDS | a TYPE_DECL with a BINDING node | Binding.scala:55 |
| BINDS_TO | type arguments to type parameters | Type.scala:165 |
| ALIAS_OF | alias relation between a type declaration and a type | Type.scala:174 |
| INHERITS_FROM | inheritance between a type declaration and a type | Type.scala:184 |
| SOURCE_FILE | a node to the node representing its source file | FileSystem.scala:84 |

### 1b. Node kinds relevant to statements and expressions

Cited as `schema/.../<Layer>.scala:<line>`.

| Node kind | semantic | source |
|---|---|---|
| METHOD | a procedure/function/method; the ENTRY node of the CFG | Method.scala:45, Cfg.scala:39 |
| METHOD_RETURN | the formal return parameter; the EXIT node of the CFG | Method.scala:116, Cfg.scala:39 |
| CALL | a (function/method/procedure) call, METHOD_FULL_NAME names the target | Ast.scala:734 |
| IDENTIFIER | an identifier referring to a variable by name | Ast.scala:155 |
| LITERAL | a literal such as an integer or string constant | Ast.scala:128 |
| CONTROL_STRUCTURE | a control structure or conditional/unconditional jump; its CONTROL_STRUCTURE_TYPE is one of BREAK/CONTINUE/DO/WHILE/FOR/GOTO/IF/ELSE/TRY/THROW/SWITCH/MATCH/YIELD/CATCH/FINALLY | Ast.scala:393, Ast.scala:326 |
| BLOCK | a compound statement | Ast.scala:111 |
| LOCAL | a local variable | Ast.scala:141 |
| FIELD_IDENTIFIER | the field accessed in a field access | Ast.scala:183 |
| RETURN | a return instruction (`return x`), distinct from METHOD_RETURN | Ast.scala:316 |
| JUMP_TARGET | a location specifically marked as a jump target | Ast.scala:273 |
| METHOD_REF | a reference to a method as it appears in an expression | Ast.scala:299 |
| TYPE_REF | a reference to a type/class | Ast.scala:310 |
| MODIFIER | a language-dependent modifier (static, private, ...) | Ast.scala:261 |
| UNKNOWN | catch-all for AST nodes with no suitable CPG kind | Ast.scala:411 |
| FILE | a source file; AST root | FileSystem.scala:95 |
| NAMESPACE | a namespace | Namespace.scala:50 |
| TYPE / TYPE_DECL / MEMBER | a type instance / a type declaration / a type member | Type.scala:153 / 78 / 139 |

The CFG construction model (relevant to the anchor doc): METHOD is the ENTRY
and METHOD_RETURN the EXIT (Cfg.scala:39-41); every expression, call
representation, and JUMP_TARGET is a CFG_NODE (Cfg.scala:31, the CFG edge at
Cfg.scala:55); the spec's CONTROL_STRUCTURE node carries the CONTROL_STRUCTURE_TYPE
that the frontend sets and that the control-flow layer reads to build CFG from
the AST "automatically" (Ast.scala:393).

---

## 2. tree-sitter-graph

### Facts

| attribute | value | source |
|---|---|---|
| what it is | a Rust library + CLI defining a DSL to build arbitrary graphs from tree-sitter parses | README.md:1-17 |
| runtime | Rust crate `tree-sitter-graph` v0.12.0; usable as a library (`tree-sitter-graph = "0.12"`) or a CLI (`cargo install --features cli tree-sitter-graph`) | Cargo.toml:2-10, README.md:22-31 |
| license | MIT OR Apache-2.0 | Cargo.toml:11, LICENSE-MIT, LICENSE-APACHE |
| tree-sitter dep | `tree-sitter = "0.24"` | Cargo.toml (tree-sitter line) |
| maintenance | last release v0.12.0 2024-12-12; last commit 2024-12-11; ~1-2 releases/yr, dormant since late 2024 | CHANGELOG.md:9, git log |
| lineage | successor of stack-graphs (GitHub code-navigation), same author (Douglas Creager) | Cargo.toml authors, LICENSE-MIT copyright |

### Can .tsg express a kind_role mapping?

Yes, directly. A `.tsg` file is a sequence of stanzas; each stanza is a
tree-sitter query pattern with captures plus a block of statements that create
graph nodes, edges, and attributes (`src/reference/mod.rs`, "High-level
structure"). A kind->role classification is exactly one stanza per control-flow
kind that tags a graph node, e.g.:

```
(if_expression) @cs { attr (@cs) kind_role = "branch" }
(return_expression) @cs { attr (@cs) kind_role = "exit" }
```

The DSL's `node`, `edge`, and `attr` statements and the query-capture
mechanism (`@capture`) are the machinery (`src/reference/mod.rs`, "Graph
nodes", "Edges", "Attributes", "Syntax nodes"). So the mapping is expressible,
and so is the full per-language CFG construction: the DSL is not limited to a
kind table, it can build the whole CFG edge set declaratively.

### Consume .tsg or borrow the idea?

Recommendation: borrow the idea, do not consume the crate. The build-vs-buy
analysis:

- The crate's natural output is a graph (nodes + edges + attributes), not a
  relational fact table. To feed our kind_role design we would run the .tsg to
  produce a graph, then extract the `kind_role` attributes back into rows. That
  is a round trip through a graph the design does not need.
- Dependency skew: tree-sitter-graph v0.12 pins `tree-sitter 0.24`
  (Cargo.toml), while `v6/sprefa-extract` is on `tree-sitter 0.25`
  (v6/sprefa-extract/Cargo.toml:57). Consuming it forces a version reconciliation.
- Maintenance state is dormant (last release 2024-12). The anchor doc already
  prices the whole CFG as a per-language hand-authored fact set; a .tsg file is
  the same hand-authoring with a different surface.
- What is worth stealing is the stanza-per-kind structure: one stanza per
  control-flow CST kind, which is the same granularity as one `kind_role` row.

---

## 3. CPG protobuf import

### Where the schema lives and how it is produced

The protobuf is NOT a committed source file. The schema is authored as a Scala
DSL in `codepropertygraph/schema/` (one file per layer, Q1). The `.proto` is
generated at build time:

- `schema/.../Protogen.scala:31` runs `new ProtoGen(builder.build).run(outputDir)`.
- `ProtoGen` is flatgraph's `domain-classes-generator/.../flatgraph/codegen/ProtoGen.scala`.
- The generated `cpg.proto` is bundled inside the `codepropertygraph-<VERSION>.jar`
  and used with `protoc` to produce language bindings
  (codepropertygraph README.md:75-85).

### Messages (from ProtoGen.scala)

The generated `cpg.proto` (package `cpg`, proto3) contains:

| message / enum | role | source |
|---|---|---|
| `CpgStruct` | top-level graph: `repeated Node node` + `repeated Edge edge` | ProtoGen.scala:103, 121, 145 |
| `CpgStruct.Node` | a node: `key` (int64), `type` (NodeType enum), `property` list | ProtoGen.scala:104-120 |
| `CpgStruct.Edge` | an edge: `src`, `dst` (int64), `type` (EdgeType enum), `property` | ProtoGen.scala:123-145 |
| `NodePropertyName` / `EdgePropertyName` | property-name enums | ProtoGen.scala:42-52 |
| `PropertyValue` | oneof of string/bool/int/long/float/double + list variants + `ContainedRefs` | ProtoGen.scala:56-72 |
| `ContainedRefs`, `StringList`..`DoubleList` | value payloads | ProtoGen.scala:74-101 |
| `CpgStruct.Node.NodeType` / `CpgStruct.Edge.EdgeType` | node-type and edge-type enums | ProtoGen.scala:108-111, 132-135 |
| `AdditionalNodeProperty` / `AdditionalEdgeProperty` | overlay property deltas | ProtoGen.scala:148-159 |
| `CpgOverlay` | a stacked diff of nodes+edges+property deltas | ProtoGen.scala:161-167 |
| `DiffGraph` (+ RemoveNode/RemoveNodeProperty/RemoveEdge/RemoveEdgeProperty) | incremental update stream | ProtoGen.scala:172-212 |
| enums for control structure types, languages, frameworks | typed constants | Protogen.scala:12-16 |

### Is the schema versioned?

No explicit schema version on the wire. proto3, package `cpg`, no version
field. Versioning is implicit, two layers:

- the artifact release (`codepropertygraph-<VERSION>.jar`, build.sbt:1-4);
- the integer `protoId` assigned to every node type, edge type, property, and
  constant (`ProtoIds.scala`, referenced as `.protoId(...)` throughout the
  schema files). The protoIds are the de-facto stability contract: an importer
  keyed on enum ids survives additions as long as ids stay stable.

### Subset that maps onto our families

CPG is a generic node table plus colored edge tables sharing an int key
(`CpgStruct.Node.key`, `CpgStruct.Edge.src/dst`). That is the same shape as the
anchor doc's node dictionary + colored edge tables. The mapping subset:

- nodes: METHOD, METHOD_RETURN, CALL, IDENTIFIER, LITERAL, LOCAL, BLOCK,
  CONTROL_STRUCTURE, RETURN, JUMP_TARGET, METHOD_REF, TYPE_REF,
  FIELD_IDENTIFIER, FILE, TYPE, TYPE_DECL, MEMBER (Q1b);
- edges: AST, CFG, CALL, ARGUMENT, RECEIVER, REF, REACHING_DEF, EVAL_TYPE,
  CONDITION, TRUE_BODY/FALSE_BODY/DO_BODY/FOR_INIT/FOR_UPDATE/FOR_BODY/
  TRY_BODY/CATCH_BODY/FINALLY_BODY/JUMP_ARGUMENT, CONTAINS, PARAMETER_LINK
  (Q1a).

### Feasibility verdict

FEASIBLE, with two costs the SCIP importer does not pay:

1. `cpg.proto` must be generated (it is not a committed source). The SCIP
   importer vendors a committed `scip.proto` into
   `v6/sprefa-extract/src/scip/scip_proto.rs`. For CPG we would pin a
   codepropertygraph release, run its generator (or check out a generated
   proto), and vendor the resulting `cpg.proto` the same way.
2. Node and edge types are enums of integer protoIds, not strings. The importer
   must translate each enum id to our kind names, so a generated enum-to-kind
   table is part of the vendored surface. The SCIP importer's types are named
   messages (`proto::Index::decode`, scip_decode.rs:26-32), which is cleaner.

Importer shape otherwise matches the SCIP pattern exactly: prost decode of a
`CpgStruct` into our flat types (`scip_decode.rs` decode half), private
generated-bindings module (`#[path = "scip/scip_proto.rs"]`, scip_decode.rs:20-22).

One scope note for the anchor doc's "import = coverage for langs never
hand-walked": a CPG import lands at the abstract tier (CALL, CONTROL_STRUCTURE,
BLOCK). It does not deliver the CST node kinds needed for the kind_role census
(Q4), which is a separate per-language source.

---

## 4. kind_role census

Rosters are the `type` list in each grammar crate's `node-types.json`. Grammar
versions come from `cargo tree -p sprefa-extract`:
tree-sitter-go 0.23.4, tree-sitter-kotlin-sg 0.4.1, tree-sitter-rust 0.24.2,
tree-sitter-typescript 0.23.2. Role is assigned by control-flow function, not
by the grammar's own naming.

### rust (tree-sitter-rust 0.24.2, src/node-types.json)

| kind name | role | note |
|---|---|---|
| `if_expression` | branch | |
| `match_expression` | branch | |
| `match_arm` | branch | branch target inside match |
| `else_clause` | branch | fallthrough arm of if/match |
| `loop_expression` | loop | |
| `while_expression` | loop | |
| `for_expression` | loop | |
| `break_expression` | jump | exits a loop |
| `continue_expression` | jump | continues a loop |
| `return_expression` | exit | exits the method |
| `yield_expression` | exit | returns a value from a generator |

### go (tree-sitter-go 0.23.4, src/node-types.json)

| kind name | role | note |
|---|---|---|
| `if_statement` | branch | |
| `expression_switch_statement` | branch | |
| `type_switch_statement` | branch | |
| `select_statement` | branch | |
| `expression_case` / `type_case` / `communication_case` / `default_case` | branch | branch targets inside switch/select |
| `for_statement` | loop | both `for_clause` and `range_clause` forms |
| `break_statement` | jump | |
| `continue_statement` | jump | |
| `goto_statement` | jump | |
| `fallthrough_statement` | jump | |
| `return_statement` | exit | |

### kotlin (tree-sitter-kotlin-sg 0.4.1, src/node-types.json)

| kind name | role | note |
|---|---|---|
| `if_expression` | branch | |
| `when_expression` | branch | |
| `when_entry` | branch | branch target inside when |
| `guard_condition` | branch | condition guard |
| `for_statement` | loop | |
| `while_statement` | loop | |
| `do_while_statement` | loop | |
| `jump_expression` | jump / exit | ONE kind for all of return/break/continue/throw (grammar.js:1119-1126); sub-role is set by the leading keyword token, not the kind name |

Kotlin is the outlier: `jump_expression` is a single CST node kind whose
`choice(seq("throw", ...), seq(choice("return", $._return_at), ...), "continue", $._continue_at, "break", $._break_at)`
(grammar.js:1119-1126) folds return (exit), throw (exit), break (jump), and
continue (jump) into one node. A kind_name->role table alone cannot split
jump from exit for kotlin; the importer must read the leading keyword token.

### ts (tree-sitter-typescript 0.23.2, typescript/src/node-types.json)

| kind name | role | note |
|---|---|---|
| `if_statement` | branch | |
| `switch_statement` | branch | |
| `switch_case` / `switch_default` | branch | branch targets inside switch |
| `ternary_expression` | branch | conditional expression |
| `else_clause` | branch | fallthrough arm |
| `for_statement` | loop | |
| `for_in_statement` | loop | |
| `while_statement` | loop | |
| `do_statement` | loop | |
| `break_statement` | jump | |
| `continue_statement` | jump | |
| `throw_statement` | exit | exits via exception |
| `return_statement` | exit | |
| `yield_expression` | exit | generator yield |

---

## 5. What this changes in the anchor doc

- **"Seven edge colors" understates the vocabulary.** The anchor doc's table
  (section 2) groups AST / CALL-ARGUMENT-REF / REACHING_DEF / EVAL_TYPE / CFG /
  CDG into seven colors, but the spec defines 34 edge kinds (Q1a). Fork #2
  ("adopt Joern's edge vocabulary as rel names") is a 34-name surface, not 7.
  The extra names relevant to a CFG/CDG build are the structured-body edges
  (CONDITION, TRUE_BODY, FALSE_BODY, DO_BODY, FOR_INIT, FOR_UPDATE, FOR_BODY,
  TRY_BODY, CATCH_BODY, FINALLY_BODY, JUMP_ARGUMENT) and DOMINATE /
  POST_DOMINATE.

- **Joern builds CFG from structured AST edges, not a next-sibling scan.** The
  anchor's generic rule `cfg_edge(A,B) :- cst_next_sibling(A,B), not jump`
  (section 4) is not how Joern does it. Joern marks every expression, call, and
  JUMP_TARGET as a CFG_NODE, uses METHOD/METHOD_RETURN as ENTRY/EXIT
  (Cfg.scala:39-41), and derives CFG from the CONTROL_STRUCTURE node's
  CONTROL_STRUCTURE_TYPE plus the body edges (Ast.scala:393). Both are workable
  for our four languages; the anchor rule is simpler and does not need the
  extra edges. This is a mechanism difference, not a feasibility break.

- **kind_role as a pure kind_name->role table fails for kotlin.** Q4 shows
  kotlin collapses all of return/break/continue/throw into one `jump_expression`
  kind (grammar.js:1119-1126); the jump/exit split needs the leading keyword
  token. The anchor's `kind_role("rust", "return_expression", "jump")`-style
  rows (section 4) work for rust/go/ts but require a token read for kotlin, or
  a coarser kotlin row that cannot separate jump from exit.

- **Fork #3 is settled toward hand-authored.** tree-sitter-graph can express
  the kind_role mapping and the full CFG as .tsg stanzas (Q2), but it is
  dormant (last release 2024-12), pinned to tree-sitter 0.24 against our 0.25,
  and emits a graph rather than a fact table. Borrow the stanza-per-kind idea;
  do not consume the crate.

- **Fork #2 with CPG protobuf import is feasible but adds a generation step.**
  The proto is generated from the Scala schema at build time, not committed,
  and node/edge types are integer protoIds (Q3). An importer shaped like
  `scip_decode.rs` is viable; the vendored surface must add a generated
  enum-to-kind table. Import covers the abstract tier, not the CST kinds that
  Q4 needs.
