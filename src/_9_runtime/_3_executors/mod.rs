//! The executor roster: every served name maps to one Rust executor.

#[path = "doc_layout.rs"]
pub mod doc_layout;
#[path = "extract.rs"]
pub mod extract;
#[path = "fetch_json.rs"]
pub mod fetch_json;
#[path = "fs_at.rs"]
pub mod fs_at;
#[path = "fs_json.rs"]
pub mod fs_json;
#[path = "fs_watch.rs"]
pub mod fs_watch;
#[path = "git_history.rs"]
pub mod git_history;
#[path = "git_refs.rs"]
pub mod git_refs;
#[path = "http_fetch.rs"]
pub mod http_fetch;
#[path = "http_serve.rs"]
pub mod http_serve;
#[path = "timer.rs"]
pub mod timer;

use super::reconcile::{application_values, IService};
use crate::_6_eval::kernel::kernel_ref;
use crate::_6_eval::{Diagnostic, Term, TermId, Universe};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;

pub use doc_layout::DocLayout;
pub use extract::Extract;
pub use fetch_json::FetchJson;
pub use fs_at::FsAt;
pub use fs_json::FsJson;
pub use fs_watch::FsWatch;
pub use git_history::GitHistory;
pub use git_refs::GitRefs;
pub use http_fetch::HttpFetch;
pub use http_serve::HttpServe;
pub use timer::Timer;

/// The bound text value at `index` of an application, with its term.
pub(crate) fn text_at(u: &Universe, application: TermId, index: usize) -> Option<(TermId, String)> {
    let value = application_values(u, application)?
        .get(index)
        .copied()
        .flatten()?;
    match u.get(value) {
        Term::Str(sym) => Some((value, u.sym_str(*sym).to_string())),
        _ => None,
    }
}

/// The text of a string term.
pub(crate) fn text_of(u: &Universe, term: TermId) -> Option<String> {
    let Term::Str(sym) = u.get(term) else {
        return None;
    };
    Some(u.sym_str(*sym).to_string())
}

/// A string column value that names a stored blob is the blob.
pub(crate) fn body_of_term(u: &Universe, term: TermId) -> HttpBody {
    match text_of(u, term) {
        Some(value) => match blob_bytes(&value) {
            Some(_) => HttpBody::Blob(value),
            None => HttpBody::Text(value),
        },
        None => HttpBody::Text(String::new()),
    }
}

/// Every soopy error flattened with its causes.
pub(crate) fn message(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

/// The one multi-thread tokio runtime per process. `call` spawns its tasks
/// here; tasks hold only plain data and signal `wake` when a result has
/// landed on their service's channel.
static RUNTIME: LazyLock<Runtime> = LazyLock::new(Runtime::new);

struct Runtime {
    tokio: tokio::runtime::Runtime,
    /// The signal tasks send. `take_wake` mints a fresh pair per answering
    /// loop, so a task of a dropped loop signals a channel its receiver no
    /// longer reads.
    wake: Mutex<std::sync::mpsc::Sender<()>>,
}

impl Runtime {
    fn new() -> Runtime {
        Runtime {
            tokio: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime"),
            wake: Mutex::new(std::sync::mpsc::channel().0),
        }
    }
}

/// The shared runtime's spawn handle.
pub fn handle() -> &'static tokio::runtime::Handle {
    let runtime: &'static Runtime = &RUNTIME;
    runtime.tokio.handle()
}

/// Tasks call this once a result is on their service's channel: the
/// answering loop's bounded wait returns and `poll` drains.
pub fn wake() {
    let _ = RUNTIME.wake.lock().send(());
}

/// The receiver of the signals sent after this call. One answering loop at a
/// time holds it; `Answerer::new` takes it.
pub(crate) fn take_wake() -> std::sync::mpsc::Receiver<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    *RUNTIME.wake.lock() = sender;
    receiver
}

/// The bytes behind blob handles: response and request bodies too large or
/// too binary to live in a row. One map per process, keyed by blake3 hex.
static BLOBS: LazyLock<Mutex<HashMap<String, bytes::Bytes>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A body column value: text, or the blake3 handle of bytes in the blob map.
#[derive(Clone)]
pub(crate) enum HttpBody {
    Text(String),
    Blob(String),
}

