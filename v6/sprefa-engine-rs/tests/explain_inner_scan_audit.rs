// issues/inner-scan-audit: three ghcache.dl6 rels EXPLAINed as a real inner
// SCAN after a join on a non-leading UNIQUE-key column. lower.pl's
// audit_scan_index_ddl/3 adds one dedicated index per column
// (v6/prolog/lower.pl, grep audit_scan_index); each assertion here pins the
// exact DDL text and checks EXPLAIN QUERY PLAN reads SEARCH, not SCAN.

use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::SqlStatement;

fn explain(seam: &SqliteSeam, sql: &str) -> String {
    let plan = seam
        .execute(&SqlStatement {
            sql: format!("EXPLAIN QUERY PLAN {sql}"),
            args: vec![],
        })
        .expect("explain");
    format!("{:?}", plan.rows)
}

#[test]
fn pr_batch_response_searches_status_after_the_audit_index() {
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.run_ddl(&[
        "CREATE TABLE \"ghcache_pr_batch_response\" (\"__id\" INTEGER PRIMARY KEY, \
         \"batch_key\" INTEGER NOT NULL, \"bucket\" INTEGER NOT NULL, \
         \"status\" INTEGER NOT NULL, \"data\" TEXT NOT NULL, \
         UNIQUE (\"batch_key\", \"bucket\", \"status\", \"data\"))"
            .to_string(),
        "CREATE INDEX \"ghcache_pr_batch_response__scan_status\" \
         ON \"ghcache_pr_batch_response\" (\"status\")"
            .to_string(),
    ])
    .expect("ddl");
    seam.execute_multiple(
        "INSERT INTO \"ghcache_pr_batch_response\" VALUES (1, 1, 1, 200, '{}'), \
         (2, 1, 1, 404, '{}')",
    )
    .expect("seed rows");

    let plan_text = explain(
        &seam,
        "SELECT DISTINCT \"data\" FROM \"ghcache_pr_batch_response\" b0 \
         WHERE (b0.\"status\" IS 200)",
    );
    assert!(
        plan_text.contains("SEARCH")
            && plan_text.contains("ghcache_pr_batch_response__scan_status"),
        "no SEARCH on the status index in: {plan_text}"
    );
    assert!(
        !plan_text.contains("SCAN b0"),
        "b0 still SCANned: {plan_text}"
    );
}

#[test]
fn rest_response_searches_status_after_the_audit_index() {
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.run_ddl(&[
        "CREATE TABLE \"ghcache_rest_response\" (\"__id\" INTEGER PRIMARY KEY, \
         \"endpoint_path\" INTEGER NOT NULL, \"bucket\" INTEGER NOT NULL, \
         \"status\" INTEGER NOT NULL, \"body\" TEXT NOT NULL, \
         UNIQUE (\"endpoint_path\", \"bucket\", \"status\", \"body\"))"
            .to_string(),
        "CREATE INDEX \"ghcache_rest_response__scan_status\" \
         ON \"ghcache_rest_response\" (\"status\")"
            .to_string(),
    ])
    .expect("ddl");
    seam.execute_multiple(
        "INSERT INTO \"ghcache_rest_response\" VALUES (1, 1, 1, 200, '[]'), \
         (2, 1, 1, 304, '[]')",
    )
    .expect("seed rows");

    let plan_text = explain(
        &seam,
        "SELECT DISTINCT \"body\" FROM \"ghcache_rest_response\" b1 \
         WHERE (b1.\"status\" IS 200)",
    );
    assert!(
        plan_text.contains("SEARCH") && plan_text.contains("ghcache_rest_response__scan_status"),
        "no SEARCH on the status index in: {plan_text}"
    );
    assert!(
        !plan_text.contains("SCAN b1"),
        "b1 still SCANned: {plan_text}"
    );
}

#[test]
fn checkout_task_searches_fetch_pr_branches_after_the_audit_index() {
    let seam = SqliteSeam::in_memory().expect("seam");
    seam.run_ddl(&[
        "CREATE TABLE \"ghcache_checkout_task\" (\"__id\" INTEGER PRIMARY KEY, \
         \"repo_slug\" INTEGER NOT NULL, \"dest_root\" INTEGER NOT NULL, \
         \"want_sha\" INTEGER NOT NULL, \"fetch_pr_branches\" INTEGER NOT NULL, \
         UNIQUE (\"repo_slug\", \"dest_root\", \"want_sha\", \"fetch_pr_branches\"))"
            .to_string(),
        "CREATE INDEX \"ghcache_checkout_task__scan_fetch_pr_branches\" \
         ON \"ghcache_checkout_task\" (\"fetch_pr_branches\")"
            .to_string(),
    ])
    .expect("ddl");
    seam.execute_multiple(
        "INSERT INTO \"ghcache_checkout_task\" VALUES (1, 1, 1, 1, 1), (2, 2, 1, 1, 0)",
    )
    .expect("seed rows");

    let plan_text = explain(
        &seam,
        "SELECT \"repo_slug\" FROM \"ghcache_checkout_task\" b0 \
         WHERE b0.\"fetch_pr_branches\" = 1",
    );
    assert!(
        plan_text.contains("SEARCH")
            && plan_text.contains("ghcache_checkout_task__scan_fetch_pr_branches"),
        "no SEARCH on the fetch_pr_branches index in: {plan_text}"
    );
    assert!(
        !plan_text.contains("SCAN b0"),
        "b0 still SCANned: {plan_text}"
    );
}
