# Anti-Unification for Rule Inference

## What it is

Anti-unification is the inverse of pattern matching.

- Pattern matching: given a pattern and a value, find bindings that make them equal.
- Anti-unification: given two values, find the most specific pattern both are instances of.

Where two trees agree, the result keeps the literal structure.
Where they differ, it introduces a variable (hole).

```
lgg("foo", "foo") = "foo"        -- same: keep literal
lgg("foo", "bar") = $VAR         -- differ: introduce hole
lgg([1,2,3],[1,2,4]) = [1,2,$X]  -- agree on prefix, hole at tail
```

`lgg` = least general generalization. The most specific pattern that both values are instances of.


## Applied to a cross-repo code index

Every ref row in the index has a full path:

```
{branch} / {repo} / {rel_path} / {node_path...} / {ref_kind} / {value}
```

plus optional:
- `parent_key` -- sibling key (e.g. dep name for a dep version)
- `file_ext`   -- determines parse mode (yaml/json/toml/source)
- `node_path`  -- sequence of yaml keys or AST node kinds leading to this leaf

Two ref rows represent "the same kind of thing" when the user says so, or when
a query returns them together. Anti-unification over their paths produces the
selector rule that would extract both.


## The algorithm

Given two ref rows R1 and R2:

```
P1 = [branch1, repo1, rel_path1, ...nodes1..., ref_kind1, value1]
P2 = [branch2, repo2, rel_path2, ...nodes2..., ref_kind2, value2]

function lgg(p1, p2):
  if both empty: return []
  head1, tail1 = p1[0], p1[1:]
  head2, tail2 = p2[0], p2[1:]
  if head1 == head2:
    return [head1] + lgg(tail1, tail2)  -- agree: keep literal
  else:
    return [$VAR]                        -- differ: hole swallows rest
```


## Segment equality rules

Each path segment type has its own comparison:

| Segment    | Match condition          | Result                        |
|------------|--------------------------|-------------------------------|
| branch     | exact match              | literal                       |
|            | differ                   | `*`                           |
| repo       | exact match              | literal                       |
|            | differ                   | `*`                           |
| rel_path   | same file                | literal                       |
|            | same filename only       | `**/filename`                 |
|            | same extension only      | `*.ext`                       |
|            | differ                   | `*`                           |
| yaml key   | same key                 | literal key name              |
|            | differ                   | `*` (any key)                 |
| array idx  | any two indices          | `[*]` (always generalizes)    |
| node kind  | same tree-sitter kind    | literal                       |
|            | differ                   | `*`                           |
| ref_kind   | same                     | emit as `ref-kind: X`         |
|            | differ                   | emit `ref-kind: {X,Y}`        |
| value      | same                     | literal constraint `[value="..."]` |
|            | both semver              | typed hole `$VERSION`         |
|            | both repo names          | typed hole `$REPO`            |
|            | both URL paths           | typed hole `$API_PATH`        |
|            | both dotted FQNs         | typed hole `$FQN`             |
|            | differ                   | unconstrained `$STRING`       |


## Concrete example

Two ref rows:

```
R1: main / api-contracts / artifacts-cache/v1/openapi.json
    yaml path: paths > /v1/widgets/_aggregate > post > operationId
    ref_kind:  api_operation
    value:     widgetsAggregate
    parent_key: POST /v1/widgets/_aggregate

R2: main / generated-clients / packages/widgets/openapi.json
    yaml path: paths > /v1/widgets/_aggregate > post > operationId
    ref_kind:  api_operation
    value:     widgetsAggregate
    parent_key: POST /v1/widgets/_aggregate
```

lgg step by step:

```
branch:      main == main                       -> main
repo:        api-contracts != generated-clients -> *
rel_path:    both end in openapi.json           -> **/openapi.json
paths:       paths == paths                     -> paths
url key:     same url, different repos          -> [*]
post:        post == post                       -> post
operationId: same                               -> operationId
ref_kind:    api_operation == api_operation     -> ref-kind: api_operation
parent_key:  same shape (METHOD /path)          -> parent-key: key-path
```

Result:

```
main > * > **/openapi.json > paths > [*] > post > operationId {
  ref-kind: api_operation;
  parent-key: key-path;
}
```

Which is the correct generic rule for "extract all POST operationIds from
openapi specs on main." Derived from two examples with no hand-authoring.


## N examples

With N examples, iterate pairwise:

```
rule = lgg(ex1, ex2)
rule = lgg(rule, ex3)
rule = lgg(rule, ex4)
...
```

Each additional example can only introduce more holes, never more specificity.
The rule converges to the correct generalization across all examples.

Negative examples work in reverse: if the inferred rule matches unwanted results,
find where the unwanted result's path diverges from the positive examples and add
a constraint at that segment.


## Typed holes

When two values differ, inspect both before defaulting to `$STRING`:

| Both values match           | Typed hole    |
|-----------------------------|---------------|
| `/^\d+\.\d+\.\d+/`         | `$VERSION`    |
| in the repo name index      | `$REPO_NAME`  |
| file paths                  | `$FILE_PATH`  |
| dotted FQNs                 | `$FQN`        |
| URL-shaped                  | `$API_PATH`   |
| otherwise                   | `$STRING`     |

Typed holes generate tighter rules and automatically drive properties like
`scan-repos: value` and `value: strip-version-operators`.


## Integration with existing index

Anti-unification needs only the stored ref rows. No re-reading source files:

```sql
SELECT r.ref_kind, s.value, f.rel_path, repo.name,
       pk.value as parent_key, f.ext
FROM refs r
JOIN strings s ON r.string_id = s.id
JOIN files f ON r.file_id = f.id
JOIN repos repo ON f.repo_id = repo.id
LEFT JOIN strings pk ON r.parent_key_string_id = pk.id
WHERE r.id IN (id_1, id_2)
```

Feed those two rows to lgg. Output is a rule ready to add to the rules config.


## The closed loop

```
rules -> scan -> index -> query -> results
                                      |
                              user marks two results
                              as "same kind of thing"
                                      |
                              lgg over their index paths
                                      |
                              new rule added to rules file
                                      |
                              next scan picks it up
```
