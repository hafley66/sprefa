# Beyond code: dl as a personal datalog/sqlite/rust monster

Date: 2026-06-11. Design notes for pointing the engine at non-code domains,
networking first (stated learning goal). Nothing built yet.

## The trick that makes every domain work

dl's source ops read FILES. Any tool that can dump its world into files makes
that world queryable: snapshot scripts write timestamped files under a scanned
directory, and the (repo, path, rev) coordinate plus file hashing gives
incremental re-extraction for free. The time axis is the filename.

    snaps/2026-06-11T23:30:00.lsof.txt
    snaps/2026-06-11T23:30:00.arp.txt
    ...
    rel conn(snap: text, proto: text, laddr: text, raddr: text, state: text).
    conn(snap, ...) <- scan("WORK", "snaps/*.lsof.txt", p, rev),
      match(p, rev, /$proto $laddr->$raddr \($state\)/, line), ...

## Networking study plans (each one is an evening)

1. **Who is my laptop talking to**: `lsof -i -nP` snapshots → conn rel →
   join against a hand-curated `known(raddr, label)` rel → `? conn` where
   NOT known = the surprise list. Anti-join is the whole lesson.
2. **LAN map**: `arp -a` + `dig -x` per address (cmd op) → node-per-host,
   d2 render via gen. The typeports pattern with hosts instead of structs:
   ports ARE ports this time.
3. **DNS chase**: `dig +trace example.com` output → parent-zone edges →
   `closure(delegates)` = the delegation path as a graph. Recursion on real
   infrastructure.
4. **Listening surface over time**: nightly `netstat -an | grep LISTEN`
   snapshot; diff two snaps relationally (the time.dl pattern); diag error on
   any NEW listening port → `dl --check` in a cron = a tripwire.
5. **TLS chains**: `openssl s_client -showcerts` dumps per host → issuer
   edges → one shared CA graph across every service you use.

Numbers 1 and 4 are rails (anti-join, diff-as-diag); 2, 3, 5 are graphs that
feed straight into the anim/ports pipeline from the other plan.

## Other domains with the same shape

- **processes**: `ps -eo` snapshots → parent edges → closure = process trees
- **homebrew/cargo dep graphs**: `brew deps --tree --all` / cargo metadata
  json → json op → blast radius of removing a package
- **browser history / shell history**: sqlite files and flat files you already
  own; `cmd` can sqlite3-dump them into scannable TSV
- **tmux**: the overlay idea from memory — pane/session activity as facts

## What the engine needs (small, shared with the other plans)

1. **un-filed command source**: today cmd needs a matched file. A snapshot
   script outside dl covers it, but a `snap("lsof -i -nP", out)` op with an
   explicit re-run policy (time-bucket in the cache key) would remove the
   cron+script step.
2. **int arithmetic** (already on the anim list): byte counts, durations,
   port ranges.
3. **timestamp ordering**: filenames sort lexically if ISO-stamped, and `<`
   is lexical, so this mostly works today; worth one example proving it.
4. **`{var:sh}` escaping** before any cmd takes untrusted strings (hostnames
   from arp output piped back into dig).
