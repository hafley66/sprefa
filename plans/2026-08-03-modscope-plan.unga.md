# The plan, in plain words

This is the only doc you need to read. No citations, no jargon pile. Short sentences.
Pictures made of text.

## The one-sentence version

A dl file is itself a scope, a rel can hold other rels inside it, and nothing anywhere
runs until somebody asks a question.

## What we found out first (the "or am I wrong" part)

You asked whether a question line in a file is already the subscribe button. Honest
answer: not yet. Half of your picture is true today, half is not.

True today:

- Every table gets created up front, whether or not anyone ever looks at it.

Not true today:

- A question line like `? tree(Id, Species).` is written down, shipped into the
  generated code as a little note saying "someone asked about tree", and then nothing
  reads that note. Its arguments get thrown away on the way out.
- The engine recomputes every rule on every tick, asked or not.
- Nothing is lazy. There is no demand machinery keyed off questions.

So the plan treats your picture as the target, and the work is: make the question line
actually be the subscribe button.

One spelling fix: the real surface is a single `?`, not `?-`. That is what the parser
reads today, and the plan keeps it.

## Idea 1: the file is the first scope

Today a file is just a bag of declarations. The plan says: a file's body is the body of
an invisible rel with no columns. Everything you declare at the top level of the file
hangs off that invisible rel, one dot away.

```
file orchard.dl6                    the invisible file rel: "orchard"
+------------------------+
| rel tree(...)          |   ->  orchard.tree
| rel pick_event(...)    |   ->  orchard.pick_event
| rel picked_by_ada(...) |   ->  orchard.picked_by_ada
+------------------------+
```

"One dot away" is the whole rule. If a thing sits under another thing, you reach it
with a dot. A column of tree is `orchard.tree.tree_id`. No exceptions, no second
punctuation.

Names work like nested rooms. You say a name, the system looks in the room you are in,
then the next room out, then the hallway. Nearest match wins. If an inner room's name
hides an outer one, you can still get the outer one by spelling its whole path from the
front door.

Two files in one compile are two rooms off the same hallway. File A can only touch file
B's things by full path, like `b.tree`. There is no import statement at all. Naming the
thing across the hall IS the asking.

## Idea 2: a rel can hold other rels (the metaclass bit)

You wanted "ruby metaclasses but in the types". Here is the plain version.

There is one big family tree of everything the program declares. Every rel gets one row
in that tree. That row does double duty:

```
            one row in the tree
           /                  \
   "the instance"          "the type"
   parent, children,       its columns hang
   its name                off the same row
```

In Ruby an object has a class and the class is also an object. Here a rel has a row and
the row is also where the shape lives. There is no separate module object, no second
meta tree. A "module" is just a rel with no columns that has children. That is the
whole trick.

The tree lives in two ordinary tables that get written once at compile time:

```
who is whose child:          (id, parent, name, kind)
who is which instance:       (instance, rel, args)
```

The dotted path of anything is computed by walking up the parent links. It is never
stored. So renaming a thing edits one row and every path under it still works.

Nesting looks like this:

```
rel orchard {
  rel tree(tree_id: int, species: text).
  rel picked(tree_id: int, picker: text).
}
```

This is sugar. Before anything else runs, the block is flattened into ordinary
declarations plus a few rows in the tree recording who sat inside whom. Everything
after that step cannot tell the block ever existed.

## Idea 3: asking is the only import, and laziness is the default

Compile does everything it can without running anything: all tables, all checks, all
type work. Then it stops. Nothing ticks.

A question line in the file is the subscribe button:

```
? picked(TreeId, Picker).
```

That one line does three jobs:

1. It says "keep this one alive".
2. It pulls demand backward through the rules: picked needs tree, tree needs whatever
   tree needs, and so on, across files if the paths cross files.
3. Anything no question can reach never runs. Its table exists, empty and quiet.

```
question
   |
   v
picked  ----needs---->  tree  ----needs---->  (arrivals from the world)

   orphan_rel  <---- nothing points here ---- never runs, ever
```

