# COST: branch B, create-on-write

**What branch B makes worse.** A typo mints a rel. Writing
`usres.active(N) <- signup(N).` compiles clean, emits a table, and produces no
rows and no warning, because an undeclared head already defaults to a `set` rel
with TEXT columns at `v6/prolog/0_program_check.pl:56`. The check branch A keeps
as `unresolvable_path` is precisely the check branch B gives up. Every
misspelling becomes a silent empty table plus two catalog rows.

**The surprising case.** Two rules spelled `a.b` at different arities become two
different rels with the same surface spelling, because U2 folds arity into the
digest. A catalog listing shows `a.b` twice with nothing on screen explaining the
difference. The alternative is worse: leaving arity out reproduces the shipped
`table_name(Name/_Arity, Name)` hazard, where `edge/1` and `edge/2` emit two
`CREATE TABLE "edge"` statements and `isAlreadyExists` at
`v6/tsv2/serve/3_engine.ts:224` swallows the second.

**The second surprising case.** Under one server database, two programs that both
create `a.b` produce the same mangled table name, the second `CREATE TABLE` is
swallowed as already existing, and the second program's rows land in the first
program's table. Their catalog rows do not merge, because `rel_id` is positional
per compile, so the same path appears twice with two ids. Create-on-write
multiplies the number of paths this can happen to, and does not cause it.

**What I would need to see before shipping.**

1. A one-command catalog listing that prints created paths beside the rule site
   that created them, so a typo is visible without a query. Without it, the typo
   cost has no discovery path.
2. The compile unit widened past `compile(source: string)` at
   `v6/tsv2/serve/0_compile.ts:98`. Until then "any file can grow any module"
   means "any rule in the one file", and the cross-file union claim is untested.
3. A decision on `rel_id` stability. Positional ids plus create-on-write plus one
   shared server connection is the collision already demonstrated at `rel_id 6`
   existing twice.
4. `just -f v6/justfile roundtrip`, `text-door`, `plunit`, and `conformance` all
   green with the new phase 43 in place, plus the intentional
   `catalog_ids_are_positional` update reviewed as an id shift rather than
   accepted as noise.
