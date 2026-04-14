//! Runner trait. Orchestrator; owns lifecycle and RunEvent stream.

use std::sync::Arc;

use futures_core::stream::BoxStream;

use crate::_0_types::RunEvent;
use crate::_2_config::{Config, ConfigDiff};

pub trait Runner: Send + Sync {
    fn run_immediate(&self, sprf_src: &str) -> BoxStream<'static, RunEvent>;
    fn run_daemon   (&self, sprf_src: &str) -> BoxStream<'static, RunEvent>;
    fn run_check    (&self, sprf_src: &str) -> BoxStream<'static, RunEvent>;
    fn run_rewrite  (&self, sprf_src: &str) -> BoxStream<'static, RunEvent>;

    fn apply_config(&self, new: Arc<Config>) -> ConfigDiff;
}
