# 3. The nested DSL spirit

> v0's `rule(name) { ... }` blocks nested repo/rev context like scopes; what v5's flattening traded away, and what a bridge back costs.

**The question.** Before this engine was datalog, it was a different DSL.
The v0 prototype (spring 2026 archive) wrote rules as nested blocks, and the
nesting *was* the query:

```
rule(lock_pin) {
  internal_dep(dep: $DEP, repo: $REPO) {
    fs(**/package-lock.json) > line($DEP.*resolved.*#$PINNED_REV)
  }
};
```

Read it inside-out: for each `internal_dep` row, *inside that repo at that
rev*, scan lock files and grep for the dep. Containment carried context — a
block inherited the enclosing row's repo and rev without naming them — and
`>` piped a file through progressive refinement (`fs > json`, `fs > line`).
The shape of the program looked like the shape of the search. That is the
spirit the question asks about: Haskell/Clojure-mood syntax where structure
does semantic work. What would it take to have it back?

## What v5 traded it for

v5 flattened everything. Context became columns: `scan(repo, rev, glob,
path, rev_out)` takes the coordinate as arguments, and a chain of refinement
became a join on shared variables. The lock-pin rule today is two flat atoms
agreeing on `repo`:

```dl
lock_pin(dep, repo, pinned_rev) <-
    internal_dep(dep, repo),
    scan(repo, "HEAD", "**/package-lock.json", lock_path, lock_rev),
    match_line(lock_path, lock_rev, /${dep}.*resolved.*#(?<pinned_rev>\w+)/, line).
```

The trade bought the whole engine. Flat rules have a uniform algebra, so
they lower to SQL; SQL gives the semi-naive fixpoint, incremental
maintenance, and the `--changed` tick; one rule kind per relation makes
retraction well-defined. Every v5 capability the tutorial teaches rides on
the flattening. What it cost is exactly the two things v0 had: visual
containment, and implicit context flow.

## Half the bridge already got rebuilt

The pipe half came back without anyone calling it that. Term-form extraction
ops run *inside* a rule body on a *bound string*: `jsonp(body, "stargazers_count",
n)` dissects a value an effect fetched two atoms earlier; `match_ast(:css, css_text,
"$PROP: $VAL")` parses a string with another grammar mid-rule
(`examples/styled-components.dl`, `examples/md-fences.dl`). That is `fs >
json > line` reborn, with the pipe spelled as a join variable. The embedded-
language chain — markdown fence to CSS body to declaration — is a three-stage
v0 pipe and it works today.

The containment half is the open one, and it is a *lowering* problem, not an
engine problem. A nested-block surface:

```
in internal_dep(dep, repo) {
    scan(repo, "HEAD", "**/package-lock.json", lock_path, lock_rev)
    ...
}
```

desugars to the flat rule above by splicing the enclosing atom into the body
and sharing its variables — which is precisely the machinery `def` templates
already have. `def` inlines a body at a call site with alpha-renaming so
instantiations never capture each other (README's `via`/`four_hop` example);
a block form is a `def` turned inside-out. No evaluator change; the fixpoint
never sees the nesting.

## Where such bridges die

The archive is honest about why this has not happened, and the reasons are
worth stating because they will still be true tomorrow.

**Implicit capture must be pinned exactly.** v0's answer was "a block
inherits *everything* above it," and that is where nested DSLs rot: add a
column to a parent relation and every nested block downstream silently
changes meaning. The engine's own history rejected ambience twice — the
ambient `recv`/`send` globals for ports were torn out in favor of explicit
`@in`/`@out` declarations, and an earlier imperative "seed pipe" surface was
declined until its body semantics could be stated precisely. A viable block
form has to make capture *explicit at the block header* (name the columns
that flow in) even though that gives back some of the terseness the syntax
exists for. That tension is the design, not a detail of it.

**Grammar surface is paid for in tooling.** The parser, the typechecker's
head-var analysis, `tree-sitter-dl` (which powers editor highlighting), the
LSP, and `--parse-only` all move together for any new form. The repo carries
known LSP maintenance debt already; a syntax whose value is ergonomic has to
clear a higher bar than one whose value is a new capability, because
ergonomics is exactly what breaks when tooling lags the grammar.

**The core must stay flat.** The criterion from this folder's introduction
applies with full force: the block form is admissible only as sugar that
lowers to ordinary rules before the engine sees it. The moment nesting needs
its own evaluation semantics — a scope that exists at tick time — v5 has been
traded back for v0, and v0's chapter of the story (chapter 0 of the book, the
sorrow-and-calm one) explains what that costs.

## The honest verdict

The bridge is smaller than nostalgia suggests and larger than syntax
enthusiasm suggests. Smaller: pipes exist (term-form ops), inlining exists
(`def`), and the desugaring is mechanical. Larger: pinning capture rules,
five tooling surfaces, and a grammar freeze that has to hold for years. The
v0 file that prompted this essay runs about thirty lines; its v5 translation
runs about thirty-five. The gap the bridge would close is five lines and a
feeling. Feelings matter in a language you live in all day — that is why this
essay exists — but the engine underneath is no longer the thing standing in
the way, and that is the actual answer to the question.
