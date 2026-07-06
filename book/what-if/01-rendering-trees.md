# 1. Rendering trees

> a JSX/hiccup-mood extension for emitting HTML from flat relations; the real gap is one ordered aggregate, not the angle brackets.

**The question.** `dl` derives flat rows. HTML is a nested, ordered tree.
What would it take to *render* — to write a rule whose output is a page, the
way JSX renders a component tree from props?

Worth saying first: the engine already crossed this bridge once, in the other
direction. The TypeScript lift reads JSX *as facts*: an element becomes a
`new` dataflow node carrying the component name, each attribute becomes a
`df_field` row, children land under a `"children"` field, and a component
usage is a call site that `call_edge` resolves (`examples/flow-jsx.dl`). JSX
in, relations out. The what-if is running that desugar forwards: relations in,
tree out.

## The three actual problems

Strip the syntax question away and three semantic gaps remain.

**Nesting.** A tree in a relational world is not a problem; it is two
relations. The engine's own CST relations are exactly this shape:
`node(id, kind, file, lo, hi, parent)` and `child(parent, child)`, with
`closure(child)` as ancestry. Any rule can *derive* a tree today by heading
node rows and edge rows; `examples/anim-deck.dl` does precisely that, emitting
`rel_node`/`rel_edge` rows that a JavaScript app renders. Nesting is solved;
it is just spelled as edges.

**Ordering.** This is the real gap. Relations are sets; `<ol>` is not. An
`ord` column expresses order, but no current construct can *consume* it:
rendering a parent requires concatenating its children's HTML **in order**,
and the aggregates on offer (`count`, `sum`, `min`, `max`) are all
order-blind. Argmax selects one row; rendering needs all rows, sequenced.
The smallest honest extension is an ordered string aggregate:

```dl
html(parent_id, join(child_html, "", child_ord)) <-
    child(parent_id, child_id),
    html(child_id, child_html),
    node_ord(child_id, child_ord).
```

SQLite already has this operation (`group_concat` with `ORDER BY` inside the
aggregate, available since 3.44), so the lowering exists; only the surface
does not. One aggregate is most of hiccup.

**Recursion through an aggregate.** That rule computes a parent's HTML from
its children's HTML: recursion *through* the new aggregate, which classic
stratification refuses for the same reason it refuses recursion through
`count`. But trees give the termination argument for free: `child` is acyclic,
so evaluation by depth (leaves first, then their parents) visits every node
once. The engine already computes exactly this layering; `rel_components`
splits strata by SCC and evaluates acyclic components one pass. A "fold over
a DAG" evaluation mode is a scheduling statement, not new semantics. The
sibling essay on [escaping stratification](02-escaping-stratification.md) is
this paragraph generalized.

## The value-shaped alternative

The relational route above keeps everything in rows. The Clojure-mood route
makes the tree a *value*: hiccup renders `["div" {:class "x"} children]`, a
nested list, and a fold turns it into a string. `dl` is closer to this than
it looks. Callables are already values in the engine; the archived
cons/calling-unification design made a list and a keyword-argument set the
same cell. Land list values plus one `list(expr, ord)` ordered aggregate and
a rule head could build `el("div", class: "x", children_list)` directly, with
render as an ordinary fold over a value, no evaluation-order question at all
(the value is built bottom-up by the same fixpoint that builds any row).

The two routes converge on the identical missing primitive. Rows-plus-edges
needs an ordered string aggregate; values need an ordered list aggregate.
Everything else exists.

## The syntax is the cheap part

Whether you write `el("div", ...)`, `(div ...)`, or `<div ...>` is a lowering
decision, and the engine has two precedents for structured literals in rule
bodies already: term-form `json` takes a query literal (`json(body, q:{ number:
$num })`), and `re` templates take `${}` holes. A quasi-quote form for trees
would be a third instance of the same move — parse a literal, lower to the
flat core. The rule for any of the spellings: it must desugar to `node`/`edge`
rows or a list value plus the one aggregate. If a proposed syntax needs the
evaluator to learn something new, the syntax is wrong.

## What you could build the day this lands

The doc splices of lesson 7 stop being line-oriented: `gen` a whole page from
`type_entity` with real sections and nested lists. The deck and atlas apps
stop shipping a JavaScript render layer for the simple cases; the `.dl` file
renders. An `@out(http)` port (the MCP binding's sibling) serves the rendered
relation, and the reactive tick means the page is *live*: edit a source file,
the fact changes, the tree re-derives, the page updates. A static site
generator where every page is a materialized view is not a metaphor at that
point; it is just what the words mean.
