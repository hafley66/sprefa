/**
 * Call edges for Go, resolved through the go type system.
 * DataFlow::CallNode.getTarget() is the frontend-resolved callee entity;
 * FuncDef.getACall().getACallee() (pass 1) matched on name only.
 * Names follow the oracle bare convention: method name without receiver type.
 * Same-file edges are kept: the callgraph oracle carries them too.
 * A FuncLit caller yields no name and drops out; the oracle spells those
 * callers `<enclosing>$<n>`, which codeql has no counterpart for.
 */
import go

from DataFlow::CallNode call, FuncDef caller, Function target, FuncDef callee
where
  caller = call.getRoot() and
  target = call.getTarget() and
  callee = target.getFuncDecl()
select caller.getFile().getRelativePath(), caller.getName(),
  callee.getFile().getRelativePath(), target.getName()
