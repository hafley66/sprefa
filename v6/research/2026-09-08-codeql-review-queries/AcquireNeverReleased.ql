/**
 * @name Resource acquired and never released in a finally
 * @kind problem
 * @problem.severity warning
 * @id hafley/acquire-never-released
 */
import javascript

from VariableDeclarator d, Variable v, CallExpr acq
where
  d.getBindingPattern().(VarDecl).getVariable() = v and
  acq = d.getInit().(AwaitExpr).getOperand() and
  acq.getCalleeName() = "acquire" and
  not exists(MethodCallExpr rel, TryStmt t |
    rel.getMethodName() = "release" and rel.getReceiver().(VarAccess).getVariable() = v and
    rel.getEnclosingStmt().getParentStmt*() = t.getFinally())
select acq, "Handle " + v.getName() + " has no release() in any finally."
