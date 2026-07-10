// Canonical permission keys. The string "can_export" lives HERE, once — the
// enforcement sites downstream reference PERM_EXPORT, not the literal string,
// so grepping for "can_export" finds this definition but none of the gates.
export const PERM_EXPORT = "can_export";
export const PERM_IMPORT = "can_import";
export const PERM_VIEW = "can_view";
