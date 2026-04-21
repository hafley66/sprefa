pub mod mem;
pub mod locator;
pub mod git;
pub mod buffer;
pub mod parse_cache;
pub mod worktree_provisioner;
pub mod path_index;
pub mod rev_class;

pub use mem::MemReader;
pub use locator::{CheckoutLocator, ConfigLocator, InMemoryLocator};
pub use git::GitBlobReader;
pub use buffer::{BufferOverlay, BufferKey};
pub use parse_cache::{ParseCacheReader, RsAstPayload};
pub use worktree_provisioner::{WorktreeProvisioner, WtHandle, ProvErr};
pub use path_index::{PathIndex, SqlxPathIndex, NoopPathIndex, PathIndexErr};
pub use rev_class::{classify, RevKind};

use std::path::PathBuf;
use std::sync::Arc;

/// Build (provisioner, path_index) from env:
///   - `SPREFA_WT_CACHE_DIR`   enables worktree materialization (phase 2)
///   - `SPREFA_PATH_INDEX_DB`  enables SqlxPathIndex (phase 1 + 2 cache)
/// Either unset short-circuits; failures are eprintln'd and degrade to `None`
/// so a broken cache never blocks startup.
pub async fn build_index_stack()
    -> (Option<Arc<WorktreeProvisioner>>, Option<Arc<dyn PathIndex>>)
{
    use tracing::{info_span, Instrument};
    let _s = info_span!("build_index_stack").entered();
    let wt_dir = std::env::var_os("SPREFA_WT_CACHE_DIR")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty());
    let pi_db = std::env::var_os("SPREFA_PATH_INDEX_DB")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty());

    let provisioner = wt_dir.map(|dir| Arc::new(WorktreeProvisioner::new(dir)));

    // Drop non-async guard before the .await below; a sync entered() guard
    // held across await would be incorrect.
    drop(_s);
    let path_index: Option<Arc<dyn PathIndex>> = match pi_db {
        Some(p) => match SqlxPathIndex::open(&p)
            .instrument(info_span!("build_index_stack.sqlx_open"))
            .await {
            Ok(idx) => Some(idx as Arc<dyn PathIndex>),
            Err(e)  => {
                eprintln!("[sprefa] path_index open {} failed: {e}; disabling", p.display());
                None
            }
        },
        None => None,
    };

    (provisioner, path_index)
}
