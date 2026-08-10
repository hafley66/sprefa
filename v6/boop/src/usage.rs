//! Token analytics: totals, group-by, gap-aware billing windows, burn rate,
//! and cost as a join against a rate table (cost is computed, never stored).

use anyhow::Result;

use crate::ident::{Row, Store};

/// The bucket a `--group-by` asks for. `None` is the totals row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum GroupBy {
    Model,
    Session,
    Project,
    Harness,
    Hour,
    Day,
    Week,
    Month,
}

impl GroupBy {
    /// The SELECT expression that names the bucket, and whether the query has
    /// to reach `agent_session` to build it.
    fn expression(self) -> (&'static str, bool) {
        match self {
            GroupBy::Model => ("dict_model.value", false),
            GroupBy::Session => ("dict_session.value", true),
            GroupBy::Project => ("COALESCE(dict_cwd.value, '-')", true),
            GroupBy::Harness => ("dict_harness.value", true),
            GroupBy::Hour => (
                "STRFTIME('%Y-%m-%dT%H', usage.ts / 1000, 'unixepoch')",
                false,
            ),
            GroupBy::Day => ("DATE(usage.ts / 1000, 'unixepoch')", false),
            GroupBy::Week => ("STRFTIME('%Y-W%W', usage.ts / 1000, 'unixepoch')", false),
            GroupBy::Month => ("STRFTIME('%Y-%m', usage.ts / 1000, 'unixepoch')", false),
        }
    }
}

/// The filter every usage read shares.
#[derive(Default, Clone)]
pub struct UsageQuery {
    pub session: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    /// Fold the session's whole spawn subtree into the numbers.
    pub rollup_subtree: bool,
    pub limit: Option<u64>,
}

/// One billing window, or the idle stretch between two of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub window_start: i64,
    pub first_ts: i64,
    pub last_ts: i64,
    pub calls: i64,
    pub total_tokens: i64,
    pub is_gap: bool,
}

/// Cost per million tokens, per bucket. A model with no row costs `null`,
/// never zero: a missing rate is not a free call.
pub struct ModelPrice<'a> {
    pub model: &'a str,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_write_5m_per_mtok: f64,
    pub cache_write_1h_per_mtok: f64,
    pub cache_read_per_mtok: f64,
    pub source: &'a str,
}

/// The five-bucket SUM every aggregate selects, plus the cost join expression.
const SUMS: &str = "
    COUNT(*) AS calls,
    COALESCE(SUM(usage.input_tokens), 0) AS input_tokens,
    COALESCE(SUM(usage.output_tokens), 0) AS output_tokens,
    COALESCE(SUM(usage.cache_create_5m_tokens), 0) AS cache_create_5m_tokens,
    COALESCE(SUM(usage.cache_create_1h_tokens), 0) AS cache_create_1h_tokens,
    COALESCE(SUM(usage.cache_read_tokens), 0) AS cache_read_tokens,
    SUM(usage.input_tokens / 1e6 * price.input_per_mtok
      + usage.output_tokens / 1e6 * price.output_per_mtok
      + usage.cache_create_5m_tokens / 1e6 * price.cache_write_5m_per_mtok
      + usage.cache_create_1h_tokens / 1e6 * price.cache_write_1h_per_mtok
      + usage.cache_read_tokens / 1e6 * price.cache_read_per_mtok) AS cost_usd,
    COUNT(*) FILTER (WHERE price.model_id IS NULL) AS unpriced_calls,
    MIN(usage.ts) AS first_ts,
    MAX(usage.ts) AS last_ts";