/// The stored bytes behind a handle, if any.
pub(crate) fn blob_bytes(handle: &str) -> Option<bytes::Bytes> {
    BLOBS.lock().get(handle).cloned()
}

/// The lowercased media type of a `Content-Type` header value, without
/// parameters; empty when the header is absent.
pub(crate) fn media_type(content_type: Option<&str>) -> String {
    content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// Media types whose bytes read back as a string column value.
fn is_text_media(media: &str) -> bool {
    media.starts_with("text/")
        || media.contains("json")
        || media.contains("xml")
        || media.contains("javascript")
        || media.contains("x-www-form-urlencoded")
}

/// Text media keep their bytes as a string; anything else is stored once and
/// answered as its handle.
pub(crate) fn classify_body(media: &str, bytes: &[u8]) -> HttpBody {
    if is_text_media(media) {
        return HttpBody::Text(String::from_utf8_lossy(bytes).into_owned());
    }
    let handle = blake3::hash(bytes).to_hex().to_string();
    BLOBS
        .lock()
        .insert(handle.clone(), bytes::Bytes::copy_from_slice(bytes));
    HttpBody::Blob(handle)
}

/// The request bytes a body column value stands for.
pub(crate) fn body_bytes(body: &HttpBody) -> bytes::Bytes {
    match body {
        HttpBody::Text(text) => bytes::Bytes::from(text.clone()),
        HttpBody::Blob(handle) => blob_bytes(handle).unwrap_or_default(),
    }
}

/// The row cell of a body column value; both spellings are strings.
pub(crate) fn body_cell(u: &mut Universe, body: &HttpBody) -> TermId {
    match body {
        HttpBody::Text(text) => u.string(text),
        HttpBody::Blob(handle) => u.string(handle),
    }
}

/// A companion relation the executor writes (`fs_at_error`) must be
/// declared, so a missing one is a diagnostic at construction.
pub fn executors_for(
    u: &mut Universe,
    names: &HashMap<String, TermId>,
    served: &[String],
) -> Result<Vec<Box<dyn IService>>, Vec<Diagnostic>> {
    executors_for_mode(u, names, served, true)
}

/// Query executors settle one frontier. `git.refs` takes its opening snapshot
/// without creating the continuing runtime's repository watcher.
pub fn query_executors_for(
    u: &mut Universe,
    names: &HashMap<String, TermId>,
    served: &[String],
) -> Result<Vec<Box<dyn IService>>, Vec<Diagnostic>> {
    executors_for_mode(u, names, served, false)
}

fn executors_for_mode(
    u: &mut Universe,
    names: &HashMap<String, TermId>,
    served: &[String],
    continuing_refs: bool,
) -> Result<Vec<Box<dyn IService>>, Vec<Diagnostic>> {
    let mut executors: Vec<Box<dyn IService>> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut missing = |u: &mut Universe, reason: &str, name: &str| {
        let atom = u.atom(name);
        let payload = u.compound(reason, vec![atom]);
        diagnostics.push(Diagnostic {
            phase: "eval",
            payload,
        });
    };
    for name in served {
        match name.as_str() {
            timer::RELATION => match names.get(timer::RELATION) {
                Some(rel) => executors.push(Box::new(Timer::new(*rel))),
                None => missing(u, "served_relation_unknown", name),
            },
            doc_layout::RELATION => match names.get(doc_layout::RELATION) {
                Some(rel) => executors.push(Box::new(DocLayout::new(*rel))),
                None => missing(u, "served_relation_unknown", name),
            },
            fetch_json::RELATION => match (names.get(fetch_json::RELATION), names.get(fetch_json::ERROR)) {
                (Some(rel), Some(error)) => executors.push(Box::new(FetchJson::new(*rel, *error))),
                (None, _) => missing(u, "served_relation_unknown", name),
                (_, None) => missing(u, "executor_relation_unknown", fetch_json::ERROR),
            },
            http_fetch::RELATION => match names.get(http_fetch::RELATION) {
                Some(rel) => executors.push(Box::new(HttpFetch::new(*rel))),
                None => missing(u, "served_relation_unknown", name),
            },
            http_serve::RELATION => {
                match (
                    names.get(http_serve::RELATION),
                    names.get(http_serve::REQUEST),
                    names.get(http_serve::RESPONSE),
                    names.get(http_serve::ERROR),
                ) {
                    (Some(rel), Some(request), Some(response), Some(error)) => {
                        executors.push(Box::new(HttpServe::new(*rel, *request, *response, *error)))
                    }
                    (None, _, _, _) => missing(u, "served_relation_unknown", name),
                    (_, None, _, _) => missing(u, "executor_relation_unknown", http_serve::REQUEST),
                    (_, _, None, _) => {
                        missing(u, "executor_relation_unknown", http_serve::RESPONSE)
                    }
                    (_, _, _, None) => missing(u, "executor_relation_unknown", http_serve::ERROR),
                }
            }
            fs_json::RELATION => {
                match (
                    names.get(fs_json::RELATION),
                    names.get(fs_json::ERROR),
                    names.get(fs_json::NONE),
                ) {
                    (Some(rel), Some(error), Some(none)) => {
                        let (rel, error, none) = (*rel, *error, *none);
                        let colon = kernel_ref(u, ":");
                        executors.push(Box::new(FsJson::new(rel, error, colon, none)))
                    }
                    (None, _, _) => missing(u, "served_relation_unknown", name),
                    (_, None, _) => missing(u, "executor_relation_unknown", fs_json::ERROR),
                    (_, _, None) => missing(u, "executor_relation_unknown", fs_json::NONE),
                }
            }
            fs_watch::RELATION => match names.get(fs_watch::RELATION) {
                Some(rel) => executors.push(Box::new(FsWatch::new(*rel))),
                None => missing(u, "served_relation_unknown", name),
            },
            git_refs::RELATION | git_history::RELATION | fs_at::RELATION | extract::RELATION => {
                let error = match name.as_str() {
                    git_refs::RELATION => git_refs::ERROR,
                    git_history::RELATION => git_history::ERROR,
                    fs_at::RELATION => fs_at::ERROR,
                    _ => extract::ERROR,
                };
                match (names.get(name.as_str()), names.get(error)) {
                    (Some(rel), Some(error)) => executors.push(match name.as_str() {
                        git_refs::RELATION if continuing_refs => {
                            Box::new(GitRefs::new(*rel, *error))
                        }
                        git_refs::RELATION => Box::new(GitRefs::snapshot(*rel, *error)),
                        git_history::RELATION => Box::new(GitHistory::new(*rel, *error)),
                        fs_at::RELATION => Box::new(FsAt::new(*rel, *error)),
                        _ => Box::new(Extract::new(*rel, *error)),
                    }),
                    (None, _) => missing(u, "served_relation_unknown", name),
                    (_, None) => missing(u, "executor_relation_unknown", error),
                }
            }
            _ => missing(u, "served_relation_no_executor", name),
        }
    }
    match diagnostics.is_empty() {
        true => Ok(executors),
        false => Err(diagnostics),
    }
}

/// Every relation name `executors_for` builds an executor for.
pub const ROSTER: [&str; 11] = [
    doc_layout::RELATION,
    timer::RELATION,
    fetch_json::RELATION,
    http_fetch::RELATION,
    http_serve::RELATION,
    fs_json::RELATION,
    fs_watch::RELATION,
    git_refs::RELATION,
    git_history::RELATION,
    fs_at::RELATION,
    extract::RELATION,
];

/// The Once executors the program's names can build. A name whose companion
/// relation is undeclared builds nothing here; the runtime still reports it.
pub fn once_executors(u: &mut Universe, names: &HashMap<String, TermId>) -> Vec<Box<dyn IService>> {
    ROSTER
        .iter()
        .filter(|name| names.contains_key(**name))
        .filter_map(|name| executors_for(u, names, &[name.to_string()]).ok())
        .flatten()
        .filter(|executor| executor.cadence() == super::reconcile::Cadence::Once)
        .collect()
}
