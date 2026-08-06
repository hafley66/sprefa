# PLAN — branch A: dotted heads contribute, they do not create

## Table of contents

| section | answers |
|---|---|
| 1 · verify + spec deviation | first action receipt; what the spec got wrong |
| 2 · what breaks today | what a dotted head does now, both doors; every site to change |
| 3 · the design | signatures, pseudo-code, storage, read/write sequence, uniqueness |
| 4 · the proof | fail-first test, sabotage receipt, regression gate, exact commands |
| 5 · cost pointer | the file-specific cost of this branch (COST.md) |

---

## 1 · Verify + spec deviation

Verification, first action:

```
$ git rev-parse HEAD
31bf4af13cbd791d625558e8c37a11b9795bc8d8
```

Matches the base commit. Continue.

Ruling files: `v6/prolog/conformance/rulings.pl:608-609` (block_lowering_first) and
`rulings.pl:613-614` (catalog_universe) read in full. The module-catalog ruling
`plans/2026-08-03-module-catalog-ruling.md` was ABSENT on my first read (the first
`find` and `ls plans/` both returned nothing) and appeared later in the session as an
untracked file (checked 08:19). I read it in full; the compressed spec summary matches
it, and stance 8 (`ruling.md:52-58`) is verbatim branch A: "a dotted head CONTRIBUTES
to a rel the path's home block declares; it does not CREATE new paths from outside."

Where the spec was wrong: none of the anchor receipts disagrees with the code. The
single deviation is that the ruling file was not present at session start; this plan
reads it where the spec pointed. No block of the design relies on a reading the file
contradicts.

---

## 2 · What breaks today

A dotted head `a.b(x) <- ...` has no working spelling on either door. Both doors fail,
differently, and neither failure is a named refusal.

### 2.1 Text door (`.dl6`): generic parse error

```
$ cat dotted_head.dl6
rel account(name: text, balance: int).
account.balance(N) <- account(N, B), B >= 100.
$ bash v6/prolog/compile/scripts/compile_dl6.sh dotted_head.dl6 out.ts
{"code":"dl_parse_error/2","message":"parse error at line 3, column 8: statement",...}
```

Cause: `parse_dl.pl:1672-1680` `compound_or_var/5` takes the identifier `account`,
sees a `.` (not `(`), and routes into `dot_chain/4` (`parse_dl.pl:1689-1694`), which
builds `dot_get(account, balance)` and RETURNS, leaving `(N)` unconsumed. The
statement parser then fails on the trailing `(` with the generic `statement` error.
There is no dotted-head statement production.

### 2.2 Term door (fixture): compiles to a garbage rel, not a refusal

`'.'(account, bal(X))` is a legal compound, and `compile_fixture` accepts it end to end:

```
$ swipl -q -l v6/prolog/compile.pl \
    -g "compile:compile_fixture(t, 'dotted_fixture.pl', 'out.ts')" -g halt
wrote /tmp/forkA/out.ts
COMPILE-TRACE program=t plan=5/41813 lower=0/1543 ...
```

What the emitted module contains (read from `out.ts`):

| output | shows |
|---|---|
| table ddl | `CREATE TABLE "." (...)` — the functor `'.'` becomes the table name |
| head | treated as rel `'.'`/2 with columns `col1 = account` (literal) and `col2 = bal(X)` (a struct/relation value) |
| boot | `INSERT INTO "." ... SELECT 'account', json_object('fn','bal','args',json_array(...))` |
| `const . =` | invalid JavaScript — `multisetDiff(before.., after..)` |

So the term door lowers `'.'(a, bal(X))` as an ordinary relation whose functor is the
atom `'.'`. Two compounding defects:

- `lower.pl:162` `table_name(Name/_Arity, Name)` drops the arity and uses the functor
  verbatim, so a dotted head's name comes out as the literal `.`.
- Nothing in the pipeline treats a `'.'`-rooted HEAD as a path; only `dot_get/2` (body
  member access) is a path, and `0_dot_expand.pl:274` `contains_dot_get/1` matches
  `dot_get(_,_)` only, never `'.'`/2. The head rewrite `0_dot_expand.pl:76-96` is
  record-member desugaring, not path handling.

No named refusal exists. The five ruling refusals
(`module_name_collision`, `container_and_leaf`, `non_static_rel_arg`,
`growing_instantiation_cycle`, `unresolvable_path`) are named only in the ruling
(`ruling.md:133-134`) and in `SPEC.md:29-31`; a repo-wide grep finds them in no source
file.

### 2.3 Ref collection and the catalog seed

The head `'.'/2` enters the ref inventory as `Ref = '.'/2`:

- `compile.pl:170-173` `program_refs` collects `rule_head_ref(Rule, Ref)` whose result
  for `'.'(a, bal(X))` is `.`/2.
- `declare.pl` path is not reached; `0_program_check.pl:56`
  `relation_kind(_, _, set)` falls back to `set`, so an undeclared dotted head is
  silently admitted (same as any undeclared flat head).
