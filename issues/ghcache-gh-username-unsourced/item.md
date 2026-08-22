---
created: 2026-08-22
updated: 2026-08-22
type: bug
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# ghcache.dl6: `gh_username` has no rule, so the user-events endpoint never polls

## Finding (coordinator, live `dl6 run`, 2026-08-22)

`v6/dl/ghcache/ghcache.dl6:204` declares `rel gh_username(login: text) key(1)`;
`:257` reads it for `users/<login>/events/orgs/<owner>`. No rule or fact produces
a row. Live run: `ghcache_poll_endpoint` holds 8 rows, none of them the
user-events path. ghcacher resolves the login through the GitHub `user`
endpoint; the port needs the same (`/gh/rest_cond` on `user`, one keyed fold).
