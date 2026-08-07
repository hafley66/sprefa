# Loading programs into a running engine, in plain words

Today the server takes ONE file, and a changed program quietly keeps the old tables. Three doors fix that.

## Door A: more than one file

    today:   you ---- one big file ----> server

    after:   main.dl6
               |  use "orchard.dl6".
               +--> orchard.dl6
               |       |  use "shared.dl6".
               |       +--> shared.dl6
               +--> harvest.dl6
                       |  use "shared.dl6".
                       +--> shared.dl6   (already read, skipped)

             all of it becomes ONE program before the server ever sees it

Two files can each own a rel called `tree`. They stay apart because the name gets its file glued to the front: `orchard.tree` becomes `orchard__tree`.

- a file reached twice is read once; a file that reaches itself is an error naming the loop
- same rel, same columns, two files: they merge. Different columns: an error naming both files
- a missing file is an error listing every place it looked

## Door B: the reload that actually reloads

    today:   new program ---> "CREATE TABLE harvest(...)"
                                |
                                v
                              SQLite: "already exists"
                                |
                                v
                              server shrugs, keeps the OLD table and the OLD rows

You add a column, the server answers 200 OK, and the table never changed. Silent.

After, the server compares fingerprints. Every rel already carries three: who it is, what shape it has, and what rule fills it. Old against new gives one of five answers.

    rel is new             ->  CREATE
    shape changed          ->  DROP, then CREATE
    shape same, rule new   ->  DELETE, then INSERT
    nothing changed        ->  emit no statement at all
    rel is gone            ->  refuse by name, unless you pass --allow-drop

"Emit nothing" is the one that matters. A reload that edits one rule should touch exactly one table.

## Door C: reading many things at once

Writes already work the way you want. One call, one list, any number of rels, adds and deletes mixed, all in the same tick.

    POST /arrivals  { batch: [ add tree, add fruit, del leaf ] }   ->  one tick

Reads work one at a time, so three rels costs three round trips.

    GET /idb/tree      GET /idb/fruit      GET /idb/leaf

After, the read rides along on the write call.

    POST /arrivals  { batch: [ ... ], read: ["tree","fruit"] }
        ->  { ticks: [...], rows: [ tree snapshot, fruit snapshot ] }

The write lands first and the read looks after it, so you get the world your write produced, and every snapshot says which tick it came from. Send an empty batch and you get a plain multi-rel read that turns no tick at all. No new URL, and the read list follows the write list's rules: your order is kept, and a name listed twice comes back twice.

## The order to build it

    B (reload)  ->  C (batch read)  ->  A (multi-file)

B goes first: it is a live bug and it waits on nothing. C is second, because it wants B's rel list to check the names you ask for. A is last, the biggest piece, and nothing else waits on it.

Eleven steps. Each has one test that fails before the change and passes after, one command that proves it, and each is green on its own.

## What I need you to decide

1. Look rels up by fingerprint, or is the flattened name enough? I say flattened name for now; a fingerprint in the URL freezes a format built for compile speed.
2. When a generic is stamped out for a concrete type, does the copy point back at the generic? I say yes, keep the link, or "reshape the generic" has nothing to follow.
3. Keep the five built-in types pinned at slots 1 through 5? I say yes, it keeps rebuilds byte-for-byte identical.
4. To write a rule for `orchard.tree`, must `orchard` already declare `tree`, or does writing it create it? I say declare first, so a typo cannot invent a rel behind your back.
5. Does `--allow-drop` live on the request or the server start line? I say the request, so one careless boot cannot disarm the guard for a whole session.
6. Does the read list live on the write call or get its own URL? I say the write call, with an empty batch when you only want to read.
