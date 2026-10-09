# CST query and build: one pattern language over tree-sitter grammars

Status: design sketch for Chris, 2026-10-09. Nothing built. Language decisions below are forks,
not choices.

User, 2026-10-09: "query and mutate (read: create strings of programs) that are ast
checked/safe, like typespec+alloy but with my own lisp with a macro that allows actual cst like
ur lowering pure dl8 target idea, but we preserve how to read tree sitter's scm lisp since we
have same tokenizer ish".

## The one idea

A tree-sitter grammar becomes dl7 types. One scm-shaped pattern either matches CST rows (query)
or constructs a typed CST term (build). A built term renders to text through the grammar's own
token order, and the text is re-parsed to prove the round trip.

```
grammar.json ──macrotime──► dl7 types per node kind  (call_expression, arguments, ...)
                                   │
        scm pattern ───────────────┤
        (call_expression function: ?Fn arguments: ?Args)
              │                    │
   query: lowers to joins     build: lowers to a constructor term,
   over CST rows               checked against the generated types
              │                    │
   ryi --kinds cst rows       cst.render: grammar token order → doc.Doc
                                   │
                              fs.file → formatter → re-parse == term
```

## Type signatures

```lisp
; generated once per grammar version, from grammar.json
(: rust.call_expression
   (* (: function rust.expression)
      (: arguments rust.arguments)))

(: rust.expression
   (+ (: identifier rust.identifier)
      (: scoped_identifier rust.scoped_identifier)
      (: call_expression rust.call_expression)))

; the CST rows ryi already emits, read as products
(: cst       (* (: node int) (: kind str) (: start int) (: end int)))
(: cst_field (* (: parent int) (: field str) (: order int) (: child int)))

; render: a typed node to a document, by grammar.json token order
(: cst.render
   (* (: node any)
      (: return doc.Doc)))
```

TypeScript twin:

```ts
type CallExpression = { kind: "call_expression"; function: Expression; arguments: Arguments };
type Expression = Identifier | ScopedIdentifier | CallExpression;
declare function render(node: CallExpression | Expression): Doc;
```

## The macro: scm syntax in both positions

```lisp
; query: in a rule body, the pattern binds captures
(<- (git_spawn ?CallNode ?CommandText)
    (scm rust
       (call_expression
          function: (scoped_identifier) @constructor
          arguments: (arguments (string_literal) @command))
       (#match? @constructor "Command::new$")
       (#match? @command "git"))
    (capture @command ?CommandText))

; build: in a rule head, the pattern constructs a typed term
(<- (fs.file "spawn.rs"
       (cst.render
          (scm rust
             (call_expression
                function: (scoped_identifier "Command::new")
                arguments: (arguments (string_literal ?ProgramName))))))
    (wanted_program ?ProgramName))
```

TypeScript twin:

```ts
// query
const hits = match(rust, `(call_expression function: (scoped_identifier) @constructor ...)`);
// build
const file = render(callExpression({ function: scopedIdentifier("Command::new"),
                                     arguments: args([stringLiteral(programName)]) }));
```

## Safety, in order

1. Build: a child whose kind the field does not admit is a check-phase type error, not a parse
   failure later (the generated sum types carry the grammar's CHOICE).
2. Render: tokens come from grammar.json STRING members, so punctuation is never hand-written.
3. Round trip: the rendered text is parsed by tree-sitter and compared to the term; a mismatch
   is a diagnostic naming the node. This catches precedence and the grammar's hidden rules.

## Lifetimes

- Generated grammar types: one per grammar version, written at macrotime, cached by the
  grammar.json digest.
- CST rows: one stream per extract run (ryi), keyed by the run's scope id.
- Built terms: comptime Universe terms, hash-consed; they live until the build sink renders them.
- Rendered text: written once by `fs.file`, after the comptime fixpoint.

## Storage and reads

- Query reads `cst` and `cst_field` rows: one join per pattern edge, keyed by (parent, field).
- Build writes no rows; the term is the value. The render walk reads grammar.json once per kind.
- Uniqueness: a CST node is (run scope id, node id); a built term is its hash-consed id.

## Forks for Chris

| fork | options |
|---|---|
| A: reader | does the dl7 reader take scm tokens as-is, or does `scm` read a string. `dl8 read` today: `field:` reads; `@constructor` and `#match?` stop with `invalid_atom` (`src/_0_read/_2_reader.rs:668`) |
| B: one pattern, two positions | `scm` in a body queries and in a head builds, or two macros |
| C: render | own grammar.json walk into `doc.Doc`, or ast-grep rewrite templates |
| D: layout | language formatter post-pass (rustfmt, biome), or `doc.Doc` group/nest from grammar hints, or topiary |
| E: type source | generated from grammar.json, or from tree-sitter `node-types.json` (fields with their allowed kinds already listed) |

## Build vs buy, to fill before code

| candidate | covers | to check |
|---|---|---|
| tree-sitter `node-types.json` | per kind: fields, allowed child kinds, required/multiple | ships with every grammar; the type source for E |
| `ast-grep-core` | match with metavariables, rewrite by source template | licence, whether its pattern AST is reachable |
| `tree-sitter-graph` | scm patterns to graph rows | overlap with the query side |
| `topiary` | query-driven layout | fork D |
| TypeSpec + Alloy | the reference design for typed builders and render | shape to copy, not a dependency |
