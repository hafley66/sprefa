/**
 * @name Calls whose CFG successor is a finally block
 * @description Receipt that the CFG carries exceptional edges: a call inside a try whose successor is the finally.
 * @kind problem
 * @problem.severity recommendation
 * @id hafley/exceptional-edge-into-finally
 */
import javascript

from TryStmt t, InvokeExpr call, ControlFlowNode succ
where
  call.getEnclosingStmt().getParentStmt*() = t.getBody() and
  succ = call.getASuccessor() and
  (succ = t.getFinally() or succ.(Stmt).getParentStmt*() = t.getFinally() or succ.(Expr).getEnclosingStmt().getParentStmt*() = t.getFinally())
select call, "CFG successor lands in the finally of the try at line " + t.getLocation().getStartLine() + " (exceptional edge)."
