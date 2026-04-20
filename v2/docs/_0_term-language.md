# Term language

Source: chat 2026-04-18. Confirmed in conversation; some sections marked
[tentative] are author-suggested and not yet locked.

## Casing-as-syntax (locked 2026-04-19)

First-character case of an identifier is a grammar disambiguator, prolog-style:

- `lowercase` — op / rule head. Verb. `ast`, `json`, `repo`, `my_rule`.
- `UPPERCASE` — term decl / ref (capture). Noun. `X`, `NAME`, `DEF`.

Parser routes on first char. No keyword reservation, no sigil required at
op position. `${...}` and `&{...}` still carry sigils at ref sites because
those positions can also host op chains.

## Concept rename

"Capture" is retired in favor of **term**.

A term has two states determined by syntactic position:

- **Decl** — appears at pipe-head OR as a named-group opener
  `(name)` before an op chain. Introduces a binding.
- **Ref** — appears anywhere else, resolves against the in-scope decl
  set. Scope depth and whether the target is a "rule" or a "nested
  capture" do not matter; one Ref kind handles all targets.

What used to be five distinct surface concepts collapse into one term lattice:

```
Term
├── Decl   ${X}                introduced in pipe-head OR by (X) group opener
└── Ref    ${X}, &{...}        resolves to any in-scope Decl (any depth)
```

Author note (2026-04-18): the prior "rule" / "xref" / "path-ref"
distinctions are dropped. A rule is just an alias for an expression
chain, opened by `(name)` before an op name. There are no statements;
only expressions. Every named expression is the same kind of thing,
and a Ref to it is just a Ref. `${X}` works the same whether `X` was
declared at the top level, inside a fork arm, or as a sub-expression
five levels deep.

`&{...}` (address ref) stays as a separate sigil because it points
into structure (cursor slot, source-tree path), not into the term
binding map.

All Refs resolve to a single `Resolution { target, tri: Tri }` record
stored on a sidecar map keyed by parse-tree node id. The assumption
checker writes the Tri; downstream consumers read it.

Tree-sitter shape: `${X}` and `&{...}` are host nodes, not leaf
tokens. Frontmatter:

```
(term_decl name: (ident))                   in pipe-head or (X) opener
(term_ref  name: (ident))                   bound by lookup
(addr_ref  body: (addr_expr))               structural address into a tree
```

Host-node form (vs leaf token) buys: per-field hover dispatch,
refactor-rename targeting just the ident, future arity sigils as a
`kind:` field, error recovery if the brace is unclosed.

## Sigil family

Three sigils, all in the same family. Brace-required at host level.

| Sigil  | Meaning                                                                  |
| ------ | ------------------------------------------------------------------------ |
| `${X}` | scalar term: one value per row                                           |
| `$$${X}` | multi/list term: sequence of values per row (mirrors ast-grep `$$$VAR`) |
| `%{X}` | reserved third sigil (purpose TBD)                                       |

Direction posture:

- `$` outward — creates a binding, fans out into rows ("creates SQLite rows
  at end of day"), unbounded assignment.
- `&` inward — references existing structure (cursor slot, source-tree node).
- `%` reserved.

## Bracing rule

Bare ident is allowed only in op-position (head of an op_call). Every term
or address reference outside op-position requires its brace pair.

The previous "$VAR ≡ ${VAR}" synonymy is retired.

Why: one rule that holds in every position eliminates precedence ambiguity
between adjacent text and term refs, and lets the host parser tokenize
without lookahead.

## ast-grep boundary

ast-grep metavars stay sub-lang. Inside an `ast[lang](<<...>>)` body:

- Bare `$NAME` and `$$$NAME` are ast-grep metavars, opaque to host.
- `${prefix}` and `$$${rest}` are host term refs, substituted INTO the
  ast-grep pattern source by a host pre-pass before ast-grep parses.

The brace draws the line between sub-lang and host. ast-grep never sees
the braced forms; the host never claims the bare forms.

## `${...}` as a recursive pipeline site [exploratory]

Source: chat 2026-04-19. This section is direction-of-thinking, not
a locked decision. An earlier draft of this section proposed a
"constraint chain" with predicate-only gating; that framing has been
retracted in favor of the broader idea below. Both shapes are still
on the table; documenting the current frame so it can be evaluated
against alternatives.

### The frame

`${...}` is host-language mode. Whatever grammar the host uses for
top-level pipelines (`op > op > op`) recurses inside the brace
expansion. A bound name is itself a pipeline expression. Exprs all
the way down. The shape echoes BigQuery's pipeline-syntax extension
to SQL: every site can host the same compositional form.

```
${VAR > line(re{...})}
```

Reads (under one possible semantics) as: at this expansion site,
take the implicit input cursor, run the pipeline `line(re{...})`
against it, bind the result to `VAR`. The `>` inside the brace is
the same operator as the `>` outside — same grammar, same lowering,
recursive application.

A bare `${X}` would be the degenerate case: a one-op pipeline that
identity-binds the source value at the site to `X`.

### What the frame would buy (if it holds)

- One grammar for composition, used at every nesting level. No new
  `tree-sitter-sprefa-term` sub-grammar.
- One compile target (`Pipeline`), no separate predicate trait.
- Constraint / predicate / transform distinctions collapse into
  per-op behavior. A "filter" op gates, a "to_lower" op rewrites,
  composition decides.
- A single mechanism for what would otherwise be ast-grep `where:`
  vs sprf constraint chain — sprf composition would lower to
  whatever the target backend wants.
- Scan-pointer becomes an op, not separate machinery: a
  `scan_pointer(rule.repos)` op consumes a cursor, stamps Tri,
  emits a cursor with the resolution attached.

### What the frame asks for (open questions)

These are load-bearing decisions the frame surfaces. None decided.

1. **Receiver semantics at the expansion site.** The implicit input
   cursor's shape depends on context (walker key position vs regex
   anchor vs op argument vs top-level). Two paths:
   - per-site type spec: every host site declares what kind of
     cursor it provides at expansions inside it. Principled, more
     to write per host.
   - uniform cursor shape at expansion sites: input is always a
     `Cursor` with whatever fields the host site can fill. Uniform,
     loses static type-checking inside `${...}`.

