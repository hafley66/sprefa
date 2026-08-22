---
created: 2026-08-22
updated: 2026-08-22
type: feature
reporter: chris
assignee: chris
status: open
priority: low
epic: cheap-fast-analysis
---

# dl6 programs the GraphQL woe in its own language, someday

User 2026-08-22: "i wouldnt mind dl6 being able to program this graphql woe in
its own lang someday." The selection set, the per-repo aliasing, the batch
split, the pagination cursor and the rate-limit read are today a mix of
`concat` string building and decode rules (`v6/dl/ghcache/ghcache.dl6:773-1136`).
The target is a dl6 module (`use "graphql.dl6"`) that owns that shape and a
program that states only the fields it wants. Depends on
`decode-named-pattern-graphql-selection` and on strings being done first.
