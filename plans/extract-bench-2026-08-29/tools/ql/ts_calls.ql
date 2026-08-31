/**
 * Cross-file call edges: caller function file/name -> callee function file/name.
 * CallNode.getACallee(2): level 1 resolves only same-file callee sets on this
 * namespace-style corpus; level 2 adds global-variable heuristic resolution.
 */
import javascript

from DataFlow::InvokeNode invoke, Function caller, Function callee
where
  callee = invoke.getACallee(2) and
  caller = invoke.getEnclosingFunction() and
  caller.getFile() != callee.getFile()
select caller.getFile().getRelativePath(), caller.getName(), callee.getFile().getRelativePath(), callee.getName()
