# Branch B: create-on-write dotted heads - design for an auditor

Base commit verified: `git rev-parse HEAD` = `31bf4af13cbd791d625558e8c37a11b9795bc8d8` (matches SPEC).

## Table of contents

| section | question it answers |
|---|---|
| 1. Spec reality check | whether the spec's reading of the code holds; the missing plan file |
| 2. What breaks today | current treatment of dotted heads, each refusal site, every change site |
| 3. The design | signatures, pseudo-code, storage, read/write sequence, uniqueness |
| 4. The proof | fail-first test, sabotage receipt, regression gate, exact commands |
| 5. Costs | the thing branch B makes worse, surprise case, pre-ship evidence |
| 6. Where the spec was wrong | deviations I found |

## 1. Spec reality check

The compressed "already decided" summary is accurate against the code. Three receipts anchor it:

1. `rulings.pl:609` holds `ruling(block_lowering_first, flat_rels_catalog_edges_arg_distribution, user, ...)` - block sugar is the lowering, a FILE is the degenerate first block, modules = rel/0 with children, dotted heads contribute. Verified by reading the clause; the rulings head-atom arity-6 gate is `catalog_mentions_atom/1` at `analyze.pl:208-209`.
2. The catalog schema matches the spec: `catalog_ddl_contract('__catalog_rel', [rel_id-int, parent_id-int, ordinal-int, local_name-text, kind-text, type_id-int])` at `lower.pl:635-637`.
3. The five named refusals are absent from the whole `v6/prolog/` tree (0 hits each; full quiet-grep numeric receipts below in section 2.6).

**One deviation in the spec's anchor list**: the file `plans/2026-08-03-module-catalog-ruling.md` does not exist. The module-catalog ruling lives in `rulings.pl:609` (the `block_lowering_first` clause) and the design doc is `plans/2026-08-03-modscope-plan.md`, whose Step 5 (`:476-482`) is the dotted-head/multi-file contribution gate. The spec's own compressed summary is a faithful reading of both, so the work proceeds on that. Section 6 records this.

The coordinator's measurements are consistent with what I re-derived by running commands (sections 2-3). I did not redo the fixture corpus count (`fixtures=302 refs=1074`); it is not load-bearing for this design.

## 2. What breaks today

A dotted head `a.b(x) <- ...` fails at FOUR distinct doors in sequence, each a site branch B must change.

### 2.1 The dot phase (parse)

`head_atom/6` (`parse_dl.pl:1062-1070`) parses exactly `ident(Name)` then `(`: a single bare identifier, with dot chars illegal (`ident/3` at `parse_dl.pl:411-421` accepts alpha/alnum/underscore only). A text-door dotted head never tokenizes as a head.

Receipt (probed through `parse_dl/4`):

```
error(dl_parse_error(statement,position(2,2)))
```

for the source `rel a(x: int).\na.b(X) <- a(X).` Probe script: `swipl -q -l probe.pl -g "probe_file('dotted_head.dl6', R), writeln(R), halt"`. Command form per SPEC (op/3 directive at file top):

```prolog
:- op(1150, xfx, <-). :- op(1150, xfx, <+). :- op(700, xfx, :=).
```

### 2.2 The dot phase (expansion, term door)

The compiler's own term door (what `program_plan/2` runs after parse) has a dot-expansion phase at order 44 (`0_dot_expand`), wired in `1_expansion.pl:38`. Its refusal is `unresolvable_member/1`, thrown by `check_dot_receiver/3` at `0_dot_expand.pl:171-177` when the chain root is not a bound body variable. The phase header states the current stance verbatim at `0_dot_expand.pl:28-31`:

> "Resolution is receiver-bound-first: the chain's ROOT must be a variable the rule body binds, else the named refusal unresolvable_member. There is no module half in scope, so a chain whose root is not a bound body variable is never silently repairable."

Source: `0_dot_expand.pl:28-31`.

Two consequences for branch B:

