//! `(fetch_json ?Url ?Body)`: every pending GET in flight at once on the blocking pool. A 2xx
//! JSON body lands as the raw text; anything else lands as `(fetch_json_error Url Status Message)`.

use super::{handle, wake};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IService};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub const RELATION: &str = "fetch_json";
pub const ERROR: &str = "fetch_json_error";

/// The 10-second law: a slower server is an error row, never a hang.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Status 0 marks a transport failure, where no response arrived.
const NO_RESPONSE: i64 = 0;

pub struct FetchJson {
    relation: TermId,
    error: TermId,
    agent: ureq::Agent,
    /// Finished fetches, plain data until `poll` mints terms.
    done: Receiver<(String, Fetched)>,
    sender: Sender<(String, Fetched)>,
    /// Fetches started and not yet drained.
    tasks: usize,
}

enum Fetched {
    Body(String),
    Failed(i64, String),
}

impl FetchJson {
    pub fn new(relation: TermId, error: TermId) -> FetchJson {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .build();
        let (sender, done) = channel();
        FetchJson {
            relation,
            error,
            agent: ureq::Agent::new_with_config(config),
            done,
            sender,
            tasks: 0,
        }
    }
}

fn fetch(agent: &ureq::Agent, url: &str) -> Fetched {
    let mut response = match agent.get(url).call() {
        Ok(response) => response,
        Err(e) => return Fetched::Failed(NO_RESPONSE, e.to_string()),
    };
    let status = i64::from(response.status().as_u16());
    let text = match response.body_mut().read_to_string() {
        Ok(text) => text,
        Err(e) => return Fetched::Failed(status, e.to_string()),
    };
    if !response.status().is_success() {
        return Fetched::Failed(status, format!("http status {status}"));
    }
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(_) => Fetched::Body(text),
        Err(e) => Fetched::Failed(status, format!("body is not json: {e}")),
    }
}

impl IService for FetchJson {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let Some(url) = application_values(u, application)
                .and_then(|values| values.first().copied().flatten())
            else {
                continue;
            };
            let Term::Str(sym) = u.get(url) else {
                continue;
            };
            let text = u.sym_str(*sym).to_string();
            let agent = self.agent.clone();
            let sender = self.sender.clone();
            self.tasks += 1;
            handle().spawn_blocking(move || {
                let fetched = fetch(&agent, &text);
                let _ = sender.send((text, fetched));
                wake();
            });
        }
    }

    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        while let Ok((text, fetched)) = self.done.try_recv() {
            self.tasks -= 1;
            tracing::info!(target: "dl8::fetch_json", url = %text, ok = matches!(fetched, Fetched::Body(_)));
            let url = u.string(&text);
            let url_cell = u.compound("const", vec![url]);
            rows.push(match fetched {
                Fetched::Body(body) => {
                    let body = u.string(&body);
                    Row {
                        rel: self.relation,
                        args: vec![url_cell, u.compound("const", vec![body])],
                    }
                }
                Fetched::Failed(status, message) => {
                    let status = u.int(status);
                    let message = u.string(&message);
                    Row {
                        rel: self.error,
                        args: vec![
                            url_cell,
                            u.compound("const", vec![status]),
                            u.compound("const", vec![message]),
                        ],
                    }
                }
            });
        }
        rows
    }

    fn in_flight(&self) -> usize {
        self.tasks
    }

    fn armed(&self) -> bool {
        false
    }
}