2. **Bare-ident sugar.** `${X}` as shorthand for what exactly?
   `${X = source}`? `${X = identity}`? Locking this fixes whether
   `${X > op}` extends an implicit identity pipeline or replaces it.

3. **Var-as-named-pipeline.** If `(VAR) > rule(name) > fs(...) >
   line(re{...})` defines a named pipeline, then `${VAR}` at a use
   site has two semantics to choose between:
   - macro expansion: substitute pipeline source at the site,
     re-parse in the new context. Simple, hygiene problems.
   - named composition: reference the compiled VAR pipeline as an
     op. Hygienic, cacheable, asks VAR to be a first-class op-like
     thing with its own trait shape.

4. **Top-level implicit input.** Inside a walker pattern the
   implicit input is the json value at the position. At top-level
   `.sprf`, what is `${VAR}`'s implicit input? Probably the
   rule-root cursor; needs locking.

5. **Scan-pointer-as-op shape.** Today scan-pointer logic lives
   inside the cursor-ref op. Promoting it to "any op can declare
   itself a Tri-stamper" is a real architectural shift. Worth
   sketching the trait surface before committing.

### Alternatives still alive

- **Constraint-chain (predicate-only)**: the prior draft. Smaller
  surface but loses transform composition and duplicates ast-grep
  `where:`. Retracted as primary direction.
- **Walker-only constraint slot**: extend the walker DSL with
  `${X where re{...}}` inside walker bodies only. No host changes,
  no new sub-grammar, no recursive pipeline at expansion sites.
  Smallest surface, smallest expressive power.
- **Status quo**: `${X}` is identity-bind only; constraints and
  transforms live in downstream pipeline ops as separate cursor
  flow. No expansion-site composition at all.

### Concrete grammar sketch (illustrative, not locked)

If the recursive frame holds:

```
expr        := op_call ('>' op_call)*
op_call     := IDENT ('[' bracket ']')? ('(' paren ')')? ('{' brace '}')?
term_decl   := '${' IDENT ('=' expr | '>' expr)? '}'
term_ref    := '${' IDENT '}'                  # disambiguated by scope
```

One `expr` non-terminal, used at top level, inside braces, and
inside `${...}`. The "inside `${...}`" case differs only in what
the implicit input is.

### What this changes vs the retracted chain proposal

| Item                          | Constraint-chain (retracted)              | Recursive-pipeline (current frame)    |
| ----------------------------- | ----------------------------------------- | ------------------------------------- |
| New sub-grammar               | `tree-sitter-sprefa-term`                 | none — host grammar recurses          |
| New compile target            | `Predicate` trait per pattern grammar     | none — existing `Pipeline` enum       |
| `cursor.captures` shape break | yes (multi-binding from regex groups)     | no (one ident per `${...}`)           |
| Cross-grammar rename problem  | yes                                       | no                                    |
| Ways to constrain             | two (host chain vs ast-grep `where:`)     | one (composition lowers either way)   |
| Open questions                | ~3 (compile target, perf, capture shape)  | 5 (listed above, all load-bearing)    |

Smaller surface, more open questions. Worth sitting with before
committing.

## Multi-arity hover

Hover support for `$$$X` multi-match against ast-grep results is a
backburner feature. Preserve the syntactic real estate even though the
runtime feature is not landed.
