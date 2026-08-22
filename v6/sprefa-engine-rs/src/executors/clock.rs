//! `/clock/tick(every) -> (bucket)`: a monotonic bucket counter, re-called by
//! run.rs's resident loop; each new bucket is a tick.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hosts::{ExecutorCadence, HostError, IHostExecutor};
use crate::types::HostRow;

use super::{required_input, row};

pub struct ClockExecutor;

impl ClockExecutor {
    pub fn new() -> ClockExecutor {
        ClockExecutor
    }

    /// The bucket a period turns over on: whole seconds since the epoch,
    /// floor-divided by `every`, matching the retired `IntervalClocks::now`.
    pub fn bucket_of(every: i64) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        now / every.max(1)
    }
}

impl IHostExecutor for ClockExecutor {
    fn cadence(&self) -> ExecutorCadence {
        ExecutorCadence::Continuing
    }

    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let every: i64 = required_input(host, env, &["every"])?
            .parse()
            .map_err(|_| super::host_error(host, "`every` wants a positive integer"))?;
        if every <= 0 {
            return Err(super::host_error(host, "`every` wants a positive integer"));
        }
        Ok(vec![row([
            ("bucket", serde_json::json!(ClockExecutor::bucket_of(every))),
        ])])
    }
}
