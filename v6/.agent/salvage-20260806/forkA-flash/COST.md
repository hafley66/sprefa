# COST — branch A (contribute-only dotted heads)

## The specific thing this branch makes worse

A contributing file cannot stand alone. Every dotted head it writes depends on a
shape that a different file in the same compile unit must declare first. Removing or
renaming the home file breaks every contributor at the same time, with
`unresolvable_path` on each dotted head until the shape is re-declared. The flat-head
escape does not close the hole: a contributor that wants its own local rel must give it
a separate flat name, so the module's rules are split between two naming surfaces and
the module is not the single place its shape lives.

## The case where a user is surprised

A user greps a module, edits only the contributing file (say renames a column or
changes a rule body), and the module's other file no longer agrees on the mangled table
name or the column set. The failure points at the contributor, not at the declaration it
drifted from, and the fix requires editing a file the user was not looking at. The
binding is invisible at the contributor until it breaks.

## What I would need to see before shipping it

1. A multi-file compile seam working at all, because contribute-only is meaningless
   with a single-file compiler (`v6/tsv2/serve/0_compile.ts` compiles one source
   string today). Prove two files in one unit resolve one catalog before any dotted
   head lands.
2. A byte-identity run of the existing text-door corpus (the tsv2 sweep, `scripts/verify.sh`)
   staying green, so the new phase provably leaves every current program untouched.
3. The disabled-visibility case closed: what a dotted head does when the module exists
   but the contributor is compiled without the declaring file in the unit. That path
   must refuse loudly and repeatably, never silently fall back to creating.
4. A coordination test: rename a declared child and run the unit, expecting the exact
   named refusal the drift scenario (above) should produce, so the surprise has a
   message.