impl Store {
    /// Totals, or one row per bucket when `group_by` is given.
    pub fn usage_report(&self, group_by: Option<GroupBy>, filter: &UsageQuery) -> Result<Vec<Row>> {
        let (bucket, needs_session) = match group_by {
            Some(group) => {
                let (expression, needs) = group.expression();
                (Some(expression), needs)
            }
            None => (None, false),
        };
        let mut sql = String::from("SELECT ");
        if let Some(bucket) = bucket {
            sql.push_str(&format!("{bucket} AS bucket, "));
        }
        sql.push_str(SUMS.trim());
        sql.push_str(
            "
            FROM agent_usage AS usage
            LEFT JOIN model_price AS price ON price.model_id = usage.model_id",
        );
        if group_by == Some(GroupBy::Model) {
            sql.push_str(" JOIN dict_model ON dict_model.id = usage.model_id");
        }
        if needs_session {
            sql.push_str(
                " JOIN agent_session ON agent_session.session_id = usage.session_id
                  JOIN dict_session ON dict_session.id = usage.session_id
                  JOIN dict_harness ON dict_harness.id = agent_session.harness_id
                  LEFT JOIN dict_cwd ON dict_cwd.id = agent_session.cwd_id",
            );
        }
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        sql.push_str(&self.scope_clause(filter, &mut values)?);
        if let Some(bucket) = bucket {
            sql.push_str(&format!(" GROUP BY {bucket} ORDER BY {bucket}"));
        }
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut rows = self.rows(&sql, values)?;
        suppress_partial_cost(&mut rows);
        Ok(rows)
    }

