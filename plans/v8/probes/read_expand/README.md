# read phase: colon rewrite walks every reader node term

## Input

`0_colon_rewrite_shapes.dl7`. `dl8 compile` gives zero diagnostics; the
settled `compiler_rows` hold `files("r", vec["f0", "f1"])` and
`size("r", 2)`.

```lisp
(files: (* (run: str) (list: vec)))
(files "r" (vec "f0" "f1"))
(size : (* (run: str) (len: int)))
(<- (size ?Run ?Len) (files ?Run ?List) (vec.len ?List ?Len))
```

## Reader output

`dl8 read plans/v8/probes/read_expand/0_colon_rewrite_shapes.dl7`. `rnN` is
`reader_node(path, N)`.

Line 1 `(files: (* (run: str) (list: vec)))`:

```js
node(rn0, form([
  node(rn1, atom('files:')),
  node(rn2, form([
    node(rn3, atom('*')),
    node(rn4, form([
      node(rn5, atom('run:')),
      node(rn6, atom('str'))])),
    node(rn7, form([
      node(rn8, atom('list:')),
      node(rn9, atom('vec'))]))]))]))
```

Line 2 `(files "r" (vec "f0" "f1"))`:

```js
node(rn10, form([
  node(rn11, atom('files')),
  node(rn12, literal("r")),
  node(rn13, form([
    node(rn14, atom('vec')),
    node(rn15, literal("f0")),
    node(rn16, literal("f1"))]))]))
```

Line 3 `(size : (* (run: str) (len: int)))`:

```js
node(rn17, form([
  node(rn18, atom('size')),
  node(rn19, atom(':')),
  node(rn20, form([
    node(rn21, atom('*')),
    node(rn22, form([
      node(rn23, atom('run:')),
      node(rn24, atom('str'))])),
    node(rn25, form([
      node(rn26, atom('len:')),
      node(rn27, atom('int'))]))]))]))
```

Line 4 `(<- (size ?Run ?Len) (files ?Run ?List) (vec.len ?List ?Len))`:

```js
node(rn28, form([
  node(rn29, atom('<-')),
  node(rn30, form([
    node(rn31, atom('size')),
    node(rn32, variable(Run)),
    node(rn33, variable(Len))])),
  node(rn34, form([
    node(rn35, atom('files')),
    node(rn36, variable(Run)),
    node(rn37, variable(List))])),
  node(rn38, form([
    node(rn39, form([
      node(rn40, atom('.')),
      node(rn41, atom('vec')),
      node(rn42, atom('len'))])),
    node(rn43, variable(List)),
    node(rn44, variable(Len))]))]))
```

## Words

| word | shape | site |
|---|---|---|
| reader node term | `node(SourceId, Payload)` | `src/_0_read/expand.rs:102` |
| reader payload | `form([...])`, `atom(A)`, `literal(V)`, `variable(...)` | `expand.rs:105` |
| reader form | `form(list)` | `expand.rs:106` |
| shape tree | a reader payload with every `SourceId` removed | `node_tree`, `expand.rs:209` |
| colon rewrite | the two rules in `rewrite()` | `expand.rs:166` |

## Which rule each reader form hits

| reader form | rule | rewritten to |
|---|---|---|
| `(files: (* ...))` line 1 | suffix colon, `:186` | `(: files (* ...))` |
| `(run: str)`, `(list: vec)` line 1 | suffix colon, `:186` | `(: run str)`, `(: list vec)` |
| `(files "r" ...)` line 2 | none, head has no trailing `:` (`:182`) | unchanged |
| `(vec "f0" "f1")` line 2 | none (`:182`) | unchanged |
| `(size : (* ...))` line 3 | infix colon, `:171` | `(: size (* ...))` |
| `(run: str)`, `(len: int)` line 3 | suffix colon, `:186` | `(: run str)`, `(: len int)` |
| `(<- ...)` line 4 and its goals | none (`:182`) | unchanged |

## One input per conditional

| fn | condition | site | input | result |
|---|---|---|---|---|
| `expand_node_children` | payload not `form` | `:107` | `node(rn11, atom('files'))` | returned as is |
| | loop over children | `:110` | `rn13` `(vec "f0" "f1")` | each child expanded, then `u.list` rebuild |
| | child `Err` | `:115` | `(: : y)` as a child, not in the file | cycle diagnostic bubbles up |
| `node_tree` | payload not `form` | `:214` | `node(rn12, literal("r"))` | `literal("r")` |
| | recurse | `:220` | `rn25` `(len: int)` | `form([atom('len:'), atom(int)])` |
| `rewrite` | shape not `form` | `:167` | `atom(files)` | None |
| | `children[1] == atom(':')` | `:171` | `(size : (* ...))` | `(: size (* ...))` |
| | no first child | `:177` | `()`, not in the file | None |
| | head not atom | `:178` | `("r" x)`, not in the file | None |
| | head lacks trailing `:` | `:182` | `(vec "f0" "f1")` | None |
| | stem empty | `:183` | `(: size (* ...))` | None, fixpoint ends |
| | suffix colon | `:186` | `(files: x)` | `(: files x)` |
| `rewrite_fixpoint` | `rewrite` None | `:129` | `(vec "f0" "f1")` | node unchanged |
| | replacement already seen | `:137` | `(: : y)`, not in the file | `expansion_cycle([infix_colon])` |
| | fires | `:149` | `(files: x)` | wave 1 `(: files x)`, wave 2 stem empty, stop |

`expand_node_children` and `node_tree` also return early on a term that is
not `node(...)` (`:103`, `:210`); no reader output takes that branch.

## The 1M cost

Probe seed `(files "r" (vec "f0" ... "f1048575"))`. Every reader node term
goes `:110` loop, then `:182` miss, then `:129` return, and every `:182` miss
paid for a full `node_tree` first.

```js
expandReaderNode(files)
  expandReaderChildren(files)
    expandReaderNode(vec)
      expandReaderChildren(vec)
        for (item of 1M) expandReaderNode(item)    // 1M visits, all unchanged
        u.list(children)                           // :112 rebuilds the 1M list
      shapeTree(vec)                               // :209 1M walk + 1M intern
      colonRewrite(shape)                          // reads children[0..2], no match
    u.list([vec])                                  // :112 rebuilds the files list
  shapeTree(files)                                 // recurses into vec, 1M walk again
  colonRewrite(shape)                              // no match
```

Mac sample at 1M (`sample_tree.py` in `../list_ladder`): `expand_dl7` 780,
of which `:112` 205 and `node_tree` about 366; parse 554; reify 388.

## Fixes

Shallow test: both rules read only `children[0]` and `children[1]`.

```js
function expandReaderNode(node) {
  const [first, second] = node.children
  const fires = isAtom(second, ':') || atomEndsWithColon(first)
  if (!fires) return expandChildrenInPlace(node)   // no shapeTree, no rebuild
  return rewriteFixpoint(node, shapeTree(node))
}
```

Unchanged children: return the input reader node term when every child came
back identical, skipping `:112`.

Flat arena (larger): read into a `Vec` of arena entries, filter the colon
sites, run the fixpoint on that queue, intern once at reify.

| fix | 1M passes left |
|---|---|
| today | 4 |
| unchanged children | 3 |
| shallow test + unchanged children | 1 |
