# Refusals: all 101 re-opened (plain version)

You said re-open all of them. Done. Every one traced to the line of code that
throws it.

## What they turned out to be

```
101 named refusals
│
├─ 39  REAL WALL      breaking it breaks the language
├─ 30  UNFINISHED     somebody stopped, nobody decided
├─ 14  ACCIDENT       a missing table row or a fall-through
├─  9  YOUR CALL      you already ruled on these
└─  9  NOT A THING    dead, deleted, lab junk, or a test harness check
```

44 of them (accident + unfinished) can move. 22 are small, 21 medium, 1 big.

## The four walls, and what each protects

| wall | what breaks if you knock it down |
| --- | --- |
| stratification | rules chase their own tail forever |
| tick planes | edge words in level rules, time gets two meanings |
| id integrity | text "1" and number 1 join in SQL, don't in the oracle, wrong rows |
| tick-log purity | ids leak into printed output, logs stop being comparable |

Every one of the 39 walls is one of those four, or the two doors agreeing.

## Five things wrong in the inventory

```
1. group_concat(X) with no separator      "not implemented"
   truth: separator defaults to a comma. one table row. done.

2. two refusals listed as live              edge_body_needs_negation
   truth: deleted months ago. only tests mention them.        edge_body_needs_now

3. three refusals listed as compiler        openapi_type_unknown, sql_text_mismatch,
   truth: they live in lab folders          value_template_never_shipped

4. one listed as compiler                   param_count_mismatch
   truth: a check inside the test runner

5. inventory says 101 refusals exist
   truth: 111 now. 25 got added after the inventory was written.
```

## The dot thing (modules)

The code comment says "there is no module half in scope". That comment is out
of date three ways:

```
the module id table            EXISTS   (__rel, with module_id + parent_id columns)
you already ruled how dots work  TWICE   (catalog_universe, block_lowering_first)
the use-door that mints modules  BUILT   (and tested)
                                    │
                                    └─ and never called by anything but tests
```

Same shape as the column-type one from this morning. Something built, sitting
next to the door, not plugged in. The fix is swapping one call in the compile
entry point.

## The string thing

You cannot get a directory out of a file path. That is why one fixture hands
`in_dir` in as hardcoded facts instead of computing it.

Reason: the code that handles string functions only accepts functions with ONE
argument. `substr`, `replace`, `rtrim` all take two or three. So they were never
added, and a program that calls one gets an error that says nothing about
strings.

```
what you need for a directory name:  rtrim(path, replace(path, "/", ""))
                                     └── both native SQLite, zero new code
                                     └── blocked only by the arity-1 check
```

Two table rows and a three-line loosening. Real splitting (all ancestor dirs)
is a bigger job and hits a second wall about building strings inside recursion.

## The json thing

`Doc := {name: N, stars: S}` refuses. Not because it can't work. Because if you
let it through today it stores the SHAPE OF THE CODE instead of the data, and
the two engines then disagree byte for byte.

Your own ruling already says json values are ordinary values. So the ruling side
is settled. Somebody just never wrote the lowering. Lift it.

## Order to build in

```
NOW, all at once, nobody steps on anybody
┌──────────┬──────────┬──────────┬──────────┐
│ REGISTRY │ COALESCE │  MODULE  │ SEQ+HOST │
│ 5 rows   │ 5 rows   │ the dots │ 2 rows   │
│ table    │ one file │ one call │ two small│
│ rows only│ 274 lines│  swap    │  files   │
└──────────┴──────────┴──────────┴──────────┘

THEN, one at a time (they all edit the same 5000-line file)
   strings ──▶ json ──▶ aggregates

ALONGSIDE (different file, also one at a time)
   edge checks ──▶ level checks

LAST
   json aggregates (needs the json one landed first)
```

## Five cheapest, in order

| # | thing | cost |
| --- | --- | --- |
| 1 | `group_concat(X)` with a default comma | one table row + one clause |
| 2 | comparison operators with no table row | add the row, no code |
| 3 | `not(X > 1)` refuses, `X =< 1` works | flip the operator, table already pairs them |
| 4 | quote inside a string literal | double the quote, four lines |
| 5 | directory-from-path (`rtrim` + `replace`) | two table rows + arity loosening |

## Two things I want you to look at

**Nine rows are yours.** They have ruling rows with your words in them. I did not
touch them, did not propose reviving them. If you want any re-opened, say which.

**One family is 9% of everything that fails.** Programs that read json inside an
edge rule. Nine of the hundred failing fixtures, all of them ghcacher-shaped.
Four separate refusal names, one root cause: the json machinery was only ever
wired into level rules. That is one arc and it is the biggest single win sitting
there.
