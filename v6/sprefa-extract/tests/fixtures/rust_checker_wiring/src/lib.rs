pub mod decoys;
pub mod editor;
pub mod helpers;
pub mod widget;

use editor::Editor;
use widget::{Described, Widget};

/// A2: the receiver is a call result reached through `?`, so the parse types it
/// `Inferred`; the dst file is reached under `finish` and missed under `replace`.
pub fn drive_a2() -> Option<usize> {
    let editor = Editor::build()?;
    let counted = editor.finish();
    Some(counted + editor.replace())
}

/// T2: the receiver's type flows through a std container, so only the checker
/// can name the impl method.
pub fn drive_t2(items: &[Widget]) -> usize {
    items.first().unwrap().render()
}

/// T1: the same receiver shape, answered by a trait DEFAULT body rather than an
/// inherent impl.
pub fn drive_t1(items: &[Widget]) -> usize {
    items.first().unwrap().label()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A method call from a `#[cfg(test)]` body: the module tree carries the
    /// body only when the loader enables the `test` cfg.
    #[test]
    fn drive_cfg_test() {
        let items = vec![Widget { size: 3 }];
        let seen = items.first().unwrap().render();
        let helped = helpers::helper();
        assert_eq!(seen + helped, 10);
    }
}
