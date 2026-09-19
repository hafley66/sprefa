# dl8 at the repository root. Earlier engines: `just v5 <recipe>`, `just v6 <recipe>`.

default:
    @just --list

build:
    cargo build

test *args:
    cargo test --no-fail-fast {{args}}

# dev loop: spans from every `dl8` run land in the viewer's DuckDB.
# `just trace-up` once, then `just trace compile fixtures/openapi/todo.dl7` as often as you like,
# then `just trace-query` after `just trace-down`.
TRACE_DB := env_var_or_default("TRACE_DB", "/tmp/dl8-trace.duckdb")

trace-up:
    otel-desktop-viewer --db {{TRACE_DB}} --open-browser=false &

trace-down:
    pkill -f "otel-desktop-viewer --db {{TRACE_DB}}" || true

# any dl8 subcommand, debug build, phase lines on stderr, spans exported
trace *args:
    RUST_LOG=info HAFLEY_OTLP_ENDPOINT=http://127.0.0.1:4318/v1/traces timeout 10 cargo run -q --bin dl8 -- {{args}}

# root phases and child spans of the newest trace, ms
trace-query:
    duckdb -readonly {{TRACE_DB}} "WITH t AS (SELECT s.span_id, s.parent_span_id, coalesce(a.value, s.name) AS name, s.start_time, s.end_time FROM spans s LEFT JOIN attributes a ON list_contains(s.attribute_ids, a.id) AND a.key='name'), t0 AS (SELECT min(start_time) AS v FROM t), q AS (SELECT c.parent_span_id AS span_id, sum(try_cast(qa.value AS DOUBLE)) AS sql_ms FROM spans c JOIN attributes qa ON list_contains(c.attribute_ids, qa.id) AND qa.key='ms' GROUP BY c.parent_span_id) SELECT round((t.start_time - (SELECT v FROM t0))/1000000.0,1) AS start_ms, round((t.end_time - t.start_time)/1000000.0,1) AS dur_ms, round(coalesce(q.sql_ms, 0.0),1) AS sql_ms, round((t.end_time - t.start_time)/1000000.0 - coalesce(q.sql_ms, 0.0),1) AS rust_ms, t.name, p.name AS parent FROM t LEFT JOIN t p ON p.span_id = t.parent_span_id LEFT JOIN q ON q.span_id = t.span_id ORDER BY t.start_time"

# Build the sqlite_ivm loadable extension the `_18_sqlite_emit` tests load.
ivm-ext:
    cargo build --release --features extension --manifest-path sqlite_ivm/Cargo.toml

book:
    mdbook build book

v5 *args:
    just --justfile v5/justfile --working-directory v5 {{args}}

v6 *args:
    just --justfile v6/justfile --working-directory v6 {{args}}

# reclaim disk: merged clean worktrees, then target dirs in the rest. dry run without --apply
reap *ARGS:
    bash scripts/fleet-reap.sh {{ARGS}}
