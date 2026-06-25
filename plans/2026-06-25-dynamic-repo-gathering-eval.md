# Dynamic repo gathering — eval sheet

> **UPDATE (implemented 2026-06-25).** Shipped a data-driven path that subsumes
> approaches (1)+(2): the `repo` builtin is now an **insertable sink**. A rule
> `repo(slug, root, url) <- <discovery body>` is drained post-fixpoint; each row
> pulls (clone-if-missing + register into `self.repos`) when its github org is in
> the `org(name)` allowlist (engine hard-check on drain — non-bypassable). The
> `repo` relation is now `(slug, root, url)` and reflects the registered set;
> `scan("*")`, lazy indexers, and the daemon's notify watcher all reach a pulled
> repo on the next tick. No `load`-RPC register path or self-scan rewrite needed.
> Details + tests in `v5/src/engine.rs` (`run_repo_pulls`, `parse_github_org`,
> `snapshot_repos`, `repo_relation`) and `v5/tests/repo_sink.rs`. Christmas #1
> (fully data-driven scan coords) remains the open general case.

Can the daemon scan/load repos **not in `config.toml`**, dynamically, so a script
or a loaded file can target arbitrary repos without pre-configuration? Assessed
against the running code (engine `resolve_repo`, daemon watcher, lazy indexers).

Tied to christmas-list #1 (data-driven scan repo/rev) and the daemon `load` RPC
work (`chat_log/20260625.4.daemon-load-rpc-op-table-autogen.md`).

## TL;DR

Dynamic **scanning** is mostly already here. Dynamic **watching** and full
**indexing** of those repos are the real gaps. The cheapest path to "load a
script, it scans its own repo reactively" is a load-time self-scan→path rewrite
(no engine surgery) + a watcher-add for the script's `.git` ancestor.

## Current state (what the code already does)

- `resolve_repo` (engine.rs:963-978) resolves a scan repo coordinate in three ways:
  1. `.` / `""` / `"self"` → `(self_slug, self.root)`.
  2. a config slug → that `RepoConfig` (clone-if-missing / `allow_missing`).
  3. **any existing path** → `(slug = path's dir name, root = path)`. ← the escape hatch.
- So `scan("/Users/x/projects/other", "WORK", "src/**/*.rs", p, rev)` ALREADY
  works today, with no `config.toml` entry. The slug is synthesized from the dir
  name; `_file.repo` carries it; retraction keys on it.
- `resolve_scan_repos("*")` fans over **config** repos only (engine.rs:1024-1035).
  A path-form repo is not in that fanout.
- 3-ary `scan(glob, path, rev_out)` (the self form) always means `self.root`.
  Naming a different repo requires the 4/5-ary form with the coord in slot 1.

## What does NOT work for a non-config repo

