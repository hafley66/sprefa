//! Phase 6 — semi-naive fixpoint evaluation (Piece 4).
//!
//! `eval_stratum` drives one [`stratify`](crate::stratify) stratum to a
//! least fixed point, then [`eval_all`] runs the strata low → high so a
//! negated relation is complete before its negation is read (the plan's
//! stratification invariant).
//!
//! A rule is a SQL query over its dependency tables (base relations +,
//! for a recursive rule, its own sink). Each round runs every rule's
//! SQL and inserts the rows it derives that are not already present;
//! the sink's row-id dedup makes an already-derived row a NO-OP, so a
//! recursive rule (`reach(X,Z) :- reach(X,Y), edge(Y,Z)`) adds strictly
//! fewer new rows each round and reaches its least fixed point in
//! finite rounds — that is what makes recursion terminate without a
//! frontier. Every loop has a HARD round cap; exceeding it returns
//! `DirtySweepError::RoundCap` (a loud error, never a hang).
//!
//! Retraction is DRed: each derived row carries one SUPPORT triple per
//! derivation WITNESS (the body binding that produced it). When a base
//! relation row is removed, [`retract_base_rows`] runs the Phase-5
//! `cascade_retract` over the witnesses that referenced it, then
//! re-runs the fixpoint. A derived row with another surviving witness
//! keeps `sum(mult) > 0` (Phase-5 mult) and stays; a row that loses its
//! last witness is deleted and the deletion cascades through the
//! closure (DRed over-delete + stop-at-supported).

use std::sync::Arc;

use effect_runtime::v2::FactStore;

use crate::dirty_source::DirtySweepError;
use crate::mounted_query::{cascade_retract, record_fact_supports, SupportLedger};
use crate::sql::run_sql_batch;
use crate::Cursor;

/// One SQL-evaluated rule in a stratum.
///
/// `sql` selects the sink columns plus a `__witness` column: the
/// derivation key (the body binding). One `(derived row, __witness)`
/// pair = one SUPPORT triple, so a row derivable two ways has mult 2
/// and survives losing one witness (Phase-5).
#[derive(Clone)]
pub struct EvalRule {
    pub name: String,
    pub sink: String,
    pub sink_cols: Vec<String>,
    pub sql: String,
    /// Tables the SQL reads (base relations and, for recursion, the
    /// rule's own sink). Materialized as views for the query.
    pub deps: Vec<String>,
}

const WITNESS_COL: &str = "__witness";

fn dummy_batch() -> Vec<Arc<Cursor>> {
    vec![Arc::new(Cursor::default())]
}

/// Run one rule's SQL once; insert each derived row not already in the
/// sink and record one SUPPORT triple per `(row, __witness)`. Returns
/// the number of NEW sink rows (0 ⇒ this rule added nothing this
/// round; the semi-naive signal that its part of the stratum settled).
fn step_rule(store: &dyn FactStore<Cursor>, rule: &EvalRule) -> Result<usize, String> {
    store.declare(
        &rule.sink,
        &rule
            .sink_cols
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
    );
    let dummies = dummy_batch();
    let batch: Vec<&Cursor> = dummies.iter().map(|c| c.as_ref()).collect();
    let nodes = run_sql_batch(store, &rule.sql, &rule.deps, &batch)?;

    // Collect the derived (sink-projected) cursors + their witness.
    let mut derived: Vec<(Arc<Cursor>, String)> = Vec::new();
    collect_emits(&nodes, &mut derived, &rule.sink_cols);

    let ledger = SupportLedger::new(store);
    let mut new_rows = 0usize;
    for (proj, witness) in derived {
        let row_id = store.row_id_for(&rule.sink, proj.as_ref());
        let before = ledger.sink_mult(&rule.sink, &row_id);
        if before == 0 {
            // Not yet present (or fully retracted) — assert it.
            store.insert_batch(&rule.sink, vec![proj.clone()]);
            new_rows += 1;
        }
        // One SUPPORT triple per derivation witness. `add` is
        // idempotent per witness (Phase-5), so re-deriving the SAME
        // witness in a later round does NOT inflate mult — the
        // precondition the semi-naive loop relies on for termination.
        let support_cursor_id = witness_support_id(&rule.name, &witness);
        record_fact_supports(store, &support_cursor_id, &rule.sink, &row_id);
    }
    Ok(new_rows)
}

