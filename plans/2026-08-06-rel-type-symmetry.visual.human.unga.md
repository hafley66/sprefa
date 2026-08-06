# rel = type, caveman version

## contents

1. [the one idea](#1-the-one-idea)
2. [same gears, both halves](#2-same-gears-both-halves)
3. [four hashes, four jobs](#3-four-hashes-four-jobs)
4. [the trap](#4-the-trap)
5. [file changes, five verdicts](#5-file-changes-five-verdicts)
6. [how far does red go](#6-how-far-does-red-go)
7. [what everyone else does](#7-what-everyone-else-does)
8. [build order](#8-build-order)

## 1. the one idea

You already built the hard half.

```d2
direction: right

types: TYPE system {
  style.fill: "#e6f4ea"
  style.font-color: "#0b3d1f"
  k: "same VALUES = same row" { style.fill: "#ffffff"; style.font-color: "#0b3d1f" }
  u: "UNIQUE (cols)" { style.fill: "#ffffff"; style.font-color: "#0b3d1f" }
  i: "__id = rowid" { style.fill: "#ffffff"; style.font-color: "#0b3d1f" }
  k -> u -> i
}

rels: REL system {
  style.fill: "#e8f0fe"
  style.font-color: "#0b3d6b"
  k: "same NAME = same rel" { style.fill: "#ffffff"; style.font-color: "#0b3d6b" }
  u: "UNIQUE (parent, name, arity)" { style.fill: "#ffffff"; style.font-color: "#0b3d6b" }
  i: "rel_id = rowid" { style.fill: "#ffffff"; style.font-color: "#0b3d6b" }
  k -> u -> i
}

types -> rels: "same trick, different key" {
  style.stroke: "#b06000"
  style.stroke-width: 3
}
```

You did not write an intern table. You asked SQLite for one, twice.

## 2. same gears, both halves

```d2
grid-rows: 6
grid-columns: 3
grid-gap: 8

h0: "the gear" { style.fill: "#f1f3f4"; style.font-color: "#202124"; style.bold: true }
h1: "TYPE, shipped" { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.bold: true }
h2: "REL, building" { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.bold: true }

q1: "the container" { style.fill: "#f1f3f4"; style.font-color: "#202124" }
a1: "rel file(repo, at)" { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
b1: "rel orchard { tree, fruit }" { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b" }

q2: "a member" { style.fill: "#f1f3f4"; style.font-color: "#202124" }
a2: "a COLUMN" { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
b2: "a CHILD REL" { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b" }

q3: "who tells them apart" { style.fill: "#f1f3f4"; style.font-color: "#202124" }
a3: "body_tag(Id, page)" { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
b3: "the kind column" { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b" }

q4: "the flat name" { style.fill: "#f1f3f4"; style.font-color: "#202124" }
a4: "body + page = body_page" { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
b4: "orchard + tree = orchard__tree" { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b" }

q5: "on collision" { style.fill: "#f1f3f4"; style.font-color: "#202124" }
a5: "THROW, shipped" { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
b5: "THROW, NOT BUILT" { style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.bold: true }
```

Only the last row differs. One file cannot declare `page` twice, so the enum
case never needed a disambiguator. Two FILES can both declare `tree`. That gap
is one number per file, and nothing more.

## 3. four hashes, four jobs

One hash cannot do this. Four can, and each breaks exactly one thing.

```d2
direction: down

id: "h_id = H(file, name, arity)\n\nthe NAME. never moves." {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"
}
schema: "h_schema = H(columns, types, key)\n\ndiffers -> DROP + CREATE. rows LOST." {
  style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.stroke: "#c5221f"
}
rule: "h_rule = H(the rule bodies)\n\ndiffers -> keep table, redo rows." {
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"
}
rows: "h_rows = H(the rows)\n\ndiffers -> wake readers.\nequal -> red STOPS." {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}

id -> schema: "breaks more" { style.stroke: "#5f6368" }
schema -> rule: "breaks less" { style.stroke: "#5f6368" }
rule -> rows: "breaks least" { style.stroke: "#5f6368" }
```

## 4. the trap

You asked for the hash to be in the name. Watch.

```d2
direction: down

before: "rel orchard.tree(tree_id, species)" {
  style.fill: "#f6f8fa"; style.font-color: "#24292f"
}
t1: "table orchard__tree__f9fc8ea9\n\n40,000 rows" {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"
}
edit: "you add one column: picked" {
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"; shape: hexagon
}
t2: "table orchard__tree__3b1c02aa\n\n0 rows" {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}
dead: "orchard__tree__f9fc8ea9\n\nstill there. 40,000 rows.\nno name anyone will ask for again." {
  style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.stroke: "#c5221f"
}

before -> t1: "compile"
t1 -> edit
edit -> t2: "recompile: the hash MOVED, so the NAME moved" { style.stroke: "#c5221f"; style.stroke-width: 2 }
t1 -> dead: "orphaned" { style.stroke: "#c5221f"; style.stroke-width: 2; style.stroke-dash: 3 }
```

Every column edit becomes a rename. A rename in SQLite is a new empty table.

Fix: hash goes in a COLUMN, never in the name.

```d2
direction: right
name: "table name\norchard__tree\n\nboring. stable. forever." {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"
}
col: "__rel.h_schema\n3b1c02aa\n\nchanges freely.\ntells you to rebuild." {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}
rust: "Rust does this.\nHashes the CRATE.\nNever the function body." {
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"
}
name -> col: "the pair" { style.stroke: "#5f6368" }
col -> rust: "same reason" { style.stroke: "#5f6368" }
```

## 5. file changes, five verdicts

Save file B. Compile file B alone. Look each rel up by `h_id`.

```d2
direction: down

look: "look up by h_id" {
  shape: hexagon
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"; style.stroke-width: 3
}

new: "NEW\n\nmiss\nCREATE + seed\nreaders RED" {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}
reshaped: "RESHAPED\n\nh_schema differs\nDROP + CREATE\nrows LOST\nreaders RED" {
  style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.stroke: "#c5221f"
}
rebodied: "REBODIED\n\nh_rule differs\nkeep table\nDELETE + recompute\nthen check h_rows" {
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"
}
green: "GREEN\n\nall four equal\nZERO work\nno DDL, no rows, no wake" {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"; style.stroke-width: 3
}
gone: "GONE\n\nwas there, absent now\nrefcount -> 0\nDROP\nreaders RED" {
  style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.stroke: "#c5221f"
}

look -> new
look -> reshaped
look -> rebodied
look -> green: "the one that matters" { style.stroke: "#137333"; style.stroke-width: 3 }
look -> gone
```

Save a file, change nothing real, and the server does nothing at all.

## 6. how far does red go

Red stops the moment recomputing gives the same answer.

```d2
direction: down

tree: "orchard__tree\nRESHAPED\n\nyou edited this" {
  style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.stroke: "#c5221f"; style.stroke-width: 2
}
fruit: "orchard__fruit\nRED\n\nrows differ" {
  style.fill: "#fce8e6"; style.font-color: "#7f1d1b"; style.stroke: "#c5221f"; style.stroke-width: 2
}
ripe: "ripe\nRED, recomputed\n\nh_rows came out IDENTICAL" {
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"; style.stroke-width: 2
}
report: "report\n\nNEVER RUNS" {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"; style.stroke-width: 3
}
cyc: "the one exception: cycles\n\na red rel on a loop\nturns the WHOLE loop red.\nhalf a loop cannot be green." {
  style.fill: "#f1f3f4"; style.font-color: "#202124"; style.stroke: "#5f6368"; style.stroke-dash: 4
}

tree -> fruit: "hop 1: recompute, differs, continue" { style.stroke: "#c5221f"; style.stroke-width: 2 }
fruit -> ripe: "hop 2: recompute, differs, continue" { style.stroke: "#c5221f"; style.stroke-width: 2 }
ripe -> report: "hop 3: STOP" { style.stroke: "#137333"; style.stroke-width: 4 }
ripe -> cyc: "unless" { style.stroke: "#5f6368"; style.stroke-dash: 3 }
```

## 7. what everyone else does

Nobody carries a flat string as the identity. Everybody carries a PAIR inside
and flattens only at the exit.

```d2
direction: right

pairs: inside the compiler: a PAIR {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"
  r: "Rust  (crate, item)" { style.fill: "#ffffff"; style.font-color: "#0b3d6b" }
  g: "Go  (pkg, name)" { style.fill: "#ffffff"; style.font-color: "#0b3d6b" }
  s: "SQLite  (schema, table)" { style.fill: "#ffffff"; style.font-color: "#0b3d6b" }
  y: "you  (parent_id, local_name)" { style.fill: "#fef7e0"; style.font-color: "#5c3200" }
}

flat: at the exit: ONE STRING {
  style.fill: "#f6f8fa"; style.font-color: "#24292f"
  r: "_RNvCs7qp..7mycrate7example" { style.fill: "#ffffff"; style.font-color: "#24292f" }
  g: "github.com/x/y.Foo" { style.fill: "#ffffff"; style.font-color: "#24292f" }
  s: "other.t" { style.fill: "#ffffff"; style.font-color: "#24292f" }
  y: "orchard__tree" { style.fill: "#fef7e0"; style.font-color: "#5c3200" }
}

pairs.r -> flat.r
pairs.g -> flat.g
pairs.s -> flat.s
pairs.y -> flat.y: "the only new part" { style.stroke: "#b06000"; style.stroke-width: 3 }
```

Who hashes what:

```d2
grid-rows: 6
grid-columns: 2
grid-gap: 8

h0: "system" { style.fill: "#f1f3f4"; style.font-color: "#202124"; style.bold: true }
h1: "hashes what" { style.fill: "#f1f3f4"; style.font-color: "#202124"; style.bold: true }

r0: Rust { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b" }
r1: "the CRATE. never the item body." { style.fill: "#e8f0fe"; style.font-color: "#0b3d6b" }

g0: Go { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
g1: "nothing. escapes bad bytes, then errors on a clash." { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }

p0: Python { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
p1: "nothing. the dotted string IS the key." { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }

s0: SQLite { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }
s1: "nothing. two names, two slots, both live." { style.fill: "#e6f4ea"; style.font-color: "#0b3d1f" }

d0: "dl v5" { style.fill: "#fce8e6"; style.font-color: "#7f1d1b" }
d1: "nothing, and no namespace. last writer silently wins." { style.fill: "#fce8e6"; style.font-color: "#7f1d1b" }
```

Zero of the five hash content into a name. Go's own comment says it plainly: a
symbol is "an object name in a segmented (pkg, name) namespace." Segmented.
Two parts, joined only at the linker.

## 8. build order

```d2
direction: down

h1: "h1  rename __catalog_rel -> __rel" {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"; style.stroke-width: 2
}
h2: "h2  put arity in the name" {
  style.fill: "#e6f4ea"; style.font-color: "#0b3d1f"; style.stroke: "#137333"; style.stroke-width: 2
}
h3: "h3  dotted name through the parser" {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}
h4: "h4  module_id + h_id, one hash per FILE" {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}
h5: "h5  h_schema + h_rule + the five verdicts" {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}
h6: "h6  h_rows + red/green" {
  style.fill: "#e8f0fe"; style.font-color: "#0b3d6b"; style.stroke: "#1967d2"
}

now: "no decision needed.\nfixes a real bug.\nland today." {
  style.fill: "#fef7e0"; style.font-color: "#5c3200"; style.stroke: "#b06000"; style.stroke-width: 3
}
blocked: "the mangler has NO INPUT\nuntil h3 exists." {
  style.fill: "#f1f3f4"; style.font-color: "#202124"; style.stroke: "#5f6368"; style.stroke-dash: 4
}

h1 -> h2 -> h3 -> h4 -> h5 -> h6
h2 -> now: "" { style.stroke: "#b06000"; style.stroke-width: 2 }
h1 -> now: "" { style.stroke: "#b06000"; style.stroke-width: 2 }
h3 -> blocked: "" { style.stroke: "#5f6368"; style.stroke-dash: 3 }
```

Two things worth knowing before you start.

**h2 fixes something already broken.** `table_name` throws the arity away.
Declare `edge/2` and `edge/3` in one program and the compiler emits
`CREATE TABLE "edge"` twice. No fixture does it, so it has never fired.

**h5 fixes something worse.** Swapping a program under a running server re-runs
the DDL and IGNORES every "already exists" error. A reshaped rel keeps its OLD
table shape and nothing tells you.

## the three questions that need you

1. Which hash function? xxh3 is what the Go TypeScript compiler uses.
2. `__` for the module join, so `orchard__tree` never looks like the enum join
   `body_page`. Yes, or pick another separator.
3. When a reshape would DELETE rows under a running server: just do it, or
   refuse and make you pass a flag?