1. The phase only rewrites `dot_get/2` chains (`contains_dot_get/1`, `0_dot_expand.pl:274-278`). A term-door `a.b(X)` is `'.'(a, b(X))` under prolog - functor `'.'`, not `dot_get` - so a dotted body atom passes through the dot phase UNTOUCHED. Receipt (probed through `expand_program/3`):

```
ok(prog([...],[(out(_5290)<-a.b(_5290),src(_5290))]))
```

for `prog([col_type(src/1,x,int),col_type(a/0,x,int)],[(out(X)<-((a.b(X)),src(X)))])`. The dotted atom survives without desugar or refusal.

2. `rewrite_head/3` at `0_dot_expand.pl:90-96` handles dots only in HEAD ARGUMENT slots, never in the head NAME. A dotted head NAME is outside this phase's grammar entirely.

### 2.3 Ref collection

`rel_ref/2` (`conformance/body.pl:26`) is `functor(Atom, Name, Arity)`. The ref inventory in `compile.pl:173` unions rule-derived and declared refs then `sort/2` fixes catalog id order:

- `program_refs/2` (`analyze.pl:255-263`) walks `rule_head_ref/2` (`analyze.pl:63-64`) and body uses.
- `rule_head_ref/2` calls `rel_ref/2`, so a dotted head's ref resolves to functor `'.'` and arity `2` - the mangled flat module name never appears unless the lowering phase mints it as a bare `a__b`-style atom first.

Sites that change: `compile.pl:173` (feed is correct once lowering mints flat names), plus the flatten step must run before `materialize_reference_target_rels/2` at `compile.pl:150` so the created module's column decls are injected alongside its target (see 2.4).

### 2.4 Table naming and decl injection

- `table_name(Name/_Arity, Name)` at `lower.pl:162`: the table name IS the rel name, arity dropped. A flat child rel `a__b__<path-digest>` therefore gets its own table by the ordinary path once the ref `Name` is the mangled atom. This stays correct under branch B and needs no change; it is the arity-drop hazard from 3.4.
- `materialize_reference_target_rels/2` (`compile.pl:121-128`) is the existing decl-injection pattern: it synthesizes `col_type/3` decls for columns referenced but not declared. A created-on-write child rel reuses this exact door to get a typed table from its inferred columns (analyze infers columns/`rel_columns/4` at `analyze.pl:285-287`, types by literal witness at `analyze.pl:356-358`, merged by the fixpoint at `analyze.pl:557`). Sequence must be: lowering mints flat refs AND records the created columns, then `materialize_reference_target_rels` injects their decls.

### 2.5 The catalog seed

Catalog rows are DDL-time, gated on whether any rule reads `__catalog_rel`:

- `program_uses_catalog/2` (`analyze.pl:191-209`) - true only when some rule names arity-6 `__catalog_rel`.
- Emission gate at `lower.pl:3568-3572`: `lower_program/2` mints `catalog_table_ddl/1` + `catalog_row_ddl/3` only when that gate is true.
- `catalog_row_ddl/3` (`lower.pl:646-655`) assigns ids POSITIONALLY: primitives `1..5`, then the catalog rel `6`, then each user rel; `catalog_rel_rows/4` (`lower.pl:665-672`) hardcodes `parent_id = 0` for every rel row and `catalog_column_rows/5` (`:674-681`) hangs columns off their rel.
- Seed is ONE `INSERT OR IGNORE` (`catalog_row_ddl`, `lower.pl:653-655`), pinned by `catalog_ids_are_positional` (`plunit_tests.pl:613-634`).

