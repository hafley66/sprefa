/**
 * 0_ast_bridge.ts — .dl text -> ast.ts Program + HostDecl[] + minted stage rels.
 *
 * Contract (plan M1, tasks.d.ts): `bridge(dlText, extraRels?) -> BridgeOk | BridgeErr`.
 * Langium parses; this file maps the Langium AST onto the store's ast.ts constructors,
 * mints the probe timecut (`h?(inputs.., outputs..)` -> __req_h rule + __resp_h EDB ref,
 * Lloyd-Topor free-variable law), rewrites literal-binding equalities (`"warn" = severity`
 * with severity otherwise unbound) into minted single-row constant rels `__lit_<n>`, and
 * applies the diag head-default law (end_line:=line, end_col:=col, hint:=null,
 * severity:="warn", code:=null when unbound). Pure: LangiumDocument per call, discarded.
 *
 * Owned by package M1 (grammar bridge). Placeholder until that package lands.
 */
export {};
