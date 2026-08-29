// Option 2 probe: link ra_ap_hir_expand into the binary and measure cost.
// A real integration needs a salsa db implementing base_db::SourceDatabase
// (file loader, crate graph, source roots) plus, for proc macros, a running
// proc-macro server process. This file only proves the link and records the
// surface area a real integration must fill.
#[allow(dead_code)]
pub fn probe() -> usize {
    let _f: fn(ra_ap_base_db::FileId) -> Option<ra_ap_hir_expand::HirFileId> = |_| None;
    std::mem::size_of::<ra_ap_hir_expand::HirFileId>()
}