- `lower.pl:646-655` `catalog_row_ddl/3` would seed a catalog row for the rel named
  `.`, at the next positional id.

### 2.4 Every site that changes for branch A

| site | path:line | change |
|---|---|---|
| parser: dotted-head production | `parse_dl.pl:1672-1694` | accept `a.b(x)` / `a.b.c(x)` as a statement head, emit a path term |
| new phase slot | `1_expansion.pl:38-53` | add the module-contribution phase between dot (44) and relation_edge (50) |
| new module resolution | `analyze.pl:255-261` (`program_refs`) | heads are flat after expansion, so no ref change if the phase erases paths first |
| declaration join | `0_program_check.pl:51-56` | the unresolved path name ships as a named refusal, not a fallback |
| table naming | `lower.pl:162` | the mangled name is already a plain atom; arity stays dropped, the digest carries it |
| catalog seed | `lower.pl:646` | declaration site only; contribution adds no row |
| module decl shape | new `nested_ref/3` decl | block flattening writes it; contribution reads it |
| multi-file unit | `v6/tsv2/serve/0_compile.ts:98-108` | a compile unit becomes a set of files |
| refusal registry | `0_refusal_messages.pl:181-193` | register the five named refusals + the arity-mismatch one |

The dot phase `0_dot_expand.pl` is untouched: it handles `dot_get` body member access;
a dotted HEAD is a different shape that the new phase owns.

---

## 3 · The design

### 3.1 Vocabulary and invariant

A dotted head `a.b(x) <- ...` is written by one file and resolves to a declared child
rel of module `a`. On expansion it is erased to a flat atom over the mangled SQL name
(`a__b__<digest>`, ruling M5, `ruling.md:129-131`), so nothing past expansion learns a
construct, matching the phase discipline of `0_dot_expand.pl:19-20`. The mangled name
is a pure function of the path and arity, so the declaring file and any contributing
file recompute the same table name with no cross-file state. The declaration is the
gate that makes a contribution legal; the mangling is the guarantee that two files
agree.

### 3.2 Type signatures (layer 1)

```
% decomposition: a dotted head is a path + a call.
%   a.b(x)      -> segments [a,b], call bal, arity 1, args [x]
%   a.b.c(x)    -> segments [a,b,c], call c, arity 1, args [x]
dotted_head_parts(+Head, -Segments, -CallName, -CallArgs, -Arity).
dotted_head_parts('.'(Segments, Call), ?) :- ...        % text-door term
dotted_head_parts(module_path(Segments, Call), ?) :- ... % both-door canonical form

% mangling, pure and deterministic (ruling M5).
mangled_child_name(+PrefixSegments, +LocalName, +Arity, -MangledRef).

% the declaration table a contribution reads.
nested_ref(+ParentRef, +LocalName, +ChildRef).

% resolution (branch A gate): contribute-not-create.
resolve_contribution(+Segments, +Arity, +Decls, -FlatHead).
resolve_contribution(_ , _, _, _) :-
    throw(unsupported_construct(unresolvable_path(Path))).

% the phase.
expand_module_heaps_in_context(+Context, +Program, -FlatProgram).

% the compile-unit seam (set of files, one catalog).
compile_unit(+Files, +OutFile).
```

Pseudo-code body (layer 1 comment form):

```
expand_module_heaps_in_context(_, prog(Decls, Rules), prog(Decls, FlatRules)) :-
    maplist(rewrite_rule_head, Rules, FlatRules).

rewrite_rule_head((Head0 <- Body), (FlatHead <- Body)) :- !,
    resolve_or_keep_head(Head0, FlatHead).
rewrite_rule_head((Head0 <+ Body), (FlatHead <+ Body)) :- !, ...
rewrite_rule_head(Rule, Rule).

resolve_or_keep_head(Head, FlatHead) :-
    ( is_dotted_head(Head),
      dotted_head_parts(Head, Segments, _CallName, CallArgs, Arity),
      resolve_contribution(Segments, Arity, Decls, FlatRef)      % else throw
    -> FlatHead =.. [MangledName | CallArgs]
    ;  FlatHead = Head ).                                         % flat heads untouched

resolve_contribution(Segments, Arity, Decls, Ref) :-
    append(PrefixSegments, [Leaf], Segments),
    module_child(PrefixSegments, Leaf, Arity, Decls, Ref).        % refuse if absent
```

### 3.3 Storage layout (layer 3)

- `nested_ref(ParentRef, LocalName, ChildRef)` lands in `Decls` when a block
  flatens. `ParentRef` for a root module is its bare name (`account`/0), for a nested
  module the mangled name. `ChildRef` is always the mangled flat name.
- The mangled name embeds arity in the digest, so two paths that differ only in arity
  mangle differently. This is the one place the plan and the shipped `lower.pl:162`
  (arity dropped) disagree: today the corpus has `same_name_two_arities=0` (coordinator
  measurement), so the dropped arity never fired; under modules the digest must carry
  arity or a two-arity child collides. The lowering keeps dropping the arity from the
  NAME; the digest makes that safe.