Want something always on? Ask about it forever. That is all "eager" means. There is an
`eager` spelling, but it is just sugar that turns into an ordinary standing question.
One mechanism, never two.

## Idea 4: the late joiner and the log

Edge rules (the `<+` arrow) are the log plane: things that happened, appended, never
taken back. What happens when someone subscribes late?

The recommended answer (the choice is still yours, see the list at the end): they get
what the store still holds, then everything new from that moment on.

```
time ------------------------------------------------>
        |---- log so far ----|---- new stuff ---->
                             ^
                        you subscribe here
        you get: what the store kept + the live tail
```

What the store kept depends on the retention the author chose:

- keep everything: the late joiner replays the whole history.
- keep last N: the late joiner gets the last N. Older occurrences are gone for real.
- keyed: the late joiner gets the current winner per key.

Nobody rebuilds what retention already threw away. If you want late joiners to see far
back, you pay for that with a bigger retention, in writing, in the program.

## The world knocking: typed event sources

Things outside the program already arrive as rows: timers, file watchers, shell
commands. The plan adds one general spelling for "a new kind of outside thing",
with real column types like any table. The example: a git pre-commit hook.

```
event pre_commit(repo_path: text, changed_file: text, diff_summary: text)
  via sh("git", ["hook-bridge", "pre-commit"]).
```

Same rules as everything else. The table exists from the start. The little bridge
program that feeds it only runs while somebody is asking. Rules read it like any
other table. In this example the bridge, once started, stays up and feeds everyone
who asks -- but that staying-up is this example's shape, not a general rule. The
general rule is one of the open choices below. The heavy engine work for all of
this belongs to a separate lane that is studying laziness against the existing
code; this plan only fixes what the declaration looks like.

## The work, in order

Five steps, smallest first. Each one lands on its own with tests.

1. Write the family-tree tables for today's flat programs, and ship a test program that
   asks questions about its own tree.
2. Give every file its row and teach name lookup the room-by-room walk with the
   full-path escape hatch.
3. Add the nesting block sugar that flattens into plain declarations plus tree rows.
4. Wire up demand: questions keep their arguments, the engine computes only what
   questions reach, and a question in a served program becomes a live stream that
   pushes new answers as they happen. Add the `eager` sugar here. Make the checker
   actually check questions.
5. Allow dotted rule heads (`a.b(x) <- ...`) so one file can contribute rules to a rel
   another file declared, and allow compiling several files together.

## The choices I did not make (you do)

- How a file's row is named. I say: the file's stem, like `orchard`. Cost: two files
  with the same stem cannot compile together yet.
- Keep `?` and not add `?-`. Cost: your muscle memory loses.
- Questions keep their arguments (so they can filter and project). Cost: a question
  becomes a small generated rel under the hood.
- Laziness is per-rel for now, not per-column. Cost: a little less lazy than the full
  dream; the per-column kind arrives later with the fancier nesting.
- Questions push answers to you, not just answer when polled. Cost: one new endpoint in
  the server.
- What a late joiner sees. My pick: what the store kept, then the live tail. The
  other options: live-only from the moment you join, or full history no matter what
  retention threw away. Cost of my pick: replay is only as long as retention.
- What the last goodbye does. When the last question over some data goes away,
  either the machinery upstream shuts down (and a later question starts it over from
  what the store kept), or it stays running and warm. My pick: one shared pipeline,
  shut down when the last watcher leaves. Cost: a question asked again after a full
  shutdown replays only what the store kept. (The always-on bridge in the git hook
  example is that one example's shape, not the general rule.)
- What happens to events that arrive before anyone asks. Drop them, hold them in
  memory, or write them down always. My pick: drop them -- nothing runs before the
  first question, so there is nothing to catch them. Cost: early events are gone
  unless the author pays for history with a retention spelling.
- `eager` is a real word that turns into a question. Cost: one more word in the
  language.
- Questions get typechecked like everything else. Cost: broken questions now fail the
  build instead of sitting silent.
