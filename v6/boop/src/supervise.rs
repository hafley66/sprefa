//! The lane supervisor: the process a lane pane actually runs. It owns one
//! `LaneChannel` and the lane's inbox, so every harness is messaged the same.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::bus;
use crate::channel::{Delivery, LaneChannel};

/// How often the inbox is re-read while a turn runs.
const POLL: Duration = Duration::from_millis(700);

/// What one lane run needs.
pub struct LaneRun {
    pub lane: String,
    pub brief: PathBuf,
    pub mail_dir: PathBuf,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub resume: Option<String>,
}

/// One inbox message the supervisor has taken responsibility for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hail {
    pub id: String,
    pub from: String,
    pub body: String,
}

/// Every unacked message addressed to `lane` whose id is not already in
/// `seen`. Reading is enough to own it; the ack is written on delivery.
pub fn pending(dir: &Path, lane: &str, seen: &BTreeSet<String>) -> Result<Vec<Hail>> {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir)? {
        rows.extend(bus::parse_box(&path));
    }
    Ok(bus::unacked(&rows)
        .into_iter()
        .filter(|row| row.to == lane)
        .filter(|row| !seen.contains(&row.id))
        .filter(|row| deliverable(&row.kind))
        .map(|row| Hail {
            id: row.id,
            from: row.from,
            body: row.body,
        })
        .collect())
}

/// A lane acts on requests and hails. Its own dispatch row and result rows are
/// bookkeeping and would loop straight back into the agent's context.
fn deliverable(kind: &str) -> bool {
    matches!(kind, "request" | "hail" | "note" | "retry" | "resume")
}

/// The text one hail becomes inside the agent's conversation.
pub fn hail_text(hail: &Hail) -> String {
    format!("[boop hail {} from {}] {}", hail.id, hail.from, hail.body)
}