- The catalog `__catalog_rel` rows are unchanged (`lower.pl:635-637` contract, six
  columns): the module is a `rel` row, its child a `rel` row whose `parent_id` is the
  module's id (`lower.pl:646` seed already writes parented rows for columns; module
  parenting reuses it). A contribution writes NO catalog rows.

### 3.4 Sequence of reads and writes (layer 3)

```
parse each file            -> per-file prog(Decls, Rules), diag line map
merge into one unit        -> prog(UnionDecls, UnionRules)
block flatten (per file)   writes nested_ref/*, mangled child decls
module_contribute          reads nested_ref, rewrites dotted heads to flat atoms
dot phase (44)              untouched (record member access only)
coalesce (45)               per-clause, unchanged
relation_edge (50)          per-clause membership, sees flat heads
program_plan (compile.pl)   program_refs collects flat mangled refs
lower                      table = mangled name; catalog seed from RelPlans
emit                       ordinary flat rel, nothing new
```

Reads: `nested_ref` (decl set) at expansion; `program_refs`/`AllRefs`
(`compile.pl:170-179`) unchanged mechanically. Writes: the rewritten head, the catalog
seed from `RelPlans` (`lower.pl:646`), no addition at the contribution site.

### 3.5 Uniqueness conditions (layer 4)

- Path + arity -> mangled name is injective; two declarations whose paths mangle
  equally (a flat root rel literally named `a__b__<digest>` vs a nested child `a.b`)
  collide and throw `module_name_collision`.
- A name that must be both a module (a path prefix, container) and a leaf rel/0 head at
  the same level throws `container_and_leaf`.
- A dotted head under a rel/N module (children closing over parent columns) is the
  reserved future surface; v1 throws `non_static_rel_arg`. It cannot be resolved
  because `nested_ref` only records rel/0 module children in v1.
- A cross-file contribution cycle that grows an instantiation (module A contributes to
  B, B to A) throws `growing_instantiation_cycle`.
- Arity or local-name mismatch between the declaration and the contribution throws a
  named refusal (`module_head_shape_mismatch`, new; the five ruling names do not cover
  it). A contribution never silently creates: absent `nested_ref` is
  `unresolvable_path`.

The four layers agree except one: layer 2 (signature) lets the digest encode arity
while layer 4 (uniqueness) relies on that encoding; layer 1 (lowering) drops the arity
from the SQL name. That tension is resolved on purpose: the digest is the carrier, the
name need not be. Stated so the divergence is visible.

---

## 4 · The proof

### 4.1 Fail-first test

New plunit group `begin_tests(module_contribute)` in
`v6/prolog/compile/test/plunit_tests.pl`:

```
test(contribute_without_declaration_is_refused) :-
    Prog = prog([], [ (module_path([account, balance], bal(N)) <- source(N)) ]),
    catch(program_plan(fixture(c, Prog, [source(1)], [], [])-[], _),
          unsupported_construct(unresolvable_path(account.balance)), true).
test(contribute_reuses_declared_mangled_name) :-
    Decls = [ nested_ref(account/0, balance, 'account__balance__<dig>'/1) ],
    ...program_plan over a unit that declares `account` and contributes `account.balance`...
    ...assert ddl contains 'CREATE TABLE "account__balance__<dig>"' and no '"."'.
```

RED before the change: the first case currently compiles to the garbage table `"."`
(no refusal), the second has no `nested_ref` machinery to resolve against, so the
assertion fails. GREEN after.

Primary command:

```
swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt
```

Text-door case, the same refusal through the full door:

```
bash v6/prolog/compile/scripts/compile_dl6.sh contribute_without_declaration.dl6 out.ts
# expect unsupported_construct(unresolvable_path(account.balance)), not dl_parse_error
```

### 4.2 Sabotage receipt

Delete the home block `rel account { rel balance(balance: int). }` from the unit while
`account.balance(N) <- ...` stays. The positive test must flip to
`unresolvable_path`. That proves the declare-first gate is load-bearing: if the
mangling alone (with no gate) is what the test asserted, the sabotage would stay green.
Second sabotage: mangle `balance` differently at the declaration than at the
contribution; the assertion on the shared table name flips, catching a disagreement
between the two files.

### 4.3 Regression gate

The existing `catalog_g1` plunit group (`plunit_tests.pl:564-647`, six tests) is the
guard that a module-contribution machinery must not leak catalog side effects:
`catalog_absent_by_default` fails if any program that never names the catalog emits a
`__catalog_rel` atom; `catalog_ids_are_positional` fails if the positional id dump
shifts. A contribution that wrongly adds a row trips both. Full gates:

```
swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt   # plunit
bash scripts/verify.sh                                                     # umbrella
bash v6/tsv2/scripts/sweep.sh                                              # dl/tsv2 sweep, byte-identity
# + the compiler's conformance sweep that the ruling's step 5 gate (c) names:
#   contribution from outside a block == same rules written inside it
```

---

## 5 · Cost pointer

The specific thing branch A makes worse is `COST.md`.