Branch-B change: a create-on-write program mints module/child rows whether or not any rule reads the catalog. The gate must become "uses catalog OR has dotted modules", and `catalog_rel_rows` must write the module parent edges (`parent_id` = the parent module's id) instead of the hardcoded `0`. This is the one catalog producer that already exists; branch B extends it rather than adding a new one.

### 2.6 The refusal set

The whole `v6/prolog/` tree carries 178 `unsupported_construct/1` names (grep `grep -rho "unsupported_construct([a-z_]*(" v6/prolog/ | sort -u`). The refusal list includes the shipped dot refusal `unresolvable_member` and `member_not_a_goal` (both raised at `0_dot_expand.pl:107-108, 169, 176`). All five named-but-unbuilt refusals have zero hits:

| refusal | hits in v6/prolog | receipt |
|---|---|---|
| `unresolvable_path` | 0 | only in `plans/2026-08-03-modscope-plan.md:138,440,481,538` |
| `module_name_collision` | 0 | only in `plans/2026-08-03-modscope-plan.md:140,451` |
| `container_and_leaf` | 0 | only in `plans/2026-08-03-modscope-plan.md:140,451` |
| `non_static_rel_arg` | 0 | spec-named only |
| `growing_instantiation_cycle` | 0 | spec-named only |

So the refusal set is a blank slate for branch B: none of these need changing, they need CREATING in a new checker plus one new refusal for the create-on-write arity conflict (section 3.6).

### 2.7 The serve collision (pre-existing, binds branch B)

`bootServedProgram` (`3_engine.ts:228-241`) replays a program's DDL and swallows "already exists" via `isAlreadyExists/1` (`3_engine.ts:224-226`) - so when a second program boots into the same DB, its positional catalog ids collide and `INSERT OR IGNORE` silently keeps the first program's rows at the shared ids. `4_http.ts` opens ONE `ScratchStore` for the whole server (`4_http.ts:156` per SPEC). Branch B makes this worse because create-on-write raises the COUNT of catalog rows per program; section 5 prices the required fix. The two-program `rel_id 6` as both `alpha` and `beta` observation is not re-run (spec says not to redo measurements).

## 3. The design (branch B, create-on-write)

Create-on-write: a dotted HEAD `a.b(x) <- ...` mints any absent prefix of its path as a rel/0 module and the leaf as a rel, then lowers itself to the flat child rel. Multiple files contributing to one dotted leaf is ordinary datalog union because the flat name is a pure function of the PATH, not the content.

### 3.1 Type signatures

```prolog
% dotted_head_lowering : the new expansion phase, placed BEFORE order-44 dot.
:- module(dotted, [ lower_dotted_program/3 ]).   % (Context, Prog, Prog)
```

```prolog
% Parse: head_atom/6 and the body atom path must accept an atom-root dotted name.
%   head_atom(Head, Vars0, Vars, S0, S)
% new clause: ident(.ident)* -> path([a,b], Args) with args parsed by head_args/5.
```

Internal signatures:

```prolog
% mint_module_tree(+PathAtoms, +Catalog0, -Catalog, -FlatName)
%   ensure every prefix of PathAtoms has a rel/0 catalog row (create if absent);
%   FlatName = the single flat table name for the whole path (path-stable digest).
%   Reads: Catalog0 (declared rels + modules). Writes: new rel/0 rows + the leaf row.

% rewrite_dotted_head(+Head0, -Head, -FlatName)
%   Head0 = path([A,B], Args) -> Head = '.'(-, -)  ; emits the flat relcall for the body.

% rewrite_dotted_body(+Body0, -Body)
%   every body atom whose root is an atom path AND not bound by the body ->
%   flat ref; everything else unchanged (member access stays with dot phase).

% check_dotted_reads(+Paths, +CreatedPaths)
%   a body-only read of a path no head creates -> throw unresolvable_path(Path).
```

### 3.2 Pseudo-code bodies

```prolog
lower_dotted_program(_, prog(Decls, Rules0), prog(Decls, Rules)) :-
    % Pass 1: collect every dotted head -> the created set, mint module tree.
    collect_dotted_heads(Rules0, Heads),               % Heads = [path([a,b],Arity) | ...]
    mint_module_tree(Heads, Catalog0, Catalog, Created),  % absent segments -> rel/0 + leaf
    % Pass 2: rewrite heads and body atom-root paths to the flat leaf.
    maplist(rewrite_dotted_rule, Rules0, Rules1),      % heads -> flat, paths rewritten
    % Pass 3: reject body-only reads of a path nothing creates.
    collect_dotted_body_reads(Rules1, Reads),
    forall(member(Flat, Reads), memberchk(Flat, CreatedFlat)),  % else unresolvable_path
    Rules = Rules1.
```

Column and type synthesis is handed to the existing doors, not re-derived:

```prolog
%   mint_module_tree records only membership; the leaf's columns come from
%   rel_columns/4 (analyze.pl:285) over all contributing rules and its types
%   from the literal-witness fixpoint (analyze.pl:557), both already union-merge
%   across files. materialize_reference_target_rels (compile.pl:121) injects
%   the col_type decls that make lower.pl's ordinary rel_ddl path build the table.
```

### 3.3 Storage layout

| store | what holds the module tree |
|---|---|
| `__catalog_rel` (`catalog_ddl_contract`, `lower.pl:635`) | existing 6-column table; `parent_id` becomes the module parent instead of 0 for nested rows |
| flat table `a__b__<path-digest>` | created by the ordinary `rel_ddl/6` path from injected `col_type` decls; `table_name/2` (`lower.pl:162`) already drops arity |
| catalog child index | existing `__catalog_rel_parent` (`catalog_table_ddl`, `lower.pl:641-642`) |

Disagreement with the signature layer: `mint_module_tree` returns `FlatName` (the signature layer), but the STORAGE layer does not name the flat table by a path it computes itself - it reuses `lower.pl:162`, which takes the already-minted bare `Name` from the ref. The signature's `FlatName` is only used by the LOWERING phase to produce that bare ref; storage never re-derives it.

### 3.4 Storage digests and arity (the live hazard)

The `<path-digest>` suffix must be a pure function of the PATH SPELLING (e.g. `sha256("a.b")[:8]`), so every file contributing to `a.b` lowers to the SAME flat name and union joins on one table. A content-derived digest would give each contributor its own table and silently break union - branch B's central operation - so the digest source is a hard requirement, not a taste call.

`table_name(Name/_Arity, Name)` (`lower.pl:162`) is the coordinator's measured live hazard: one program with `a.b/2` and a separate `a.b/3` would emit two `CREATE TABLE "a__b__<digest>"` with different columns. Under branch B this is a real, reachable path (the corpus's `same_name_two_arities=0` only holds because nothing creates modules today). Create-on-write resolves the shape on first write; a later contributor whose head arity disagrees with the created leaf is a refusal (section 3.6 `dot_arity_mismatch`).

