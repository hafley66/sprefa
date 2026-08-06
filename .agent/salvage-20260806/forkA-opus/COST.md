# COST: branch A, contribute-only

Lane note: this lane ran in a read-only planning harness with no Write tool and
returned its deliverables inline. The coordinator transcribed them verbatim.

## What branch A makes worse

**Declaration-order coupling, bought before there is anyone to sell it to.**
Contribute-only says one file declares and another file contributes. A compile
takes exactly ONE source string (`v6/tsv2/serve/0_compile.ts:98-108`,
`compile(source: string)`, digested into one `.dl6`). There is no second file. So
in v1 the rule can only be violated inside a single file, where the author could
have written the declaration on the line above. Branch A charges a real cost, the
extra declaration and the extra refusal path, for an interface guarantee whose
only consumer arrives with multi-file compile.

**The catalog's child walk gets more to leak.** Two programs booted into one
server database share one connection (`v6/tsv2/serve/4_http.ts:156`) and boot
with `INSERT OR IGNORE` that swallows collisions (`v6/tsv2/serve/3_engine.ts:229-241`).
Today every `parent_id` is 0 or a rel's own id, so a colliding walk returns
another program's COLUMNS. Branch A writes real module edges, so the same
colliding walk returns another program's SUBMODULES. Branch A does not create the
collision and it does widen the blast.

## Where a user is surprised

Declare `rel orchard.tree(tree_id: int, species: text).` then write
`orchard.tree(TreeId) <- source_row(TreeId).`

Every bare rel in dl6 is created by being written at whatever arity you wrote.
`table_name(Name/_Arity, Name)` at `v6/prolog/lower.pl:162` drops arity entirely,
and the corpus measurement is `same_name_two_arities=0`, so nobody has ever felt
it. Under branch A that one-column head is `unresolvable_path('orchard.tree'/1)`,
and the message says the path is unresolvable when the path resolves perfectly
and the COLUMN COUNT is wrong. Dotted rels obey a stricter law than bare rels
sitting three lines away in the same file, and the refusal name points at the
wrong half of the mismatch. The message text has to spell both arities or this
reads as a bug.

Second surprise: `rel orchard().` is not a module declaration. It parses to
`prog([], [])`, verified, because `typed_decl_entries(_, [], []).` at
`v6/prolog/compile/parse_dl.pl:712` yields nothing. Under branch A, containers
exist only because a child declared them, so an author who writes the empty
container first gets silence, then `unresolvable_path` on the child. Either
refuse the zero-column form by name or make it declare a container; silence is
the one option that has to go.

## What I would need to see before shipping

1. **A dotted fixture through the text-door receipt.**
   `cd v6/prolog && bash compile/scripts/text_door_receipt.sh` at 197/197/0. The
   mangled name is a digest, and a digest computed from anything the two doors
   spell differently is a byte difference in the emitted module. That gate is the
   only place it shows.
2. **A decision on the runtime name.** `relColumns` keys are the runtime's rel
   identity (`v6/prolog/emit_ts.pl:661-664`), and `GET /idb/:rel` plus
   `POST /arrivals` read that identity. Under M5 the user's `orchard.tree` becomes
   `orchard__tree__f9fc8ea9` on the wire. Either the HTTP layer learns the catalog
   and accepts the dotted spelling, or the first person to curl a nested rel has
   to read a digest out of a generated file. Ship neither branch until that is
   answered.
3. **Two files in one compile unit.** Contribute-only has no observable meaning
   until a second file exists. Until then the branch is a lint rule with a module
   system attached, and it should be landed as one refusal in one new expansion
   phase rather than as a wave.
