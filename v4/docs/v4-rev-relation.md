# V4 rev Relation

`rev` is both the git revision producer and the built-in revision relation.

```text
rev(...)   resolves or discovers revisions and writes rev rows
rev?(...)  queries stored rev rows
```

## Direct Revs

```sprf
repo() > rev(:HEAD)
repo() > rev(:main, :HEAD~1)
```

Direct mode resolves explicit revspecs through git and emits one cursor per
resolved commit.

## Dynamic Ref Discovery

```sprf
repo()
  > rev(glob`support/0.1.${PATCH?}`)
```

Glob mode enumerates git refs, matches each short ref name, binds glob
captures, peels the matched ref to a commit, writes a `rev` row, and emits a
rev cursor.

Current ref kinds:

| `KIND` | Ref prefix |
| --- | --- |
| `tag` | `refs/tags/` |
| `branch` | `refs/heads/` |
| `remote` | `refs/remotes/` |

## Built-in Table

`rev(...)` declares and writes the `rev` table.

| Column | Meaning |
| --- | --- |
| `REPO` | repo slug |
| `KIND` | `spec`, `tag`, `branch`, or `remote` |
| `SPEC` | direct revspec for direct mode |
| `NAME` | short ref name for glob mode |
| `REF` | full git ref name |
| `REV` | peeled commit oid |
| `TARGET` | direct ref target oid |
| `COMMIT_TS` | peeled commit timestamp seconds |
| `TAG_TS` | reserved for annotated tag timestamp |

`REV` always means the peeled commit oid, so downstream `fs()` keeps using the
same rev coord contract as `rev(:HEAD)`.

## Query Side

```sprf
rev?(KIND: `tag`, NAME?, REV?, COMMIT_TS?)
  > render.markdown`- ${NAME}: ${REV}`;
```

`rev?` does not open git. It reads only the built-in `rev` relation. If no
producer has run, it emits zero rows.

## Example

```sprf
rule(:support_revs, REPO?, PATCH?, REV?, NAME?);

repo()
  > rev(glob`support/0.1.${PATCH?}`)
  > support_revs(REPO, PATCH, REV, NAME);

support_revs?(REPO?, PATCH?, REV?, NAME?)
  > render.markdown`- ${REPO} patch ${PATCH}: ${NAME} ${REV}
`;
```

## Remaining Work

- Add `re``...`` mode for dynamic ref discovery.
- Add dirty wake domains for git refs and tag/branch changes.
- Add support-counted retraction for deleted or retargeted refs.
- Add `poll` and `changed` primitives so userland sprf can prototype watchers.
- Add `git.distance(BASE, HEAD, AHEAD?, BEHIND?, MERGE_BASE?)`.
