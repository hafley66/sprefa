//! Phase 3 of `v4/plans/lsp-diags-to-claude-code-full.md`: minimal
//! Watchman ingress. ONE `connect()` and ONE SCM-aware query. Used
//! during `lsp_pre_warm` for telemetry; Phase 6 will own subscriptions
//! and the WakeEvent ingress.

use std::path::{Path, PathBuf};

use watchman_client::{
    expr::Expr,
    fields::NameOnly,
    pdu::{Clock, ClockSpec, FatClockData, QueryRequestCommon, ScmAwareClockData},
    CanonicalPath, Connector, Error,
};

pub struct WatchmanIngress {
    client: watchman_client::Client,
}

impl WatchmanIngress {
    pub async fn connect() -> Result<Self, Error> {
        let client = Connector::new().connect().await?;
        Ok(Self { client })
    }

    /// Best-effort SCM-aware diff vs the merge-base with `branch`.
    /// Returns:
    /// * `Ok(Some(paths))` on success (paths are absolute, rooted at `root`).
    /// * `Ok(None)` when Watchman is reachable but the SCM query cannot
    ///   complete for this repo (no merge base, non-VCS root, etc.).
    /// * `Err(_)` on transport / server failure.
    pub async fn try_scm_changed_since(
        &self,
        root: &Path,
        branch: &str,
        suffixes: &[&str],
    ) -> Result<Option<Vec<PathBuf>>, Error> {
        let canon = match CanonicalPath::canonicalize(root) {
            Ok(c) => c,
            Err(e) => return Err(Error::ConnectionError(e)),
        };
        let resolved = self.client.resolve_root(canon).await?;

        let scm_clock = Clock::ScmAware(FatClockData {
            clock: ClockSpec::null(),
            scm: Some(ScmAwareClockData {
                mergebase: None,
                mergebase_with: Some(branch.to_string()),
                saved_state: None,
            }),
        });

        let query = QueryRequestCommon {
            expression: Some(Expr::Suffix(
                suffixes.iter().map(|s| PathBuf::from(*s)).collect(),
            )),
            since: Some(scm_clock),
            ..Default::default()
        };

        let qr: watchman_client::pdu::QueryResult<NameOnly> = match self
            .client
            .query::<NameOnly>(&resolved, query)
            .await
        {
            Ok(qr) => qr,
            Err(Error::WatchmanResponseError { message, .. }) => {
                tracing::warn!(
                    target = "watchman",
                    message = %message,
                    "SCM query unsupported; pre-warm degrades to walker only",
                );
                return Ok(None);
            }
            Err(Error::WatchmanServerError { message, command, .. }) => {
                tracing::warn!(
                    target = "watchman",
                    message = %message,
                    command = %command,
                    "Watchman server error; pre-warm degrades to walker only",
                );
                return Ok(None);
            }
            Err(e) => return Err(e),
        };

        let root_path = resolved.path();
        let files = qr
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|nf| {
                let rel: PathBuf = nf.name.into_inner();
                if rel.is_absolute() {
                    rel
                } else {
                    root_path.join(rel)
                }
            })
            .collect();
        Ok(Some(files))
    }
}
