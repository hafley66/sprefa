# Branch B in plain words

A module is a folder. A rel is a file in that folder.

```
root
├── users              (a folder, no table of its own)
│   ├── active         (a rel, has a table)
│   └── banned         (a rel, has a table)
└── events             (a rel at the top level, has a table)
```

Writing `users.active(Name) <- signup(Name).` anywhere makes the folder and the
file if they are missing. No one has to announce `users` first.

Under the hood every nested name is flattened into one long name with a short
fingerprint at the end:

```
  what you write        what the database sees
  users.active/1   ->   users__active__4f1a90cd
  users.active/2   ->   users__active__b7e30112     (different arity, different fingerprint)
  billing.active/1 ->   billing__active__2c8d55e1   (different folder, different fingerprint)
```

Two files writing rules for the same name is plain union, the same way two rules
for one name have always merged.

```
  file one:  reports.daily(D) <- sales(D).
  file two:  reports.daily(D) <- refunds(D).
                    |
                    v
             one rel, both rule sets, no coordination
```

The catalog is one table listing every name and who its parent is. Folders appear
in it with no table attached. Columns appear in it as children of their rel. One
table, one parent column, one index.

```
  id  parent  name       kind
  13    0     users      rel      <- the folder
  14   13     active     rel      <- the file inside it
  15   14     name       column   <- the column inside that
```

The price of never declaring first: a misspelled name is a brand new empty rel
that compiles green.