/// Run the lane to completion and return the exit code the pane re-raises.
pub fn run(lane: LaneRun, channel: &mut dyn LaneChannel) -> Result<i32> {
    let brief = std::fs::read_to_string(&lane.brief)
        .with_context(|| format!("read lane brief {}", lane.brief.display()))?;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut held: Vec<Hail> = Vec::new();
    let mut turn = brief;
    loop {
        channel.start_turn(&turn)?;
        remember_conversation(&lane, channel);
        let end = loop {
            if let Some(end) = channel.poll_turn(POLL)? {
                break end;
            }
            for hail in pending(&lane.mail_dir, &lane.lane, &seen)? {
                seen.insert(hail.id.clone());
                match channel.steer(&hail_text(&hail))? {
                    Delivery::MidTurn => {
                        println!("[boop] hail {} delivered midturn", hail.id);
                        record_delivery(&lane.mail_dir, &lane.lane, &hail, Delivery::MidTurn);
                    }
                    Delivery::NextTurn => {
                        println!("[boop] hail {} held for the next turn", hail.id);
                        held.push(hail);
                    }
                }
            }
        };
        println!("[boop] turn ended: {}", end.detail);
        remember_conversation(&lane, channel);
        for hail in pending(&lane.mail_dir, &lane.lane, &seen)? {
            seen.insert(hail.id.clone());
            held.push(hail);
        }
        if held.is_empty() {
            channel.close()?;
            return Ok(match end.ok {
                true => 0,
                false => 1,
            });
        }
        turn = held
            .drain(..)
            .map(|hail| {
                record_delivery(&lane.mail_dir, &lane.lane, &hail, Delivery::NextTurn);
                hail_text(&hail)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
    }
}

/// Pin the harness's current conversation to the lane route and to the lane's
/// trace. The id MOVES on `/clear`, on compaction and on resume; the trace does
/// not, so every id this lane ever wears lands under one trace.
fn remember_conversation(lane: &LaneRun, channel: &dyn LaneChannel) {
    let Some(id) = channel.conversation_id() else {
        return;
    };
    record_conversation(&lane.mail_dir, &lane.lane, &id);
    let Ok(store) = crate::Store::default_path().and_then(crate::Store::open) else {
        return;
    };
    let trace = store
        .trace_of(&lane.lane)
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("trace-{}", lane.lane));
    let _ = store.attach_trace(&lane.lane, &trace, "lane-run", crate::channel::now_ms());
    let _ = store.attach_trace(&id, &trace, "supervisor-conversation", crate::channel::now_ms());
}

/// Write the harness's own conversation id onto the lane's registry route so a
/// later resume finds it without a transcript scan.
fn record_conversation(dir: &Path, lane: &str, conversation: &str) {
    let path = dir.join("registry.json");
    let lane = lane.to_owned();
    let conversation = conversation.to_owned();
    let _ = bus::cas_update_json(&path, |map| {
        let entry = map
            .entry(lane.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(object) = entry.as_object_mut() {
            object.insert(
                "sessionId".into(),
                serde_json::Value::String(conversation.clone()),
            );
        }
        Ok(())
    });
}

/// Stamp the mailbox row delivered so no later read re-offers it.
pub fn ack(dir: &Path, hail: &Hail) {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    let Some(mut row) = bus::fold(&rows).into_iter().find(|row| row.id == hail.id) else {
        return;
    };
    row.to_timestamp = Some(bus::now_iso());
    let line = bus::message_line(&row);
    let path = dir.join("bus.ndjson");
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    use std::io::Write;
    let _ = writeln!(file, "{line}");
}

/// Ack plus a store edge naming the tier, so `boop db` answers "did the lane
/// get it, and did it land mid-turn".
fn record_delivery(dir: &Path, lane: &str, hail: &Hail, tier: Delivery) {
    ack(dir, hail);
    let Ok(store) = crate::Store::default_path().and_then(crate::Store::open) else {
        return;
    };
    let _ = store.add_edge_at(
        &hail.from,
        lane,
        &format!("deliver-{}", tier.as_str()),
        crate::channel::now_ms(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_box(dir: &Path, rows: &[bus::Message]) {
        use std::io::Write;
        let mut file = std::fs::File::create(dir.join("bus.ndjson")).unwrap();
        for row in rows {
            writeln!(file, "{}", bus::message_line(row)).unwrap();
        }
    }

    fn message(id: &str, to: &str, kind: &str) -> bus::Message {
        bus::Message {
            id: id.into(),
            from: "coordinator".into(),
            to: to.into(),
            from_timestamp: "2026-08-12T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: kind.into(),
            reply_to: None,
            body: format!("body of {id}"),
            r#ref: None,
        }
    }

    #[test]
    fn pending_takes_only_this_lane_s_unacked_actionable_rows() {
        let dir = tempdir();
        let mut acked = message("m3", "mine", "request");
        acked.to_timestamp = Some("2026-08-12T00:00:01.000Z".into());
        write_box(
            &dir,
            &[
                message("m1", "mine", "request"),
                message("m2", "other", "request"),
                acked,
                message("m4", "mine", "result"),
                message("m5", "mine", "hail"),
            ],
        );
        let rows = pending(&dir, "mine", &BTreeSet::new()).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["m1", "m5"]
        );
    }

    #[test]
    fn a_seen_id_is_not_offered_twice() {
        let dir = tempdir();
        write_box(&dir, &[message("m1", "mine", "request")]);
        let seen: BTreeSet<String> = ["m1".to_owned()].into_iter().collect();
        assert!(pending(&dir, "mine", &seen).unwrap().is_empty());
    }

    #[test]
    fn hail_text_names_the_id_and_the_sender() {
        let text = hail_text(&Hail {
            id: "m1".into(),
            from: "coordinator".into(),
            body: "stop and write /tmp/x".into(),
        });
        assert_eq!(text, "[boop hail m1 from coordinator] stop and write /tmp/x");
    }

    #[test]
    fn result_and_dispatch_rows_never_reach_the_agent() {
        assert!(!deliverable("result"));
        assert!(!deliverable("dispatch"));
        assert!(deliverable("request"));
        assert!(deliverable("hail"));
    }

    #[test]
    fn ack_stamps_the_row_so_the_next_read_skips_it() {
        let dir = tempdir();
        write_box(&dir, &[message("m1", "mine", "request")]);
        let hail = pending(&dir, "mine", &BTreeSet::new()).unwrap().remove(0);
        ack(&dir, &hail);
        assert!(pending(&dir, "mine", &BTreeSet::new()).unwrap().is_empty());
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boop-supervise-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
