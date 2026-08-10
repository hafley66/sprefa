# DD payload grain

## The term

One rule can create several graph nodes.

```text
left ----> join_1_1 ----> map_1 ----> matched
right --->/                 |
                            SQL statements
```

The join has a pointer to `map_1`.

```text
join_1_1: sqlite([left/2, matched/1, right/2], owner(map_1))
map_1:    sqlite([left/2, matched/1, right/2], [SQL statements])
```

## SQLite tick

The SQLite runner walks nodes. It runs SQL only when it reaches a node with a statement list. Owner nodes are skipped for SQL.

```text
map_1      run SQL
join_1_1   follow owner(map_1), run no SQL
```

That keeps a join rule at one SQL execution per tick.

## RAM kernel

The RAM kernel reads map, join, reduce, filter, arrangements, and wires. It ignores the SQLite field. The SQL placement does not change its compilation input.

## Size

```text
join golden before: 4,804 bytes
join golden after:  2,899 bytes
```

The SQL text now occurs once. The second node holds a short owner pointer.

## Future cases

```text
several rules, different heads      one SQL owner per rule map
same body relation in two rules     each rule map owns its SQL
filter / reduce / iterate            owner pointer to that rule map
same head in several clauses         choose one owner for the grouped head SQL
```

The last case needs a later emitter amendment because current level SQL groups adjacent clauses with the same head.
