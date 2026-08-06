# dots, the human version

## the thing you already have

a rel column can name another rel. that is a nested record.
it stores an integer pointing at the other rel's row.

```
rel repo(name: text).
rel file(repo: repo, at: fpath).     <- `repo` column holds an int
rel span(file: file, start, end).    <- `file` column holds an int


  span row            file row           repo row
  +--------+          +---------+        +--------+
  | file=7 |--------->| repo=3  |------->| name   |
  | start  |          | at=12   |        +--------+
  | end    |          +---------+
  +--------+
```

## the thing you do not have

a way to say "walk that arrow".

today you spell the walk two ways, both ugly:

```
way 1, take it apart:
  coord(P, S, E) <- span(file(_, fpath(P)), S, E).
                         ^^^^^^^^^^^^^^^^ name every level, even skipped ones

way 2, bind then decode:
  coord(P, S, E) <- span(F, S, E), decode(F, {at: {name: P}}).

way 3, the dot, does not exist:
  coord(F.at.name, S, E) <- span(F, S, E).
```

all three make the SAME sql. the join is already written and already tested.
so a dot is grammar work only. no new sql, no new rx, no new engine.

## the fight nobody settled

five docs talk about dots. they split two ways and never met:

```
  "a dot is a JOIN"                "a dot is a PATTERN / projection"
  runtime-decomp plan              LANG.md
  lab-assimilation sweep           kernel.pl
  the Proj operator sketch         extraction-spellings

                    both are right
                    the column holds an int
                    -> reading it is a projection
                    -> fetching it is a join
                    nobody wrote that sentence down
```

that sentence is a free win. costs zero code.

## the typespec thing you hate, exactly

typespec uses ONE dot for five different jobs:

```
  Foo.Bar          namespace inside namespace
  Foo.Bar.Model    model inside namespace
  Direction.North  member inside enum
  Iface.write      op inside interface
  Pet.name         property inside model
```

so `a.b.c` cannot be read left to right. you need the symbol table
to know what each piece even is.

then they ran out of dot and bolted on a second symbol anyway:

```
  Pet.name::type      <- `.` for the property, `::` for its type
```

and it gets worse. `using` bindings are visible inside a namespace
but NOT reachable through the dotted path:

```
  namespace Two {
    using One;
    alias B = A;      // fine, A is in scope
  }
  alias C = Two.A;    // NOT fine
  alias C = Two.B;    // fine
```

two resolvers, one spelling. that is the oddity.
downstream the c# emitter just renames things with an underscore
when a namespace and a type collide.

## your three options

```
A) one dot, both jobs        B) :: for namespace         C) no dots (today)
   .  for member                .  for member

   parse: ~22 lines             parse: ~12 for ::           parse: 0
   + fix statement-end dot      + same fix if . ships       + nothing
   + langium too                + langium too
   + rewrite in 2 engines       + rewrite in 2 engines

   sql/rx/ts emit: 0            sql/rx/ts emit: 0           0
   corpus migration: 0          corpus migration: 0         0

   ambiguity TODAY: none        ambiguity: none             none
   (nothing to collide with)
   ambiguity LATER: yes         later: none                 none
   reproduces typespec          avoids typespec             avoids typespec

   dot is a SQL word            :: is nobody's word         nothing to name
   (vocabulary law ok)          (breaks vocabulary law)
```

## the sleeper: you have no modules

```
  v6 dl6:  zero use / import / module lines in 360 files
  v5 .dl:  `use "path".`  = paste the file in, flat, error on clash
```

a namespace symbol needs something to scope.
right now there is nothing. so:

- option A's ambiguity is currently FAKE. nothing to collide with.
- the day modules land, `a.b.c` needs the symbol table, and you
  inherit the typespec `using` hole for free.
- option B has no ambiguity ever, and costs you the vocabulary law.

so the real order is: decide modules first, dots second.

## two bugs found on the way, unrelated to dots

```
1. statement-end dot has no whitespace rule.
   prolog requires "dot then space". we do not.
   this blocks any dot in the language, and it is wrong anyway.

2. the compiler's private table names have NO guard.

   your rel `a`  ->  compiler makes table  __delta_a
   your rel `__delta_a`  ->  ALSO table    __delta_a

   `rel __delta_a(x: int).` parses clean today. proven.
   same for __frontier_ __pre_ __ref_ __tick and friends.
```

both worth fixing whether or not dots ever ship.

## interfaces / traits: the honest answer

you asked if you need them. i went looking for the fakes.
found nine. eight do not want a language construct:

```
  host executor contract      compiler metadata, users never touch it
  host input roles            same
  bind definitions            same
  operator type rules         same (12 rows, works fine)
  TS I-interfaces             already first class in TS
  rust traits (13 of them)    already first class in rust
  watch source seam           already a TS interface
  "one rel one rule kind"     v6 already dropped this law
```

the ninth is real but small. the flow panel finds graph layers
by regex on table names:

```
  SELECT name FROM sqlite_master WHERE name LIKE 'rel_%_node'
  then  nodeTable.replace(/_node$/, '_edge')
  then  a hardcoded list for the pairs that break the convention
```

that is an interface faked with a suffix, enforced outside the language.
fix is a declared fact the panel reads. not a trait system.

one more real one: picking which host executor runs is decided by
grepping the shell template text for `$DL_EXTRACT_BIN` and `{path}`.
a cold author already hit the failure this causes, it is in the source
comments. fix is one word on the `sh` decl saying which contract it meets.

```
  sh span_scan(a: text) -> (b: int) is sprefa_extract = `...`.
                                     ^^^^^^^^^^^^^^^^ ~6 lines of parser
```

verdict to consider, not a decision: no trait system. two one-line fixes.
a trait system would have zero second customers in this repo.

## your calls, in blocking order

1. is a dot ONLY "walk a ref column", or also a namespace?
2. does dl6 ever get modules, or is flat-plus-underscores forever?
3. if both ship: `.` for namespace (typespec's mistake) or `::` (rust's answer,
   breaks the rxjs/prolog/sql-words-only rule)?
4. 2026-05-07 you wrote "namespace via dot-access (`write_cursor`)".
   did you mean a real dot, or underscore names?
5. dot allowed in head position, or body-only with an explicit bind?
6. does the dot REPLACE one of the two existing spellings, or make a third?
7. ship the two bugs above regardless?
8. explicit contract word on `sh` decls, yes or no?