    /// The WHERE that scopes a usage read: time range, one session, or the
    /// session's whole spawn subtree.
    fn scope_clause(
        &self,
        filter: &UsageQuery,
        values: &mut Vec<rusqlite::types::Value>,
    ) -> Result<String> {
        let mut clause = String::from(" WHERE 1=1");
        match (&filter.session, filter.rollup_subtree) {
            (Some(session), true) => {
                clause.push_str(
                    " AND usage.session_id IN (
                        WITH RECURSIVE subtree(session_id) AS (
                          SELECT id FROM dict_session WHERE value = ?
                          UNION
                          SELECT edge.child_session_id FROM agent_edge AS edge
                          JOIN subtree ON edge.parent_session_id = subtree.session_id)
                        SELECT session_id FROM subtree)",
                );
                values.push(session.clone().into());
            }
            (Some(session), false) => {
                clause.push_str(
                    " AND usage.session_id = (SELECT id FROM dict_session WHERE value = ?)",
                );
                values.push(session.clone().into());
            }
            (None, _) => {}
        }
        if let Some(since) = filter.since {
            clause.push_str(" AND usage.ts >= ?");
            values.push((since as i64).into());
        }
        if let Some(until) = filter.until {
            clause.push_str(" AND usage.ts < ?");
            values.push((until as i64).into());
        }
        Ok(clause)
    }

    /// Every call's timestamp and token total in time order, the one scan the
    /// window fold needs. `idx_usage_ts` serves the ORDER BY.
    fn usage_timeline(&self, filter: &UsageQuery) -> Result<Vec<(i64, i64)>> {
        let mut values = Vec::new();
        let scope = self.scope_clause(filter, &mut values)?;
        let sql = format!(
            "SELECT usage.ts,
                    usage.input_tokens + usage.output_tokens + usage.cache_create_5m_tokens
                      + usage.cache_create_1h_tokens + usage.cache_read_tokens
             FROM agent_usage AS usage{scope} ORDER BY usage.ts"
        );
        let mut statement = self.connection().prepare(&sql)?;
        let mut out = Vec::new();
        let iter = statement.query_map(rusqlite::params_from_iter(values.iter()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in iter {
            out.push(row?);
        }
        Ok(out)
    }

    /// Gap-aware billing windows, folded in one pass over the timeline.
    pub fn usage_blocks(&self, window_ms: i64, filter: &UsageQuery) -> Result<Vec<Block>> {
        Ok(fold_blocks(&self.usage_timeline(filter)?, window_ms))
    }

    /// Tokens per minute and cost per hour over a trailing window.
    pub fn usage_burn_rate(&self, filter: &UsageQuery) -> Result<Vec<Row>> {
        let mut values = Vec::new();
        let scope = self.scope_clause(filter, &mut values)?;
        let sql = format!(
            "SELECT MIN(usage.ts) AS window_first, MAX(usage.ts) AS window_last, COUNT(*) AS calls,
                    COALESCE(SUM(usage.input_tokens + usage.output_tokens
                      + usage.cache_create_5m_tokens + usage.cache_create_1h_tokens
                      + usage.cache_read_tokens), 0) AS total_tokens,
                    COALESCE(SUM(usage.input_tokens + usage.output_tokens), 0) AS billable_tokens,
                    SUM(usage.input_tokens / 1e6 * price.input_per_mtok
                      + usage.output_tokens / 1e6 * price.output_per_mtok
                      + usage.cache_create_5m_tokens / 1e6 * price.cache_write_5m_per_mtok
                      + usage.cache_create_1h_tokens / 1e6 * price.cache_write_1h_per_mtok
                      + usage.cache_read_tokens / 1e6 * price.cache_read_per_mtok) AS cost_usd
             FROM agent_usage AS usage
             LEFT JOIN model_price AS price ON price.model_id = usage.model_id{scope}"
        );
        let mut rows = self.rows(&sql, values)?;
        for row in &mut rows {
            let first = row["window_first"].as_i64().unwrap_or(0);
            let last = row["window_last"].as_i64().unwrap_or(0);
            let span_ms = (last - first).max(0);
            let total = row["total_tokens"].as_i64().unwrap_or(0) as f64;
            let cost = row["cost_usd"].as_f64();
            let (per_minute, per_hour) = if span_ms == 0 {
                (None, None)
            } else {
                (
                    Some(total * 60_000.0 / span_ms as f64),
                    cost.map(|cost| cost * 3_600_000.0 / span_ms as f64),
                )
            };
            if let Some(object) = row.as_object_mut() {
                object.insert("span_ms".into(), serde_json::json!(span_ms));
                object.insert("tokens_per_minute".into(), serde_json::json!(per_minute));
                object.insert("cost_usd_per_hour".into(), serde_json::json!(per_hour));
            }
        }
        Ok(rows)
    }

    /// The rate table, joined back to model names.
    pub fn price_list(&self) -> Result<Vec<Row>> {
        self.rows(
            "SELECT dict_model.value AS model, model_price.input_per_mtok,
                    model_price.output_per_mtok, model_price.cache_write_5m_per_mtok,
                    model_price.cache_write_1h_per_mtok, model_price.cache_read_per_mtok,
                    dict_price_source.value AS source, model_price.fetched_ts
             FROM model_price
             JOIN dict_model ON dict_model.id = model_price.model_id
             JOIN dict_price_source ON dict_price_source.id = model_price.source_id
             ORDER BY dict_model.value",
            Vec::new(),
        )
    }

    /// Models that priced calls but have no rate row, so a report can name what
    /// its cost column is missing instead of quietly undercounting.
    pub fn unpriced_models(&self, filter: &UsageQuery) -> Result<Vec<Row>> {
        let mut values = Vec::new();
        let scope = self.scope_clause(filter, &mut values)?;
        let sql = format!(
            "SELECT dict_model.value AS model, COUNT(*) AS calls
             FROM agent_usage AS usage
             JOIN dict_model ON dict_model.id = usage.model_id
             LEFT JOIN model_price AS price ON price.model_id = usage.model_id{scope}
               AND 1=1
             GROUP BY usage.model_id HAVING SUM(price.model_id IS NOT NULL) = 0
             ORDER BY calls DESC"
        );
        self.rows(&sql, values)
    }

    /// Write one rate row, replacing whatever was there.
    pub fn price_set(&self, price: &ModelPrice) -> Result<()> {
        let model_id = self.intern_public("dict_model", price.model)?;
        let source_id = self.intern_public("dict_price_source", price.source)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.connection().execute(
            "INSERT INTO model_price (model_id, input_per_mtok, output_per_mtok,
               cache_write_5m_per_mtok, cache_write_1h_per_mtok, cache_read_per_mtok,
               source_id, fetched_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(model_id) DO UPDATE SET
               input_per_mtok = excluded.input_per_mtok,
               output_per_mtok = excluded.output_per_mtok,
               cache_write_5m_per_mtok = excluded.cache_write_5m_per_mtok,
               cache_write_1h_per_mtok = excluded.cache_write_1h_per_mtok,
               cache_read_per_mtok = excluded.cache_read_per_mtok,
               source_id = excluded.source_id,
               fetched_ts = excluded.fetched_ts",
            rusqlite::params![
                model_id,
                price.input_per_mtok,
                price.output_per_mtok,
                price.cache_write_5m_per_mtok,
                price.cache_write_1h_per_mtok,
                price.cache_read_per_mtok,
                source_id,
                now
            ],
        )?;
        Ok(())
    }
}

/// A bucket holding unpriced calls has no total cost, only the cost of the
/// calls that were priced. Reporting the partial sum as `cost_usd` reads as a
/// total and undercounts silently.
fn suppress_partial_cost(rows: &mut [Row]) {
    for row in rows.iter_mut() {
        let unpriced = row["unpriced_calls"].as_i64().unwrap_or(0);
        if unpriced == 0 {
            continue;
        }
        let priced = row["cost_usd"].clone();
        if let Some(object) = row.as_object_mut() {
            object.insert("cost_usd".into(), serde_json::Value::Null);
            object.insert("cost_usd_priced_only".into(), priced);
        }
    }
}

/// A window opens when a call lands more than `window_ms` after the window
/// started OR after the previous call; its start floors to the hour.
pub fn fold_blocks(timeline: &[(i64, i64)], window_ms: i64) -> Vec<Block> {
    const HOUR_MS: i64 = 3_600_000;
    let mut blocks: Vec<Block> = Vec::new();
    let mut previous_ts: Option<i64> = None;
    for (ts, tokens) in timeline.iter().copied() {
        let opens = match (blocks.last(), previous_ts) {
            (None, _) => true,
            (Some(block), Some(previous)) => {
                ts - block.window_start > window_ms || ts - previous > window_ms
            }
            (Some(_), None) => true,
        };
        if opens {
            if let (Some(previous), true) = (previous_ts, !blocks.is_empty()) {
                if ts - previous > window_ms {
                    blocks.push(Block {
                        window_start: previous,
                        first_ts: previous,
                        last_ts: ts,
                        calls: 0,
                        total_tokens: 0,
                        is_gap: true,
                    });
                }
            }
            blocks.push(Block {
                window_start: ts / HOUR_MS * HOUR_MS,
                first_ts: ts,
                last_ts: ts,
                calls: 1,
                total_tokens: tokens,
                is_gap: false,
            });
        } else if let Some(block) = blocks.last_mut() {
            block.last_ts = ts;
            block.calls += 1;
            block.total_tokens += tokens;
        }
        previous_ts = Some(ts);
    }
    blocks
}

/// The ninth decile of closed, non-gap window totals: a ceiling measured from
/// your own history rather than guessed from a plan name.
pub fn p90_ceiling(blocks: &[Block]) -> Option<i64> {
    let mut totals: Vec<i64> = blocks
        .iter()
        .filter(|block| !block.is_gap)
        .map(|block| block.total_tokens)
        .collect();
    if totals.is_empty() {
        return None;
    }
    totals.sort_unstable();
    // Nearest rank: the smallest total at or above the ninth decile, so 90% of
    // closed windows sit at or below it.
    let rank = (totals.len() as f64 * 0.9).ceil() as usize;
    Some(totals[rank.saturating_sub(1).min(totals.len() - 1)])
}

#[cfg(test)]
mod tests {
    use super::{fold_blocks, p90_ceiling, Block};

    fn hour(n: i64) -> i64 {
        n * 3_600_000
    }

    /// The fold is the implementation; the recursive CTE in the plan is the
    /// spec. Same input, same windows.
    #[test]
    fn the_fold_matches_the_window_rule() {
        let window = hour(5);
        let timeline = vec![
            (hour(10) + 60_000, 100),
            (hour(10) + 120_000, 200),
            (hour(12), 300),
            (hour(20), 400),
        ];
        let blocks = fold_blocks(&timeline, window);
        let real: Vec<&Block> = blocks.iter().filter(|block| !block.is_gap).collect();
        assert_eq!(real.len(), 2, "two windows: {blocks:#?}");
        assert_eq!(real[0].window_start, hour(10), "start floors to the hour");
        assert_eq!(real[0].calls, 3);
        assert_eq!(real[0].total_tokens, 600);
        assert_eq!(real[1].calls, 1);
        assert_eq!(real[1].total_tokens, 400);
    }

    /// An idle stretch longer than the window is a gap row, never a window
    /// that pretends work happened inside it.
    #[test]
    fn a_long_silence_becomes_a_gap_row() {
        let blocks = fold_blocks(&[(hour(1), 10), (hour(9), 20)], hour(5));
        let gaps: Vec<&Block> = blocks.iter().filter(|block| block.is_gap).collect();
        assert_eq!(gaps.len(), 1, "{blocks:#?}");
        assert_eq!(gaps[0].first_ts, hour(1));
        assert_eq!(gaps[0].last_ts, hour(9));
        assert_eq!(gaps[0].total_tokens, 0, "a gap bills nothing");
    }

    /// A window stays open while calls keep arriving inside it, even past the
    /// window length measured from the previous call.
    #[test]
    fn a_steady_stream_stays_in_one_window_until_the_window_length() {
        let step = 60_000;
        let timeline: Vec<(i64, i64)> = (0..300).map(|n| (hour(1) + n * step, 1)).collect();
        let blocks = fold_blocks(&timeline, hour(5));
        let real: Vec<&Block> = blocks.iter().filter(|block| !block.is_gap).collect();
        assert_eq!(real.len(), 1, "300 minutes is inside a 5 hour window");
        assert_eq!(real[0].calls, 300);
    }

    #[test]
    fn an_empty_timeline_has_no_windows() {
        assert!(fold_blocks(&[], hour(5)).is_empty());
        assert_eq!(p90_ceiling(&[]), None);
    }

    #[test]
    fn the_ceiling_is_the_ninth_decile_of_closed_windows() {
        let blocks: Vec<Block> = (1..=10)
            .map(|n| Block {
                window_start: hour(n),
                first_ts: hour(n),
                last_ts: hour(n),
                calls: 1,
                total_tokens: n * 100,
                is_gap: false,
            })
            .collect();
        assert_eq!(p90_ceiling(&blocks), Some(900));
    }
}

#[cfg(test)]
mod cte_equality {
    use super::{fold_blocks, UsageQuery};
    use crate::ident::{sync_session, Store};

    /// The recursive CTE from plans/2026-08-09-boop-analytics-PLAN.md section 6.
    /// It is the spec; the Rust fold is the implementation, and they must agree.
    const PLAN_CTE: &str = "
        WITH RECURSIVE
          ordered AS (
            SELECT ROW_NUMBER() OVER (ORDER BY ts) AS seq, ts, session_id, turn
            FROM agent_usage),
          walk(seq, ts, session_id, turn, window_start, prev_ts) AS (
            SELECT seq, ts, session_id, turn, ts / 3600000 * 3600000, ts
            FROM ordered WHERE seq = 1
            UNION ALL
            SELECT next.seq, next.ts, next.session_id, next.turn,
                   CASE WHEN next.ts - walk.window_start > :window_ms
                          OR next.ts - walk.prev_ts     > :window_ms
                        THEN next.ts / 3600000 * 3600000
                        ELSE walk.window_start END,
                   next.ts
            FROM ordered AS next JOIN walk ON next.seq = walk.seq + 1)
        SELECT walk.window_start, COUNT(*) AS calls,
               SUM(usage.input_tokens + usage.output_tokens + usage.cache_create_5m_tokens
                 + usage.cache_create_1h_tokens + usage.cache_read_tokens) AS total_tokens
        FROM walk
        JOIN agent_usage AS usage
          ON usage.session_id = walk.session_id AND usage.turn = walk.turn
        GROUP BY walk.window_start ORDER BY walk.window_start";

    fn iso(ms: i64) -> String {
        use time::format_description::well_known::Rfc3339;
        time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
            .unwrap()
            .format(&Rfc3339)
            .unwrap()
    }

    fn store_with(timeline: &[(i64, i64)]) -> (Store, std::path::PathBuf, std::path::PathBuf) {
        use std::io::Write;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let db_path =
            std::env::temp_dir().join(format!("boop_cte_{}_{}.db", std::process::id(), stamp));
        let log_path =
            std::env::temp_dir().join(format!("boop_cte_{}_{}.jsonl", std::process::id(), stamp));
        let _ = std::fs::remove_file(&db_path);
        let store = Store::open(db_path.clone()).unwrap();
        let mut file = std::fs::File::create(&log_path).unwrap();
        for (index, (ts, tokens)) in timeline.iter().enumerate() {
            writeln!(
                file,
                r#"{{"type":"assistant","sessionId":"s","timestamp":"{}","requestId":"req_{index}","message":{{"id":"msg_{index}","model":"m","usage":{{"input_tokens":{tokens},"output_tokens":0}},"content":[{{"type":"text","text":"x"}}]}}}}"#,
                iso(*ts)
            )
            .unwrap();
        }
        drop(file);
        let session = crate::harness::SessionRef {
            harness: "claude",
            session_id: "s".to_owned(),
            nickname: "s".to_owned(),
            path: log_path.clone(),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        };
        sync_session(&store, &crate::harness::claude::Claude, &session).unwrap();
        (store, db_path, log_path)
    }

    fn cte_windows(store: &Store, window_ms: i64) -> Vec<(i64, i64, i64)> {
        let mut statement = store.connection().prepare(PLAN_CTE).unwrap();
        let rows = statement
            .query_map(rusqlite::named_params! { ":window_ms": window_ms }, |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        rows.map(|row| row.unwrap()).collect()
    }

    fn assert_agrees(timeline: &[(i64, i64)], window_ms: i64) {
        let (store, db_path, log_path) = store_with(timeline);
        let folded: Vec<(i64, i64, i64)> = store
            .usage_blocks(window_ms, &UsageQuery::default())
            .unwrap()
            .into_iter()
            .filter(|block| !block.is_gap)
            .map(|block| (block.window_start, block.calls, block.total_tokens))
            .collect();
        let cte = cte_windows(&store, window_ms);
        assert_eq!(folded, cte, "fold and the plan's CTE disagree");
        let direct = fold_blocks(timeline, window_ms);
        assert_eq!(
            direct.iter().filter(|b| !b.is_gap).count(),
            cte.len(),
            "the pure fold must see the same window count"
        );
        drop(store);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&log_path);
    }

    fn hour(n: i64) -> i64 {
        n * 3_600_000
    }

    #[test]
    fn fold_and_cte_agree_on_one_busy_window() {
        assert_agrees(
            &[
                (hour(10) + 60_000, 100),
                (hour(10) + 120_000, 200),
                (hour(12), 300),
            ],
            hour(5),
        );
    }

    #[test]
    fn fold_and_cte_agree_across_a_gap() {
        assert_agrees(
            &[(hour(1), 10), (hour(9), 20), (hour(9) + 60_000, 30)],
            hour(5),
        );
    }

    #[test]
    fn fold_and_cte_agree_when_a_window_times_out_from_its_start() {
        let timeline: Vec<(i64, i64)> = (0..12).map(|n| (hour(1) + n * hour(1) / 2, 5)).collect();
        assert_agrees(&timeline, hour(2));
    }
}
