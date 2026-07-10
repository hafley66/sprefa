---
name: feedback_dsl_functional_no_statements
description: "sprefa DSL is functional/expression-oriented; derivable things are facts/relations that flow, not top-level statements"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 13518730-e9ed-42c4-8d42-54f4af3ff3b6
---

The sprefa v5 `.dl` language must stay functional/expression-shaped: values flow and
compose, even though the engine is datalog. Do NOT add imperative top-level **statements**
for things that are derivable facts.

Concrete ruling (2026-06-01): I added a `repo nearest.` top-level statement
(`Item::Repo`/`RepoSpec`) to pick the source root. Chris rejected it: "why statement, why
not a fact/relation call... its the diff of parents vs not, no?" — nearest-`.git` is a pure
function of a path's parents, so it is a derivable fact, belongs as a relation you join
(`repo_root(dir, root)` / a typed `scan` argument), not a new grammar mode. Reverted; made
`--root` default to nearest-`.git` instead, and the repo coordinate flows as a typed `scan`
arg (commit 5b72394).

**Why:** a statement adds a second grammar mode for something the relational layer already
expresses; it does not compose. The only thing that ever seems to justify a statement is
bootstrap timing (a value needed before evaluation) — that dissolves by passing the value
as an argument/coordinate instead of ambient global config.

**How to apply:** before adding any keyword/directive/statement, ask "is this a derivable
fact?" If yes, express it as a relation/typed value that flows into the rule by join. Reify
the noun as a TYPE (repo/rev should be `Type::Repo`/`Type::Rev`, not `Text`) so the
coordinate is explicit. Aligns with [[feedback_rule_is_function_not_channel]],
[[feedback_no_imperative_seed_pipes]], [[project_callable_value]],
[[project_cons_calling_unification]].
