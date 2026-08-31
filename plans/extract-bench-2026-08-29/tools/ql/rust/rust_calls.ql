/**
 * Call edges for Rust, resolved through the codeql rust type inference.
 * Call.getStaticTarget() is TypeInference::resolveCallTarget, the frontend's
 * own binding for a call site.
 * Caller naming matches ra_ide_probe: the nearest enclosing named fn; a
 * ClosureExpr caller reports its enclosing fn, which is the same mirror the
 * projection's `--closure enclosing` leg collapses.
 * Names are bare, receiver type dropped, matching rust.oracle.call.tsv.
 */

import rust

Function enclosingFunction(AstNode node) {
  result = node.getEnclosingCallable()
  or
  exists(ClosureExpr closure |
    closure = node.getEnclosingCallable() and
    result = enclosingFunction(closure)
  )
}

from Call call, Function caller, Function callee
where
  caller = enclosingFunction(call) and
  callee = call.getStaticTarget() and
  exists(callee.getName())
select call.getFile().getRelativePath(), caller.getName().getText(),
  callee.getFile().getRelativePath(), callee.getName().getText()