### 3.5 Sequence of reads and writes

```
parse (head_atom accepts dotted name; body path atoms parse)
  -> dotted lowering phase (NEW, order < 44): read declared rels + module tree;
       mint absent modules + leaf rows; rewrite heads/bodies to flat names;
       refuse body-only reads
  -> dot phase (44) unchanged: still the member-access door for bound-var roots
  -> materialize_reference_target_rels (compile.pl:150): inject created col_type decls
  -> materialize_catalog_rel (compile.pl:151): ensure __catalog_rel decls when used
  -> sort(AllRefs0, AllRefs) (compile.pl:173): deterministic leaf order
  -> column_type_fixpoint (analyze.pl:557): merge types across contributors
  -> lower: rel_ddl + catalog_row_ddl write module parent edges
```

The catalog seed runs only when `program_uses_catalog OR dotted seen` (section 2.5) - the gate flip at `lower.pl:3568`.

### 3.6 Uniqueness conditions

- The flat name for path P is a pure function of P's spelling - contributors agree, union is correct (section 3.4).
- Each rel/0 module's `local_name` namespace is shared with its leaf children and its columns; two children with the same `local_name` under one parent is `module_name_collision`; a parent that is BOTH a module (has children) and a leaf (has data rows) is `container_and_leaf`. These two named refusals get their first call sites here.
- The created leaf's arity is fixed by its first contributor; a later head to the same path at a different arity is the new `dot_arity_mismatch` refusal (not one of the pre-named five; it is the create-on-write neighbor of the arity-drop hazard).
- Column names are positional, taken deterministically (sorted ref order) so two files contributing `a.b(X,Y)` and `a.b(A,B)` to one `a.b/2` do not flip columns.
- `catalog_rel_id` stays unique within one compile because ids are positional over the sorted ref list (`compile.pl:173`, `catalog_rel_rows` `lower.pl:665`). Cross-program uniqueness is NOT guaranteed today - that is the serve collision, priced as a pre-ship requirement in section 5, not solved here.

