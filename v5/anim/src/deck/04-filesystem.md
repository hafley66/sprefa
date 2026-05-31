# Filesystem

## a project takes shape

The right panel is a new lens: a file tree, rendered like a real explorer. Step forward and watch files animate in. Same keyed-FLIP idea as the code tween, but the key is the **path**.

```fs
Cargo.toml
src/main.rs
src/lib.rs
```

## add the SCC pass

A new file appears and slides in; the rows already present hold their place. Nothing re-lays-out with a jump.

```fs
Cargo.toml
src/main.rs
src/lib.rs
src/scc.rs +
```

## and the engine

Another file lands. The tree grows, existing rows FLIP smoothly to their new positions.

```fs
Cargo.toml
src/main.rs
src/lib.rs
src/engine.rs +
src/scc.rs
```

## reorganize into a module

Move the graph code into `src/graph/`. The old paths fade out, the new folder and its files fade in, and the untouched files slide to absorb the change. A move is just a remove + add on keyed rows.

```fs
Cargo.toml
src/main.rs
src/lib.rs
src/graph/engine.rs ~
src/graph/scc.rs *
```

## one cursor, many lenses

This is the spine: a frame is a cursor, and a panel is a lens that reads it. Code, graph, and now filesystem all animate by the same rule — match keys, slide what moved, fade what changed. Git history and an editor view are just two more lenses on the same bus. See [[03-relations]] for the graph lens.