| gap | where | impact |
|---|---|---|
| **not watched** | daemon watcher = `eng_root` + config repos (daemon.rs:530,536) | edits in a path-scanned repo don't fire a reactive tick; only a full tick (program/git/config change) re-runs. This is #6 generalized. |
| **not in `repo_roots()`** | engine.rs:1050 = self + config only | the lazy indexers (`type_entity`, call graph, `doc_node`) read each file from its repo root via this map → a path-scanned repo's files get `scan`/`match`/`ast` rows but **no type/call/doc rows**. Partial coverage. |
| **`tick_paths` skips non-self** | engine.rs:1693 `p.strip_prefix(&self.root)` else `continue` | even if watched, an incremental tick over a config/path-repo edit drops it. Full-tick only. (=#6) |
| **not persisted/registered** | `self.repos: Vec<RepoConfig>` is config-driven | a path-repo is ad hoc: slug synthesized per-scan, not a stable registered identity. Fine for one scan; not for "this repo is now in view permanently." |

## Approaches (evaluated)

1. **🎁 Load-time self-scan→path rewrite** (the #4 candidate).
   On `load watched`, compute the script's `.git` ancestor; rewrite its 3-ary
   `scan("WORK", g, p, r)` rules to `scan("<ancestor>", "WORK", g, p, r)` in the
   parsed AST before merge. `resolve_repo` accepts the path as-is.
   - Feasibility: **high**. Localized to the `load` path (a `BodyItem::Scan` walk
     + term swap). No `Rule`-origin surgery, no lowerer change.
   - Catches: only the self/WORK form. `scan("HEAD",…)`, named-slug scans, and
     `*` fanout are untouched (correct — those are explicit already).
   - Still needs #6 + a watcher-add for reactivity.

2. **🔧 `load` root param + register-and-watch**.
   `--load <path> --repo <slug|root>` (or auto from `.git` ancestor): the daemon
   `set_repos` appends a `RepoConfig { slug, root, url: None }`, adds the root to
   the notify watcher, and `repo_roots()` picks it up (it already unions
   `self.repos`). Lazy indexers then cover it.
   - Feasibility: **high**. `set_repos` + watcher.watch already exist. The
     registered repo behaves exactly like a config repo (reactive once #6 lands).
   - This is "dynamic config" without editing `config.toml`. Most aligned with
     "folders in view" permanence.

3. **🔧 Auto-register on unknown-path scan**.
   When `resolve_repo` hits the existing-path branch (case 3 above), also INSERT
   the `(slug, root)` into `self.repos` and signal the daemon to watch it.
   - Feasibility: **medium**. `resolve_repo` is `&self` (no mutation) and runs
     mid-tick; registering there needs interior mutability or a post-tick
     "newly-seen repos" set the daemon drains. Doable but invasive.
   - Upside: zero-config — any scan of a path auto-brings it into view.

4. **🎁 Data-driven scan (christmas #1)**.
   A derived row drives the repo/rev/glob: `scan(R, Rev, G, P, RevO)` where R/Rev
   are vars bound by a previous body atom. Today coordinates are literal-only
   (parse.rs:scan term() is a literal), so this is impossible.
   - Feasibility: **low / large**. Needs the lowerer to generate a per-row scan
     (semi-naive over a dynamic scan set) and the engine to run source rules per
     bound coordinate. This is the christmas-list big-rock; not in scope for the
     load RPC but it's the general case of "dynamic scanning."
   - Until then, dynamic coords are faked via codegen (shell-loop emits a rule
     per repo) or the `load`+rewrite of (1)/(2).

5. **🌐 On-disk discovery** (scan a parent dir for `.git` children, register all).
   - Feasibility: trivial but noisy (registering every sibling repo). Only worth
     it as an explicit command (`dl --add-repo <path>` or a discovery root), not
     automatic.

## Recommendation

Do **(1) + (2) together** as the `load` watched path, then **#6** to make them
reactive:

- `load watched`: compute `.git` ancestor (or accept `--repo`/`--root`); call a
  new `daemon.register_repo(root)` that appends to `RepoConfig` + adds a notify
  watch + saves to `_repo` (so it survives restart as "dynamic config").
- Rewrite the loaded script's self-scans to the registered slug (cleaner than a
  bare path: stable slug, lazy indexers cover it, `*` fanout includes it).
- #6: `tick_paths` routes a changed path to its owning repo (self or registered)
  and runs the incremental tick against that repo's root, falling back to full
  tick when the mapping is ambiguous. Unblocks reactivity for ALL non-self repos,
  not just loaded-script ones.

Defer (3) and (4). (3) is nice-to-have ergonomics; (4) is the christmas big-rock
(data-driven scan) and a separate project.

## Sequence (concrete, against the load RPC already built)

1. **#4a register-on-load**: `daemon::Daemon::register_repo(&self, root)` — append
   `RepoConfig{slug: dir_name, root, url:None}` to engine repos (via a locked
   mutation + `set_repos`), `watcher.watch(root, Recursive)`, persist to `_repo`.
   Hook into the `load` watched arm.
2. **#4b self-scan→slug rewrite**: walk the loaded file's `BodyItem::Scan` items;
   for the 3-ary self/WORK form, swap `repo` term to the registered slug. Pure
   AST transform before merge.
3. **#6 multi-root tick_paths**: engine.rs:1693 — map each changed path to its
   owning repo root (try `strip_prefix` against self + each registered repo);
   run the WORK source rules for that repo; full-tick fallback on ambiguity /
   derived churn (the existing :1657-1662 fallback).
4. Lazy-indexer coverage is automatic once the repo is in `self.repos`
   (`repo_roots()` unions it).

## Open questions

- Slug collisions: two repos with the same dir name. Need a disambiguator (full
  path hash suffix, or require explicit `--repo <slug>`).
- Persisted dynamic repos vs ephemeral: should `--load`-registered repos survive
  daemon restart (write to `_repo` / a dynamic-config file) or be load-only?
- Should `*`/`all` fanout include dynamically-registered repos? (Probably yes —
  they're "in view" once registered.)