Disagreement with the signature layer: the signature layer lets `mint_module_tree` create rows freely, but the UNIQUENESS layer constrains creation - a head may only create the leaf and its rel/0 prefix, never a parent that already exists as a data leaf (`container_and_leaf`). The layers disagree in degree, not direction; the requirement layer wins.

## 4. The proof

### 4.1 Fail-first test

`create_on_write_outside_block` - a compile where file `a` declares module `a` (rel/0) and a dotted head `a.b(x) <- src(x).` with NO decl for `b` anywhere. Branch B must lower it to the flat leaf, mint `a` + `a.b` catalog rows, build the table, and produce rows identical to the same leaf written inline inside module `a` (the modscope Step 5 union-parity gate, `plans/2026-08-03-modscope-plan.md:480-482`).

Fails on the CURRENT tree: the dotted head throws `dl_parse_error(statement,position(2,2))` (section 2.1 receipt). Passing means the parse accepts the head, the lowering mints the rows, and `catalog_ids_are_positional` shapes still hold for a program that both creates and queries.

### 4.2 Sabotage receipt

A regression that (a) makes `catalog_rel_rows` assign ids non-positionally, (b) breaks a module parent edge, or (c) mints catalog for a program that neither creates modules nor reads `__catalog_rel` is caught by the exact `catalog_g1` plunit pins:

- `catalog_ids_are_positional` (`plunit_tests.pl:613-634`) asserts the literal `(N,parent,ordinal,...)` tuples - a reordered or wrong-parent seed fails here first.
- `catalog_absent_by_default` (`plunit_tests.pl:568-573`) fails the moment catalog text leaks into a non-catalog program (the gate-flip must stay off when unused).
- `catalog_table_shape` (`plunit_tests.pl:577-584`) pins the CREATE TABLE and the parent index.

### 4.3 Regression gate

The catalog plunit group (green today, verified by running it):

```
swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g "run_tests(catalog_g1)" -g halt
```

Result: `6/6 passed`. The full suite command is `-g run_tests` (`plunit_tests.pl:8` author-instructed). The conformance oracle sweep, which guards that lowering did not change any shipped program's semantics (green today):

```
swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
```

Result: runs to `PASS` on the full fixture set. Both `swipl` invocations were executed during this audit and passed.

## 5. Costs

The specific thing branch B makes worse and the surprise case go in `COST.md`. In short: create-on-write removes the only forward declaration of shape, so a misspelled dotted head in one file silently becomes a new empty module instead of an error (a class of silent-catch defect branch A would refuse), and cross-program catalog id collisions (the serve two-program defect) become easier to hit because every create adds rows. Pre-ship evidence is itemized in `COST.md`.

## 6. Where the spec was wrong

1. `plans/2026-08-03-module-catalog-ruling.md` does not exist. The ruling is `rulings.pl:609` and the design doc `plans/2026-08-03-modscope-plan.md`. The spec's compressed summary reads both faithfully, so no design change.
2. Method note: the dot refusal for an ATOM-root dotted body atom in a term-door program is not `unresolvable_member` today (that throw requires `dot_get/2`, `0_dot_expand.pl:274`); a hand-written `a.b(X)` survives untouched as `'.'(a, b(X))`. The spec's claim "an ATOM root is refused by construction" holds only for the TEXT door (parse), which is the real gate. Receipt in section 2.2.
3. The spec's anchor line numbers for `compile.pl` (157, 175) are 16 and 4 lines off the working tree (actual: `sort` at 173, `subtract` at 179). Same symbols, same meaning; cited at their real lines.
