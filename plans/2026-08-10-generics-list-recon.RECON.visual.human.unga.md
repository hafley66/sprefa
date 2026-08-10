# Generics and lists in plain words

## The split under discussion

```text
json_list(text)
    one value carried inside one column
    ["a", "b", "a"]

list(text)
    one list identity
          |
          +-- position 0 -> "a"
          +-- position 1 -> "b"
          +-- position 2 -> "a"
```

The first shape travels as one JSON value. The second shape has relational rows that rules can read, derive, retract, share, count, and reorder.

## The empty case

```text
none             no parent-to-list link
some([])         parent-to-list link -> list with zero members
some([x])        parent-to-list link -> list with one member
```

Three storage forks remain:

| fork | how empty exists | rows needed |
|---|---|---:|
| list entity | an empty list still has an identity row | 1 |
| presence link | the parent link proves presence; zero member rows prove empty | 1 link |
| no empty witness | zero members means absent and empty | 0 |

## Identity and equality

```text
List 41 -> [a, b]
List 92 -> [a, b]

identity equality:  41 != 92
content equality:   [a,b] == [a,b]
```

| fork | equality test | construction cost |
|---|---|---|
| entity identity | compare two integer ids | allocate an id |
| interned content | compare canonical content ids | canonicalize and deduplicate members |
| structural content | compare length and every ordered member | scan members on comparison |

## Order and change

Dense positions make reads simple:

```text
before: 0:a  1:b  2:c  3:d
insert x at 1
after:  0:a  1:x  2:b  3:c  4:d
```

That insertion changes every old position after the insertion point.

| order fork | ordinary middle insert | ordered read |
|---|---|---|
| dense integers | one insert plus suffix renumber | sort by integer |
| gapped integers | usually one insert; occasional rebalance | sort by integer |
| fractional labels | usually one insert; labels can widen or exhaust gaps | sort by label |
| linked members | insert member and change neighbors | follow links |

A stable member identity can be separate from its order:

```text
member 77: value=b, order=1
member 77: value=b, order=2   after inserting x
```

## Duplicates and sharing

```text
duplicates:       [a, a] has two member rows with different positions

shared entity:    parent A --+
                              +--> list 41 --> members
                  parent B --+

owned lists:      parent A --> list 41 --> copied members
                  parent B --> list 92 --> copied members
```

Sharing needs a parent-to-list junction and either reference counting, append-only storage, or garbage collection. Ownership permits direct deletion of all members when the parent link disappears.

## Nested types

The generic expander receives the outside shape and closes the inside shapes first:

```text
list(option(text))
     option(text)      first
list(option(text))     second

option(list(text))
       list(text)      first
option(list(text))     second
```

Nested relational lists can point from an outer member to an inner list identity. Cycles then become possible unless construction rejects them.

## Rules made by templates

```text
list declaration
      |
      +-- member relation
      +-- parent link
      +-- optional length relation
      +-- optional order-error relation
      +-- optional JSON view
```

| template fork | generated automatically | recurring work |
|---|---|---|
| declarations plus rules and guards | schema, length, checks, JSON view, selected lifetime rules | every instance maintains every selected artifact |
| declarations only | schema | consumers calculate length, checks, and views when needed |
| declarations plus imported capabilities | schema; selected libraries add rules | only imported capabilities are maintained |

A generated relation can be read in a rule body or written by a rule head. A relation written by a rule must stay out of the arrival-input set. The existing engine behavior duplicates rows when one relation is wired both ways.

## One expansion pass

```text
source types
    |
    v
normalize type shapes
    |
    v
find innermost generic instances
    |
    v
mint names, declarations, optional rules and guards
    |
    +---- new generic instances found? ----+
    |                                      |
    +---------------- yes -----------------+
    |
    no
    v
ordinary rule-sugar passes, checks, and planning
```

The expansion state remembers each normalized type. Repeated uses of `list(text)` reuse one generated schema. Encountering the same type while it is already being expanded reports a template cycle.

## Canonical names

Names must depend only on the normalized type:

```text
option(text)              -> one stable name
text?                     -> the same stable name
list(option(text))        -> another stable name
list(text, dense)         -> includes the dense choice
list(text, linked)        -> includes the linked choice
```

| naming fork | shape | cost |
|---|---|---|
| length-prefixed structural name | reversible full type encoding | long nested names |
| readable stem plus stable digest | short readable prefix and fixed digest | digest version and collision check become contracts |

Declaration order, fixture order, filesystem path, and process hash seed stay outside the name.

## Surface forks

### Parameterized options with a default wrapper

```text
list(text)
    means the default form

list_with(text,
          order: dense,
          identity: entity,
          bound: unbounded,
          duplicates: sequence)
```

The short and fully explicit default forms normalize to the same type and generated name.

### Named flavors

```text
list(text)
deque(text)
linked_list(text)
interned_list(text)
```

Each name owns a fixed combination of ordering, identity, sharing, and duplicate behavior.

### One stock form first

```text
list(text)
```

Additional parameterized or named forms arrive later. The first form's canonical name needs enough version structure to distinguish a later representation change from the same representation gaining more wrappers.

## Option fields that change the type

| field | examples | why it changes generated storage |
|---|---|---|
| ordering | dense, gapped, linked, unordered | member key and reorder behavior change |
| identity | owned, entity, interned content | sharing, equality, and deletion change |
| bound | unbounded, 64 | construction guards change |
| duplicates | sequence, set, bag | key and equality change |
| maintained artifacts | length, order check, JSON view | public generated relations and rules change |

Unknown fields, duplicate fields, incompatible combinations, runtime arguments, name collisions, and recursive template cycles need named checker errors.

## Mechanism size

The existing option expander is 122 lines. The estimated general mechanism and adapters are:

```text
type normalization and names       70..120
fixpoint and deterministic output  70..110
template interpreter               50.. 90
enum adapter                       35.. 70
option adapter                     55.. 90
first relational list template     80..150
errors and collision checks        45.. 80
                                   -------
total                             405..710 lines
```

Parser changes, storage lowering, rendering, runtime code, migrations, and tests sit outside that range.

## Test map

```text
names      equivalent spelling, nesting, collision, fixture-order stability
fixpoint   reuse, dependency order, cycles, deterministic output
rules      body read, derived head, no derived-plus-arrival wiring
lists      empty, duplicates, reorder, sharing, retraction, nesting
engine     aggregate into list, explode out, log, keep, dictionary rendering
errors     wrong arity, bad option, bad combination, collision, cycle
parity     existing enum and option behavior where equivalence is claimed
```
