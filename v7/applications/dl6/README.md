# DL6 catalog on DL7

- [What this is](#what-this-is)
- [Reading order](#reading-order)
- [Run it](#run-it)
- [What the layout says](#what-the-layout-says)
- [Two policies, one authored dictionary](#two-policies-one-authored-dictionary)
- [Why the constructor needs two rules](#why-the-constructor-needs-two-rules)
- [Why one file](#why-one-file)
- [Not here yet](#not-here-yet)

## What this is

An illustrative DL6-style source catalog authored as ordinary DL7 userland. A userland constructor
`Interned` marks the text columns that should live in a dictionary. The
existing storage emitter derives the layout. No compiler change, no new
emitter relation, no new representation name.

## Reading order

| file | what it holds |
| --- | --- |
| `0_catalog.dl7` | the whole application: constructor, catalog types, dictionary selections, two policies, companion emitter |
| `../../emitters/2_interned_storage.dl7` | the storage classifier this application feeds |
| `../../test/17_dl6_interned.test.pl` | exact expected rows for every artifact |
| `1_demo.pl` | prints the derived layout |

## Run it

```bash
cd v7                   # from the repository root
just dl6-demo            # print the layout
just dl6-interned-test   # assert every row
```

## What the layout says

`SelectivePolicy` maps only the wrapper. Authored `(Interned text)` columns
reach the dictionary; plain `text` columns next to them stay scalar.

| owner.field | authored type | representation | storage domain |
| --- | --- | --- | --- |
| `Repository.url` | `text` | `scalar` | `text` |
| `SourceFile.path` | `(Interned text)` | `dictionary-local-id` | `SharedTextDictionary` |
| `SourceFile.language` | `text` | `scalar` | `text` |
| `SourceFile.repository` | `Repository` | `reference-local-id` | `Repository` |
| `SourceFile.size` | `int` | `scalar` | `int` |
| `Symbol.file` | `SourceFile` | `reference-local-id` | `SourceFile` |
| `Symbol.name` | `(Interned text)` | `dictionary-local-id` | `SharedTextDictionary` |
| `Symbol.line` | `int` | `scalar` | `int` |

`SourceFile.path` and `Symbol.name` are two owners selecting one
specialization. `intern` mints a single identity `Interned(text)`, so both
fields carry the same target and share one dictionary.

The selection names the wrapper as its scalar, which is the whole mechanism:

```lisp
(: WrappedTextSelection
   (* (: capability "scalar-dictionary-v1")
      (: scalar (Interned text))
      (: dictionary SharedTextDictionary)))
```

The emitter's existing `dictionary-local-id` arm then claims those fields, and
its existing `not (storage_dictionary_target ...)` guard already keeps the
scalar arm off them. Nothing was added to the emitter for this.

## Two policies, one authored dictionary

`SharedDictionaryPolicy` carries both selections, wrapper and primitive, each
naming `SharedTextDictionary`. Every text-shaped column becomes
dictionary-backed, and the dictionary identity row is derived twice and appears
once. The test pins that at exactly one row.

## Why the constructor needs two rules

The replay rule runs while the intern snapshot is still present and
materializes ordinary `Interned` rows. Those ordinary rows are what remain once
the snapshot rows are dropped. The forward rule alone is enough for the storage
layout, because the type-graph edge already carries the identity. It is not
enough for an emitter that reads the `Interned` relation itself:

| constructor rules | storage `fields` rows | companion `interned` rows |
| --- | --- | --- |
| forward `intern` only | 16 | 0 |
| forward plus `intern_snapshot` replay | 16 | 1 |

This follows the forward and snapshot-replay pattern used by `Option` and
`Partial` in the prelude.

## Why one file

The example is kept self-contained so it reads start to finish in one place.
Splitting the constructor from the declarations that apply it needs a
cross-file spelling this application has not established; nothing here settles
whether one exists.

## Not here yet

| gap | state |
| --- | --- |
| compound `Key` label over an application target | covered by binding-symmetry test 18 in the integration branch |
| named alias as a field target | approved follow-up; absent from this checkpoint |
| physical dictionary writes | design only; storage rows are target-neutral |
| SQLite IVM / DD / Rust consumers | not wired to these artifacts |
