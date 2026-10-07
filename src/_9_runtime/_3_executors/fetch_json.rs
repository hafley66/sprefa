//! `(fetch_json ?Url ?Body)`: one blocking GET per application. A 2xx JSON body
//! lands as the raw text; anything else lands as `(fetch_json_error Url Status Message)`.

use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IService};
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
    /// Fetches answered inside `call`, drained by `poll`.
    done: Vec<Row>,
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
        FetchJson {
            relation,
            error,
            agent: ureq::Agent::new_with_config(config),
            done: Vec::new(),
        }
    }

    fn fetch(&self, url: &str) -> Fetched {
        let mut response = match self.agent.get(url).call() {
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
            let fetched = self.fetch(&text);
            tracing::info!(target: "dl8::fetch_json", url = %text, ok = matches!(fetched, Fetched::Body(_)));
            let url_cell = u.compound("const", vec![url]);
            self.done.push(match fetched {
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
    }

    fn poll(&mut self, _u: &mut Universe) -> Vec<Row> {
        std::mem::take(&mut self.done)
    }

    fn armed(&self) -> bool {
        false
    }
}
