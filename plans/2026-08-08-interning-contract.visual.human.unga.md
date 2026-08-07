# Interning contract, plain words

Same plan as `2026-08-08-interning-contract.md`, told without a single file
reference. If you only read one, read this one.

## TOC

- [1. The one-sentence version](#1-the-one-sentence-version)
- [2. What the compiler writes today](#2-what-the-compiler-writes-today)
- [3. What it writes after](#3-what-it-writes-after)
- [4. One word bag for the whole program](#4-one-word-bag-for-the-whole-program)
- [5. The view ships with the table](#5-the-view-ships-with-the-table)
- [6. The scary part: sorting](#6-the-scary-part-sorting)
- [7. The front door](#7-the-front-door)
- [8. Table shapes](#8-table-shapes)
- [9. The escape hatch](#9-the-escape-hatch)
- [10. Proving nothing broke](#10-proving-nothing-broke)
- [11. Who builds what](#11-who-builds-what)
- [12. Numbers to hit](#12-numbers-to-hit)
- [13. Things that will bite](#13-things-that-will-bite)

---

## 1. The one-sentence version

Stop storing the same string forty times. Store it once, in a numbered list,
and put the number in every table that mentions it. Ship a decoder view with
every table so nobody ever has to write the join by hand.

This has been built and lost four times. It dies every time because someone
treats it as an upgrade to apply later. So this time it is the compiler's
default path, and the decoder view comes out of the same function call as the
table.

---

## 2. What the compiler writes today

```
                     rel flow_reach
   ┌──────────────┬──────────────┬────────────┬────────────┐
   │  from_path   │  from_name   │  to_path   │  to_name   │
   │  TEXT        │  TEXT        │  TEXT      │  TEXT      │
   └──────────────┴──────────────┴────────────┴────────────┘
    all four are the PRIMARY KEY, table is WITHOUT ROWID

   src/engine/lower/pass_2/module_17.ts   resolveBindingStep_3   ...
   src/engine/lower/pass_2/module_17.ts   resolveBindingStep_4   ...
   src/engine/lower/pass_2/module_17.ts   resolveBindingStep_5   ...
                  ▲
        38-40 bytes, 24 of them the same prefix, written again per row
```

That path is copied into the table's key, into the delta table, into the
frontier table, into the wave table, and into every index over any of them.
Five or more copies of the same forty bytes, per row.

Count from the current corpus: the compiler emits 754 tables, 569 of them
`WITHOUT ROWID`, and 491 of those have at least one TEXT column sitting in the
primary key. That covers 167 of the 211 programs that compile, and 190 of the
211 carry a text column somewhere, in or out of a key. This is the normal
output.

Every comparison inside the database walks those shared prefix bytes one at a
time before it can decide which row is bigger.

---

## 3. What it writes after

```
     __str  (the word bag, one per database)
   ┌───────┬────────────────────────────────────────────┐
   │ __id  │ content                                    │
   ├───────┼────────────────────────────────────────────┤
   │   1   │ src/engine/lower/pass_2/module_17.ts       │
   │   2   │ resolveBindingStep_3                       │
   │   3   │ resolveBindingStep_4                       │
   └───────┴────────────────────────────────────────────┘

                     rel flow_reach
   ┌──────────────┬──────────────┬────────────┬────────────┐
   │  from_path   │  from_name   │  to_path   │  to_name   │
   │  INTEGER     │  INTEGER     │  INTEGER   │  INTEGER   │
   ├──────────────┼──────────────┼────────────┼────────────┤
   │      1       │      2       │     1      │      3     │
   └──────────────┴──────────────┴────────────┴────────────┘

               __txt_rel_flow_reach  (a view over the table above)
   ┌──────────────────────────────────────┬──────────────────────┬─...
   │ src/engine/lower/pass_2/module_17.ts │ resolveBindingStep_3 │
   └──────────────────────────────────────┴──────────────────────┴─...
```

Measured on identical four-column tables: integer keys insert 1.7 to 2.0 times
faster than text keys, at every size from four thousand rows to a million.

---

## 4. One word bag for the whole program

Two options were on the table.

| option | what it means |
|---|---|
| one bag per column shape | `flow_reach.from_path` gets its own numbered list, separate from `call_edge.caller_path` |
| **one bag for everything** | every string in the program, in one numbered list |

Pick one bag. The reason is a bug rather than a benchmark.

```
   rule:   reach(P) <- module(P), touched(P)
                       ▲          ▲
                       │          │
                    bag A       bag B
                  "foo.ts" = 7   "foo.ts" = 3

   the database compares  7 = 3   and answers "no rows"
   with no error, no warning, and no way to notice
```

Two bags means the same string has two different numbers, and every rule that
joins two relations on a text column silently returns nothing. Fixing that
would mean decoding both sides of every join, which is exactly the hand-written
string join this whole plan exists to abolish.

Also: every earlier version that survived used one bag. The two that used
something else are the two that died.

Costs of one bag, written down rather than hidden:

- it is one busy table, which matters the day there are two writers
- deleting a relation cannot delete its strings
- it grows and never shrinks within a run

None of those change any decision here. The growth one gets measured before
anyone builds a cleanup.

---

## 5. The view ships with the table

The recorded sentence that killed version four was "view deferred until
verified live". The view never got verified live. It never got built.

So the fix is structural rather than procedural.

```
   the function that emits DDL returns a LIST

     plain relation      →  [ CREATE TABLE ... ]
     relation with text  →  [ CREATE TABLE ... , CREATE VIEW ... ]
                                                 ▲
                                    same call, same list, same commit
```

There is no second function. There is no follow-up task. There is nowhere in
the code for a table to exist without its decoder. The test is a length check:
every relation with an interned column returns exactly two things.

The compiler already writes this exact line for declared struct types. It has
worked for a month. The change applies it to plain text columns too.

Written as rx, the view is the last operator in the chain:

```
  arrivals ──scan──▶ word bag ────────────────┐
     │                                        │
     └──map(text→number)──▶ the walk ──▶ withLatestFrom ──▶ decoded rows
                            (never sees                       │
                             a string)                        ▼
                                                       what serve and the
                                                       tick log read
```

---

## 6. The scary part: sorting

Numbers do not sort the way words do. `"apple"` comes before `"banana"`, but
apple might be number 91 and banana number 4. Every place the system leans on
word order rather than word identity is a place this plan can break.

The good news, found by reading the compiler: the language mostly refuses to
sort text already.

| what a rule can say about text | today | after |
|---|---|---|
| `A < B`, `A > B` and friends | refused, numbers only | still refused, nothing to break |
| `A == B`, `A \== B` | works | works, because one bag means one number per word |
| `min(A)`, `max(A)` | refused on text, numbers only | still refused |
| `regexp(A, "...")` | works | **breaks** without a decode |
| `norm(A)` | works | **breaks** without a decode |
| joining two strings together | works | **breaks** without a decode |
| `group_concat` and `json_group_array` sorted by value | works | **breaks** without a decode |

Four breaks, all in the same family, all fixed by one rule:

> If an expression wants the WORD, hand it the word. If it only wants to know
> whether two things are the same, hand it the number.

One function decides which. Five call sites use it. A test walks the operator
registry and fails if any text operator is missing its decode, so the next
person to add a text operator finds out immediately.

### Why the tick log survives

```
  table (numbers)
      │
      ▼  the decoder view
  rows with words
      │
      ▼  multiset diff  ── keyed by row content, order does not matter
  add / del lists
      │
      ▼  tick log       ── sorts both lists alphabetically before printing
  bytes on disk
      │
      ▼
  compared against the oracle, which never touched SQL and did not change
```

Scan order never reaches the comparison. That is the whole safety argument, and
it holds only if the decoder view exists. Which is why the view is section 5.

### The one place order really does change

Row insertion order changes. That is invisible everywhere except two spots:

1. append-only log relations, where physical row order is the data
2. `keep(count(N))`, which keeps the last N rows by insertion order

Both get a new fixture built to fail before the fix and pass after.

---

## 7. The front door

Two SQL statements per tick, whatever the number of rows or relations in it.

```
  ┌── one tick ──────────────────────────────────────────────┐
  │                                                          │
  │  incoming rows, all relations, all text values           │
  │            │                                             │
  │            ▼                                             │
  │   statement 1:  add any new words to the bag             │
  │   statement 2:  read back the numbers for all of them    │
  │            │                                             │
  │            ▼                                             │
  │   rewrite every row: word → number                       │
  │            │                                             │
  │            ▼                                             │
  │   struct plane runs (it also has text columns)           │
  │            │                                             │
  │            ▼                                             │
  │   the rules run                                          │
  └──────────────────────────────────────────────────────────┘
```

Word interning goes FIRST, before the struct plane, because struct target rows
have text columns of their own and their identity key must be computed over
numbers.

The struct plane uses three statements because its key can be part of the row,
so two different rows can claim the same key and it has to check. The word bag's
key is the whole word, so that check cannot fail and the statement is deleted
rather than copied.

Nothing is allowed to be NULL. Stored text columns are already declared NOT
NULL, and the word bag's column is NOT NULL too. A NULL arriving at the door
stops with a named error saying which relation and which column. The empty
string is an ordinary word with an ordinary number.

### The rail that keeps it flat

A test counts SQL statements while feeding the door 1, 3, and 50 distinct
values across 1 and 4 relations. The answer must be 2 every time. Feeding it
nothing must cost 0.

The test header records what it looks like when sabotaged: switch the door to
one statement per row, watch the count go to 50, that is the number the test is
guarding against.

---

## 8. Table shapes

Two separate questions that keep getting confused.

```
   question 1: does this column hold a number instead of a word?
               → decided by the column's type and the escape hatch

   question 2: does this table have a hidden row counter?
               → decided by whether the fixpoint walks it a certain way

   these are INDEPENDENT. Do not couple them.
```

| table | shape | why |
|---|---|---|
| ordinary relation | keeps its current no-rowid shape, keys now numeric | fastest insert available, and it collects the 1.7-2.0x win directly |
| recursive head that the fixpoint walks | gains a rowid | the sub-second work needs to read "everything added since last round" as a rowid range, which a no-rowid table cannot do |
| wave, ping, pong, cone tables | unchanged | their scan order IS the ordering contract; touching them moves every program's output |
| the arrivals staging table | unchanged | its rowid is the sequence number |
| log relations | unchanged | duplicate rows have to physically coexist |

One gap to state plainly: the recorded cost constant comparing the two table shapes is
ambiguous about which side owns which number, and the insert ladder above it
reads the other way. The lane that flips head shapes measures it first and
writes the direction down. Do not flip a table on the strength of a number
nobody can read.

---

## 9. The escape hatch

Interning is a 2.44x win when words repeat and a 1.2% loss when every word is
unique. That was measured in this repo and written down nowhere useful.

So the default is on, and there is one word to turn it off:

```
  rel http_log(url: text, body: text) log keep(count(64)) direct(body).
                                                          ▲
                                            url gets a number (paths repeat)
                                            body stays a word (every one unique)
```

`direct` is the word because the compiler's internal record already calls the
two choices "direct" and "dict". Same word inside and outside, nothing to
translate.

| turn it off when | leave it on when |
|---|---|
| every value is different: digests, ids, request bodies | the value is a path, a name, a kind, a tag |
| the column is written once, read once, never compared | the column is joined against another relation's text column |
| the relation is a small rolling log | the column is part of a key on a table with real row counts |

Guessing wrong toward `direct` costs 2.44x. Guessing wrong toward interning
costs 1.2%. That asymmetry is why the default is on.

Write one line of reason next to every `direct` in a `.dl6`. Review asks for it.

---

## 10. Proving nothing broke

There are 306 test programs. 211 of them compile. The compiler's output changes
for 167 of them.

```
  what changes                        what cannot change
  ─────────────────────────────       ──────────────────────────────
  emitted SQL: column types           the oracle's answer files
  emitted SQL: one view per relation  the tick logs the runtime prints
  emitted SQL: decode subqueries      which programs compile and which refuse
```

The oracle is the referee and it stays the referee for a structural reason: it
computes the right answer in prolog and never issues a single SQL statement. It
cannot notice this change. So if the tick logs still match, the change is
correct.

Three checks, in order:

1. run the sweep, demand zero wrong tick logs and the same compile counts
2. demand no refusal message changed, except the two brand-new ones
3. **classify every changed line of emitted code**

Check 3 is the one that makes this reviewable. There are exactly four kinds of
line that are allowed to change:

```
   ✓ a column type flipped from TEXT to INTEGER
   ✓ a CREATE VIEW line appeared
   ✓ a read switched from the table to the view
   ✓ a decode subquery appeared in a text expression

   ✗ anything else  ← that is the finding, go look at it
```

Write that classifier as a script. Nobody should read 167 diffs by hand.

New test programs to add, each one red before the fix and green after:

- a sorted `group_concat` where insertion order and alphabetical order disagree
- a `keep(count(2))` log fed in an order that exposes the sequence change
- two relations joined on a text column, asserting the result is not empty
- a text column holding non-ASCII bytes, round-tripped out through the view
- a `direct` relation, asserting its column stayed TEXT

---

## 11. Who builds what

Nobody shares a file. Where two lanes want the same file, they run one after
the other, never at the same time.

```mermaid
flowchart TD
  A["A · the DDL<br/>tables, views, the direct word"] --> AR["A review"]
  AR --> B["B · the front door<br/>two statements + count rail"]
  AR --> C["C · text expressions<br/>the decode rule"]
  AR --> D["D · the compiler record<br/>says dict instead of direct"]
  B --> BR["B review"]
  C --> CR["C review"]
  D --> DR["D review"]
  BR --> E["E · table shapes<br/>measure first, then flip"]
  CR --> E
  DR --> E
  E --> ER["E review"]
```

| lane | job | who |
|---|---|---|
| A | emit the word bag, emit every relation's decoder view in the same breath, parse `direct` | the careful model: deciding which columns intern is a judgment call |
| A review | count the DDL branches, confirm no path returns a table without a view | the fast model: it is a length check |
| B | the two-statement door, the NOT NULL rule, the ordering against the struct plane, the count rail | the fast model: the struct plane is a working template and the SQL is written out for them |
| B review | interface rules, one subscribe, no awaited observables, and does the count test actually count | the fast model |
| C | the decode rule and its five call sites | the careful model: a missed call site is silent, which is the worst kind |
| C review | walk the break list line by line against real emitted SQL; also confirm equality checks did NOT grow a decode by accident | the careful model |
| D | three clauses so the compiler's own record says "this column is a number now" | the fast model |
| D review | does the agreement test compare two real outputs or two hardcoded strings | the fast model |
| E | measure the table-shape constant, then flip only the tables that need it | the careful model: this is a compiler-wide change, not a local one |
| E review | did any tick log move, and did the sequence number move anywhere unpredicted | the careful model |

Every lane starts by fast-forwarding to the commit the coordinator names. If
that fails, stop and say so. Lanes do not spawn helpers.

---

## 12. Numbers to hit

What the bench already measured:

| thing | number |
|---|---|
| interning speed | 7.5 million edges per second |
| interning as a share of total work | 0.06% on small inputs, 4.3% at a million edges |
| turning numbers back into words | 40 to 43 million rows per second |
| that decode versus the insert it feeds | 1 to 29 |
| integer keys versus text keys, same table | 1.68x to 1.99x faster, every size |
| the walk itself on interned data | 97-104% of the integer baseline, inside noise |

What this arc must hit:

| gate | number |
|---|---|
| interning share of a real text-keyed program | at most 4.5% |
| insert speedup on that program | between 1.68x and 1.99x |
| the three shootout cases | no regression, at most +2% |
| single-edge insert and delete ticks | 42ms, 56ms, 82ms, 1ms, all held |
| sweep | zero wrong tick logs |
| changed emitted lines | all four allowed kinds, nothing else |
| every gate | under ten seconds |

One number this does NOT buy: the shootout cases feed integer node ids already.
Their keys are numbers today. Interning cannot speed them up, and the sub-second
goal still belongs to the table-shape lane. Anyone quoting 1.7x at the grid case
has mixed two different workloads.

---

## 13. Things that will bite

| what | when you notice | what catches it |
|---|---|---|
| **numbers sort differently than words** | a sorted list comes out shuffled | the decode rule for expressions, plus two new fixtures for the two spots where insertion order is real data |
| **NULL** | a text column arrives empty | impossible in storage, named error at the door, never a silent zero |
| **numbers do not travel** | somebody copies a snapshot to another database and the numbers mean different words | law: a number is a fact about one database, and nothing written outside the database contains one |
| **the bag only grows** | long-running process, high word churn, the bag outgrows the data | accepted for now, matching how the previous version ships. Measure it on a real workload before building any cleanup |
| **all-unique workloads** | 1.2% slower than doing nothing | the `direct` escape hatch exists exactly for this |
| **doing the swap in the wrong order** | a word lands in a column declared as a number, the database stores it anyway, and the row is unfindable forever | the door's count rail also asserts the batch it hands onward contains no words in numbered positions |
| **a new text operator forgets to decode** | it reads a number, matches nothing, ever | a test walks the operator registry and fails the day the operator is added |
| **the view drifting from the table** | a reader gets fewer columns than the table has | impossible while both are built from the same column list in one function; the reviewer's job is to confirm no second builder appears |