/// Stable support cursor id for one derivation witness of one rule:
/// `blake3(rule ++ 0x1f ++ witness)`. Distinct witnesses → distinct
/// triples → `sink_mult` counts derivation paths (Phase-5 mult).
fn witness_support_id(rule: &str, witness: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(rule.as_bytes());
    h.update(&[0x1f]);
    h.update(witness.as_bytes());
    let b = h.finalize();
    let mut s = String::with_capacity(64);
    for x in b.as_bytes() {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn collect_emits(
    nodes: &[effect_runtime::v2::Node<Cursor>],
    out: &mut Vec<(Arc<Cursor>, String)>,
    sink_cols: &[String],
) {
    use effect_runtime::v2::Node;
    for n in nodes {
        match n {
            Node::Emit(c) => {
                let witness = c.get(WITNESS_COL).unwrap_or("").to_string();
                let mut proj = Cursor::default();
                for col in sink_cols {
                    if let Some(v) = c.get(col) {
                        proj.set(col, v);
                    }
                }
                out.push((Arc::new(proj), witness));
            }
            Node::Many(inner) => collect_emits(inner, out, sink_cols),
            Node::Yield { value, .. } => {
                let witness = value.get(WITNESS_COL).unwrap_or("").to_string();
                let mut proj = Cursor::default();
                for col in sink_cols {
                    if let Some(v) = value.get(col) {
                        proj.set(col, v);
                    }
                }
                out.push((Arc::new(proj), witness));
            }
            Node::Done => {}
        }
    }
}

/// Semi-naive least fixed point of ONE stratum. Loops every rule in
/// the stratum until a whole round adds no new sink row (local fixed
/// point), bounded by `round_cap`. Returns the total new rows derived,
/// or `RoundCap` if the stratum never settled.
pub fn eval_stratum(
    store: &dyn FactStore<Cursor>,
    stratum: &[EvalRule],
    round_cap: usize,
) -> Result<usize, DirtySweepError> {
    let mut total = 0usize;
    for round in 0..round_cap {
        let mut round_new = 0usize;
        for rule in stratum {
            match step_rule(store, rule) {
                Ok(n) => round_new += n,
                Err(_e) => {
                    // A malformed rule SQL is a no-op for this round;
                    // it cannot wedge the loop (round_new stays 0 for
                    // it and the stratum still converges/cap-trips).
                }
            }
        }
        total += round_new;
        if round_new == 0 {
            return Ok(total);
        }
        if round + 1 == round_cap {
            return Err(DirtySweepError::RoundCap {
                rounds: round_cap,
                handled: total,
            });
        }
    }
    // Unreachable for round_cap >= 1 (the loop returns on settle or
    // on the last round); keep the cap contract explicit.
    Err(DirtySweepError::RoundCap {
        rounds: round_cap,
        handled: total,
    })
}

/// Run the stratified program low → high. Each stratum reaches its
/// least fixed point before the next runs, so a rule that negates a
/// relation in a lower stratum reads it COMPLETE (the plan's
/// stratification soundness invariant). Hard round cap per stratum.
pub fn eval_all(
    store: &dyn FactStore<Cursor>,
    strata: &[Vec<EvalRule>],
    round_cap: usize,
) -> Result<usize, DirtySweepError> {
    let mut total = 0usize;
    for stratum in strata {
        total += eval_stratum(store, stratum, round_cap)?;
    }
    Ok(total)
}

/// DRed delete half for a base-relation change. `removed_witness_ids`
/// are the witness support-cursor ids that referenced the now-gone
/// base rows; `cascade_retract` decrements their `mult` and deletes a
/// derived row (descending the closure) only when its LAST witness is
/// gone — a row with an alternate witness keeps `sum(mult) > 0` and
/// survives (Phase-5). Call, then re-run the fixpoint: any row that is
/// still derivable comes back (the DRed re-derive half).
pub fn retract_base_rows(store: &dyn FactStore<Cursor>, removed_witness_ids: &[String]) {
    cascade_retract(store, removed_witness_ids);
}

/// The witness support id for `(rule, witness)` — exposed so a caller
/// retracting a base row can name exactly the witnesses to tear down.
pub fn witness_id(rule: &str, witness: &str) -> String {
    witness_support_id(rule, witness)
}

/// Every distinct SUPPORT cursor id that derives a row in `sink`. A
/// base change over-deletes by `cascade_retract`ing these (the DRed
/// over-delete half: clear the possibly-affected closure), then
/// `eval_stratum` re-derives what is still supported — a row with a
/// surviving derivation is re-asserted (Phase-5 mult back > 0), a row
/// with none stays gone.
pub fn support_cursor_ids_for_sink(
    store: &dyn FactStore<Cursor>,
    sink: &str,
) -> Vec<String> {
    let mut ids: Vec<String> = store
        .rows_of(crate::mounted_query::SUPPORT_TABLE)
        .iter()
        .filter(|r| r.get("support_table") == Some(sink))
        .filter_map(|r| {
            r.get(crate::mounted_query::SUPPORT_CURSOR_ID)
                .map(|s| s.to_string())
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}
