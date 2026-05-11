# V4 Config And CLI

## Config Path

Default load order:

```text
$SPREFA_CONFIG
~/.config/sprefa/config.toml
~/.config/sprefa/repos.toml    # legacy repo-only shape
```

Missing config loads as empty config. Current load path swallows parse/read errors because app construction does not have a diagnostics channel yet.

Example config:

```text
v4/examples/sprefa.config.example.toml
```

## Config Shape

```toml
[store]
fact_db = "/Users/me/.local/share/sprefa/facts.db"
queue_db = "/Users/me/.local/share/sprefa/queue.db"

[run]
root = "/Users/me/projects"
remote = "http://127.0.0.1:8787"
show_rows = true
max_diags = 50

[daemon]
bind = "127.0.0.1:8787"
root = "/Users/me/projects"
ghcache_db = "/Users/me/.local/share/ghcache/gh.db"
ghcache_interval_ms = 500

[[repos]]
slug = "myorg/sprefa"
root = "/Users/me/projects/sprefa"
```

`[store]` supplies shared defaults. `[run]` and `[daemon]` can override `fact_db` and `queue_db` for that driver.

## Precedence

```text
CLI flag
  > command-specific config section
  > [store] shared default
  > built-in default
```

Examples:

```text
sprefa-run --fact-db X       beats [run].fact_db and [store].fact_db
[run].fact_db                beats [store].fact_db
sprefa-daemon --queue-db X   beats [daemon].queue_db and [store].queue_db
```

## sprefa-run

```bash
sprefa-run <path-to-sprf-file> \
  [--show-rows | --no-show-rows] \
  [--max-diags N] \
  [--remote URL] \
  [--root PATH] \
  [--fact-db PATH] \
  [--queue-db PATH]
```

Config defaults:

```text
[run].root
[run].remote
[run].show_rows
[run].max_diags
[run].fact_db or [store].fact_db
[run].queue_db or [store].queue_db
```

`--remote` routes calls to a running `sprefa-daemon`. Without `--remote`, the runner builds an in-process app router.

## sprefa-daemon

```bash
sprefa-daemon \
  [--bind 127.0.0.1:8787] \
  [--root DIR] \
  [--fact-db PATH] \
  [--queue-db PATH] \
  [--ghcache-db PATH] \
  [--ghcache-interval-ms N]
```

Config defaults:

```text
[daemon].bind
[daemon].root
[daemon].fact_db or [store].fact_db
[daemon].queue_db or [store].queue_db
[daemon].ghcache_db
[daemon].ghcache_interval_ms
```

Ghcache flags exist only when the `ghcache` feature is compiled in. The feature is default-on. With `--no-default-features`, ghcache CLI flags return a usage error.

## sprefa-lsp

`sprefa-lsp` currently has no project-specific CLI flags. In VS Code it roots the in-process app at the first workspace folder from `initialize`; outside VS Code it falls back to the current working directory. The VS Code extension controls:

```text
sprefa-v4.serverPath
sprefa-v4.serverArgs
sprefa-v4.trace.server
```

Future config hook: let LSP reuse the same `SprfConfig` root/store defaults as `sprefa-run` and `sprefa-daemon`.

## Just Recipes

```bash
just v4-config-test
just v4-run-with-config
just v4-ghcache-test
just v4-no-ghcache-test
just v4-lsp-test
just v4-lsp-build
```

`v4-run-with-config` defaults to:

```bash
SPREFA_CONFIG=v4/examples/sprefa.config.example.toml
```
