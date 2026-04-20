# Lowering pipeline (v3 direction)

Source: chat 2026-04-18. Author goal: master parsing/tree-sitter/LSP/
grammar before producing v3, with pleasant op authoring as the design
constraint.

The PHP analogy is the clearest framing. Sprf interpolations
(`${X}` / `&{...}`) play the role of `<?= $x ?>`. Op bodies play the
role of HTML. Sub-grammars (walker, markdown, ast-grep, json) play the
role of the HTML parser. The host owns interpolations as nodes and
hands the sub-grammar a discontinuous byte range so the sub-grammar
parses the rest as if interpolations are not there.

## The five-stage pipeline

```
1. host-parse   tree-sitter-sprefa over the whole file
                produces: pipeline tree, op_call nodes, terms,
                addresses, comments

2. body-extract for each op_call:
                read body byte range
                find ${X} / &{...} sites inside that range -> HostInterps

3. body-inject  for each op_call:
                parser.set_language(op.body_grammar())
                parser.set_included_ranges(body_range minus HostInterps)
                parser.parse(SRC, None)  ->  InnerTree
                walk InnerTree for further injection sites; recurse

4. lower        for each op_call:
                op.parse(invocation { host_tree, inner_tree, host_interps })
                produces: runtime artifact (ast-grep Rule,
                WalkerPattern, etc.) with bind-sites pointing at
                HostInterps

5. run          for each cursor:
                resolve bindings (term lattice, Tri stamping)
                substitute HostInterps with bound values
                execute the op's runtime artifact
```

Stages 1-3 are language-agnostic; the same loop runs for every op.
Stage 4 is op-specific (the only place op-author code runs in the
lowering chain). Stage 5 is runtime, separate phase.

Validated by `v3-parse-experiment/src/bin/recurse.rs` — fixed-point
loop drives stages 2-3 at arbitrary depth with one decider closure.

## Where each concern lives

- **Host grammar (`tree-sitter-sprefa`)** owns: pipeline shape,
  op_call boundaries, term sigils (`${X}` / `&{...}` / etc), comments,
  the addressing language. Knows nothing about op semantics.
- **Sub-grammars** own: walker DSL shape, markdown shape, ast-grep
  pattern shape. Know nothing about the host or about interpolation.
- **Op `parse()`** owns: the bridge. Reads host's term_ref nodes +
  sub-tree nodes, produces a runtime artifact. This is where the
  PHP-style "fill in the holes at run time" plan is encoded.

## Two interpolation cases

| Where the interp sits        | What sub-grammar needs to know | Mechanism                          |
| ---------------------------- | ------------------------------ | ---------------------------------- |
| Syntactic value position     | Nothing                        | `set_included_ranges` skips holes  |
| Mid-token (mid-ident, etc.)  | Placeholder rule               | Sub-grammar declares e.g. `expression_placeholder` matching the gap |

Sprf's bracing rule (term-language doc) keeps every interp in a value
position, so the first row covers planned grammars. Sub-grammars stay
clean of host syntax.

## Op authoring shape [tentative]

The design constraint is "pleasant op authoring." A sketch of what
this looks like in v3:

```rust
impl Operator for WalkerOp {
    fn brace_grammar(&self) -> &'static LanguageFn {
        &tree_sitter_sprefa_walker::LANGUAGE
    }

    fn parse(&self, inv: &OpInvocation, _pctx: &mut ProgramCtx)
        -> Result<Pipeline, Vec<Box<dyn Diagnostic>>>
    {
        // inv.brace_tree       = walker tree (sub-grammar)
        // inv.brace_interps    = host's term_ref nodes inside the body
        // inv.brace_byte_range = original body span

        let pattern = WalkerPattern::compile(
            inv.brace_tree.root_node(),
            &inv.brace_interps,
            &SRC[inv.brace_byte_range],
        )?;

        Ok(Pipeline::Op(LoweredOp::bare(Arc::new(WalkerInstance { pattern }))))
    }
}
```

Every op consumes `brace_tree` + `brace_interps` + (optional)
`paren_tree` + `bracket_tree` and produces a runtime artifact with
bind holes. The walker is no different from `ast[rs]`, `md`, `marker`,
`json`.

## What "pleasant" means concretely (reader inference)

These are the checks that decide whether the abstraction holds:

- One `grammar.js` per op-specific sub-language, or zero (reuse
  existing tree-sitter grammars).
- One `parse()` method per op, takes structured input (no byte
  hunting).
- One `pipe()` method per op, takes cursors, emits cursors.
- Diagnostics emitted via `OpCtx.diags` (already a v2 invariant).
- Hover dispatch by node kind from the relevant tree (no byte probing).
- Capture binding declared at compile time via the inner tree's
  structure + the interp positions — runtime substitution flows for
  free.
- Adding a new op = write grammar.js (or pick existing) + write
  `parse()` + write `pipe()`. No host parser changes.

If any of these grow past one method or require host parser tweaks,
the abstraction is leaking and needs revisiting before v3 ships.

## Open lowering questions [tentative]

These are the bits the experiments have not yet covered. Land each
before declaring v3 lowering done.

- **OpInvocation shape**: today (`v2/src/_5_op.rs`) it carries raw
  byte ranges via `BracketSlot` / `ParenSlot` / `BraceSlot`. v3 needs
  it to carry parsed `Tree`s plus host_interps lists. Migration is
  trait-shape-changing for every op.
- **Recursive injection ownership**: who walks the inner tree for
  further injection sites? Framework or op? Framework keeps op authors
  out of the recursion; op-driven keeps each op self-contained. Author
  preference not stated.
- **Sub-grammar diagnostic rebasing**: dissolved.
  `set_included_ranges` makes inner-tree ERROR/MISSING node positions
  carry original source coordinates automatically. Op reads inner
  errors and emits diagnostics directly; no rebasing layer needed.
- **Interp position validity per sub-grammar**: each sub-grammar
  declares which node kinds accept interps. The walker grammar
  accepts interps in dict-value position; ast-grep pattern grammar
  accepts them in metavar position; etc. This validation belongs in
  the body-extract stage so an interp in a forbidden position errors
  before the sub-grammar even runs.
- **Cache key for parsed inner trees**: tree-sitter `Tree::edit` works
  on the host tree, but each inner tree was parsed separately. When
  the host edits inside a body, only that inner tree needs to
  reparse. Cache key: `(op_call_node_id, body_grammar_id)`. Survives
  reparse iff the host's node-id mapping survives.
- **Interp resolution timing**: implementation detail, not a design
  call. Lower-time when the bind target is statically resolvable;
  run-time otherwise. The runtime decides per ref. Punted as a
  design question.

## What lands first

The author has stated the order: study the components first
(parsing, tree-sitter, LSP, grammar), then produce v3. The lowering
pipeline is the final piece because it sits on top of all four.

Concrete prerequisite skills (reader inference):

1. `tree-sitter-sprefa` host grammar exists and parses the full v2.1
   surface byte-for-byte against the hand-written parser.
2. `tree-sitter-sprefa-walker` exists and replaces `v2/src/walk/`.
3. The recursive injection driver from `v3-parse-experiment` is
   generalized into a framework-level loop with a registry +
   per-grammar `injections.scm`.
4. `OpInvocation` migrated to carry parsed `Tree`s.
5. One op (likely `walker` or `json`) is ported to the new shape as
   the reference implementation.
6. Remaining ops follow the reference op's pattern.
