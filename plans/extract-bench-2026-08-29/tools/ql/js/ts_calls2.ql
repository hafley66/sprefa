/**
 * Call edges for TypeScript, resolved through the TypeScript type system.
 * InvokeExpr.getResolvedCallee() is TypeResolution::callTarget, populated only
 * for files extracted with full TypeScript extraction; pass 1 used
 * DataFlow::InvokeNode.getACallee(2), a global-variable heuristic.
 * Caller naming matches oracle_ts.mjs: nearest named enclosing function,
 * else `<module>`.
 */
import javascript

string nearestName(Function fn) {
  result = fn.getName()
  or
  not exists(fn.getName()) and
  (
    result = nearestName(fn.getEnclosingContainer().(Function))
    or
    not fn.getEnclosingContainer() instanceof Function and result = "<module>"
  )
}

string callerName(InvokeExpr invoke) {
  result = nearestName(invoke.getEnclosingFunction())
  or
  not exists(invoke.getEnclosingFunction()) and result = "<module>"
}

string calleeName(Function fn) {
  exists(ConstructorDeclaration ctor | ctor.getBody() = fn |
    result = ctor.getDeclaringClass().getName() + ".constructor"
  )
  or
  not exists(ConstructorDeclaration ctor | ctor.getBody() = fn) and result = fn.getName()
}

from InvokeExpr invoke, Function callee
where callee = invoke.getResolvedCallee()
select invoke.getFile().getRelativePath(), callerName(invoke),
  callee.getFile().getRelativePath(), calleeName(callee)
