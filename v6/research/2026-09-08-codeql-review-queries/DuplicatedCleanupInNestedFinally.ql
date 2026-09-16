/**
 * @name Cleanup call repeated in an enclosing finally
 * @description The same method call appears in the finally of a try and in the finally of an enclosing try, so it runs twice on the normal path.
 * @kind problem
 * @problem.severity warning
 * @id hafley/duplicated-cleanup-nested-finally
 */
import javascript

string callText(MethodCallExpr c) { result = c.getReceiver().(VarAccess).getName() + "." + c.getMethodName() }

from TryStmt inner, TryStmt outer, MethodCallExpr a, MethodCallExpr b
where
  inner.getParentStmt+() = outer.getBody() and
  a.getEnclosingStmt().getParentStmt*() = inner.getFinally() and
  b.getEnclosingStmt().getParentStmt*() = outer.getFinally() and
  not a.getEnclosingStmt().getParentStmt*() = outer.getFinally().getAChildStmt+().(TryStmt).getFinally() and
  callText(a) = callText(b) and
  a != b
select b, "Cleanup " + callText(b) + " also runs in the inner finally at line " + a.getLocation().getStartLine() + "."
