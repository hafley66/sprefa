/**
 * @name Resource acquired outside the try that releases it, with a call in between
 * @description A handle from acquire() is released in a finally, but between the acquire and that try another call runs, so a throw there leaks the handle. The idiom `const h = await acquire(); try {...} finally { h.release() }` is accepted.
 * @kind problem
 * @problem.severity warning
 * @id hafley/acquire-before-try
 */
import javascript

from VariableDeclarator d, Variable v, CallExpr acq, TryStmt t, MethodCallExpr rel, BlockStmt b, int i, int j
where
  d.getBindingPattern().(VarDecl).getVariable() = v and
  acq = d.getInit().(AwaitExpr).getOperand() and
  acq.getCalleeName() = "acquire" and
  rel.getMethodName() = "release" and
  rel.getReceiver().(VarAccess).getVariable() = v and
  rel.getEnclosingStmt().getParentStmt*() = t.getFinally() and
  b.getStmt(i) = d.getEnclosingStmt() and b.getStmt(j) = t and j > i and
  exists(int k, InvokeExpr call | k > i and k < j and call.getEnclosingStmt().getParentStmt*() = b.getStmt(k))
select acq, "Acquired here; a call runs before the try at line " + t.getLocation().getStartLine() + " whose finally releases it."
