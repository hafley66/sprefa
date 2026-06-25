# TASKS — `feat/json-declarative-pattern` follow-ups

Shipped: Steps 1–7, merged to `main` (c6b8d44) and pushed. Suite green.
Open follow-ups below. Grouped by theme. Priorities: **H**igh / **M**edium / **L**ow.

## Matching support (current)

| shape | works? | note |
|---|---|---|
| `{ a: $x }` | yes | single exact key, leaf |
| `{ $k: $v }` | yes | single capture key → iterates entries |
| `{ a: { b: $x } }` | yes | single key, nested descent (any depth) |
| `{ a: $a, b: $b }` | yes | conjunctive, **but only if every value is a capture leaf** |
| `{ a: { b: $x }, c: $y }` | no | nested value in multi-entry → bails (T1) |
| `{ $k: $v, kind: u }` | no | capture key mixed with exact in one object → bails (T3) |
| `{ a: $x } OR { b: $x }` | no | no alternation in the grammar (T2) |
| `{ a.b.c: $x }` | no | must nest; no key-path shorthand (T4) |

Workaround for OR today: multiple `json(...)` rules unioned at the head relation.

## Matching gaps

- [ ] **T1 (H)** Nested + conjunctive combo: `{ a: { b: $x }, c: $y }` produces
      no match today. `walk_object`'s conjunctive arm requires every value to be
      a bare capture leaf (`vpat.first() == Leaf{capture:Some}`). Generalize via
      continuation-passing: each entry's value sub-pattern threads the bind vec
      into the next sibling, emitting once at the end.
- [ ] **T2 (H)** OR / alternation: no `|` in the grammar. Decide a surface:
        - (a) pattern union at the literal: `q:{ a: $x } | { b: $x }`
        - (b) datalog-native: multiple `json(...)` rules unioned at the head
          (works today, no syntax change — probably the right default).
      Add (a) only if (b) proves too noisy.
- [ ] **T3 (M)** Capture + Exact mix in one object: `{ $k: $v, kind: user }`
      bails today (multi-entry rejects non-Exact keys). Semantics choice: does
      the capture-key iterate (cross product with the exact-key filter) or does
      it bind the matched key only when the exact siblings also match?
- [ ] **T4 (L)** Key-path shorthand: `{ a.b.c: $x }` sugar for
      `{ a: { b: { c: $x } } }`. jsonpath-ish convenience. Low cost, low need.
- [ ] **T5 (L)** Optional/nullable keys: `{ a: $x, b?: $y }` (absent `b` doesn't
      fail the match). No nullable support now.

## Surface / binding

- [ ] **T6 (H)** Capture/head-var mismatch is a **runtime** error today
      (`head var X unbound in source rule`, engine.rs ~4751), not typecheck.
      Surface the parsed capture names to typecheck (open decision #2 in the
      plan): store `caps: Vec<String>` on `BodyItem::Json`, or re-parse in
      typecheck. Emit a clean diag naming the mismatched var.
- [ ] **T7 (M)** `LeafPattern` (a quoted value containing `$`, e.g.
      `"$REPO:$TAG"`) is matched **literally** today. Decide: extract the `$`
      sub-strings as captures (v4 had a Segment matcher), or keep literal-only
      and document. v4 `compile_pattern`/`parse_segment_pattern` is the
      reference if we extract.
- [ ] **T8 (M)** Quoted-key escaping: the `"..."` key scanner has no `\"`; a
      key containing a literal `"` can't be matched. Add backslash escape in
      `key()`'s quoted arm (and `quoted_value()`).
- [ ] **T9 (L)** `**` has no depth guard; a pathological doc could recurse
      hard. Add a max-depth to `all_descendants` / the `Any` arm.

## Naming / consistency

- [ ] **T10 (L)** Scheme word is `q:`. Revisit (`json:` / `pat:`) if a second
      target lands. One-line change in `desc.rs::SCHEMES`; tests carry the rest.
- [ ] **T11 (L)** Reserve-name guard is body-dispatch only (matches how
      `scan`/`match` work). If a decl-level guard is ever wanted, apply it
      uniformly to all ops — not just json/jsonp.
- [ ] **T12 (L)** `Step::AnyCapture` and `KeyMatcher::Recursive` exist in the IR
      but have no parser surface (the `$$${PATH}` key was dropped). Either add a
      surface (e.g. `**$: $path` to bind the traversed dot-joined path) or
      remove the dead variants.

## Performance

- [ ] **T13 (M)** The engine re-parses the pattern body **per file** invocation
      (`parse_pattern` in the engine.rs `BodyItem::Json` arm). Cache the parsed
      `Vec<Step>` on the BodyItem (parse once) — needs ast.rs→datapath coupling
      or a `OnceCell`. Pattern is tiny so cost is small, but a warm scan re-parses
      N files × M json-rules per tick.

## Bigger directions (design, not yet scoped)

- [ ] **T14 (M)** AST target: the `Step` IR + tree-descent over a code AST
      (tree-sitter), reusing `entries`/`items` semantics for node fields/children.
      "json-style brace query over code." Overlaps `sg`/`ast` ops — needs a pass
      on what a "key" means per node kind before building.
- [ ] **T15 (L)** Relation-graph target (GraphQL-ish brace selection over the
      call/ref/flow graph). Different evaluator (relation-join, not tree-descent);
      datalog already queries it. This would be a sugar over relations, not a
      tree walker. Separate project; do not fold into `run_pattern`.
- [ ] **T16 (L)** rel-as-scope (parked from the prior session): repo-optional
      via named projection, superseding the repo positional-column churn. Would
      change how `scan`'s `repo` threads through every op.

## Docs / polish

- [ ] **T17 (L)** Add a declarative-`json` example `.dl` under `examples/` with
      fixture data; the existing `openapi.dl` still uses `jsonp`.
- [ ] **T18 (L)** VSCode grammar (`editors/vscode-dl`): `q:` is already colored
      as a scheme literal; verify the brace body reads well and the `$cap`
      holes inside `q:{...}` get the metavar highlight.
- [ ] **T19 (L)** Update the json-declarative plan doc
      (`plans/2026-06-25-json-declarative-syntax.md`) status: Steps 1–7 done;
      record the deviations (`q:` carrier, dropped `$$sigil`/cross-ref/`$$${PATH}`,
      LeafPattern literal, shallow conjunctive).

## Decisions log (what we already chose)

- Carrier = `q:` PathLit (structured/highlightable, not a string). `json` is the
  declarative op; `jsonp` is the dotted-string rename.
- v4 host-grammar artifacts dropped: `$$sigil(...)` annotations, `${rule.$VAR}`
  cross-refs, `$$${PATH?}` recursive-capture key. `**` kept.
- Capture vars bind lowercase (dl convention); `$NAME` matches a rule var of
  the same name. Literal `$ref`-style keys (OpenAPI/JSON-Schema) **must be
  quoted** (`"$ref"`) — bare `$ref` binds a capture named `ref`. Quoted `"$ref"`
  classifies as Glob → regex-escaped → matches the literal.
- Evaluator is tree-descent over the existing tree-sitter parse (json/yaml/toml
  by extension). No new tables; spans flow to the existing ref spine.
