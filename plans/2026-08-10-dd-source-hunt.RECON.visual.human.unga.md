# Why the Retraction Times Differ

## Context

The benchmark removes one root from a graph and waits until every reachable node has its final alive or dead state.

At the 960,000-node scale, the correct SQLite paths take about 1.7 seconds when the whole database is in memory. The dataflow path takes about 0.17 seconds. All paths return 800,002 survivors with the same input and output fingerprints.

Moving SQLite from a file to a memory database saves between 3.2 and 6.1 percent. Most of the time remains after file access has been removed from the run.

## The Two Execution Shapes

SQLite uses two graph walks:

```text
remove root
    |
    v
walk outward and mark the whole affected cone dead
    |
    v
look for cone nodes that still have a live parent
    |
    v
walk outward again and bring supported nodes back
    |
    v
final answer
```

The first walk accounts for about 52 percent of the traced run. The second walk accounts for about 48 percent.

The dataflow path carries changes through one continuing calculation:

```text
root change: -1
    |
    v
join changed parents with stored edges
    |
    v
add and cancel support changes for each child
    |
    v
emit a child change only when alive status crosses zero
    |
    +----------------------+
    | more child changes?  |
    +----------+-----------+
               | yes
               v
          next inner round
               |
               +------ back to the join

               no
               |
               v
          final answer
```

Cycles may require several inner rounds. The calculation carries positive and negative changes until they cancel or settle. It does not first declare the whole cone dead and then reconstruct it.

## Storage Work

SQLite already keeps its temporary frontier tables in memory. Each graph round still changes balanced-tree tables:

```text
clear next frontier
insert next frontier rows
count them
update alive flags
insert affected-cone rows
repeat
```

The dataflow engine stores sorted, read-only batches:

```text
new small batch
    |
    v
sort equal keys together
    |
    v
add their signed counts
    |
    v
drop zero totals
    |
    v
merge with a similar-size batch in bounded pieces
```

This layout stays in memory and is designed around streams of changes. It avoids clearing and refilling one mutable frontier table for each graph level.

## Suspect Results

```text
CONFIRMED  two graph passes versus one signed fixed-point calculation
CONFIRMED  mutable balanced-tree work versus sorted immutable batches
KILLED     repeated full joins in this benchmark
KILLED     SCC analysis in the measured SQLite path
```

The SQLite joins already start from the changed frontier and use an edge index. The function named for SCC handling goes directly to the two-pass cone algorithm. No SCC partition is computed during the measured operation.

## Transfer Forks

### 1. Signed changes with inner rounds

```text
outer update T
  round 0: accept root changes
  round 1: propagate resulting child changes
  round 2: propagate the next child changes
  ...
  empty round: close T
```

Keep a support count for each reachable item. Emit an alive or dead change only when that count crosses zero. This removes the separate rebuild pass. The measured rebuild pass is about 0.81 seconds, or 48 percent of the traced SQLite run.

The open choice is whether inner time, feedback, and threshold state are hidden runtime rules or visible plan fields.

### 2. Append batches, then consolidate

```text
update batch A -----+
                    +--> sorted merged batch --> stable arrangement
update batch B -----+
```

Store sealed update batches and merge them by size. Equal keys add their signed counts. Zero totals disappear. This work overlaps both SQLite graph passes, so the current measurements do not provide a separate time estimate.

The open choice is whether every arrangement uses one backend policy or the plan can select batch and compaction behavior.

### 3. Reuse indexed join history for general rules

```text
new left  x stored right
stored left x new right
new left  x new right, counted once
```

The reachability benchmark already has the equivalent small-frontier join shape in SQLite. Its measured opportunity here is zero. General rules with changes on both inputs still need an ownership rule for the new-by-new term.

### 4. Cancel repeated signed changes early

```text
key A  +1
key A  +1
key A  -1
---------
key A  +1
```

Cancellation should occur before another join or feedback round whenever the time boundary permits it. The current benchmark deduplicates boolean frontier membership, so this effect has no isolated timing result.

## Decisions

No language or intermediate-plan choice is made in this recon.

The current plan vocabulary already has signed arrangements, delta connections, joins, iteration, and consolidation. Runtime conventions can supply the missing timing and storage rules. Explicit plan fields are needed if those rules must be inspected, tested independently, or selected per operator.

## Verification

The probe used one release binary, the same generated graph parameters, one process per engine, and matching result fingerprints. File-backed and memory-backed SQLite runs used the same algorithm. The timing window covered retraction only.

The traced memory run assigned 871.04 milliseconds to over-delete work and 807.70 milliseconds to rederive work. Logged database statements covered 99.79 percent of total wall time.

## Staffing

One recon lane performed the source audit, the matched memory probe, and the transfer sketches. No dependency version changed. The remaining work is a human selection among the plan and runtime forks above.
