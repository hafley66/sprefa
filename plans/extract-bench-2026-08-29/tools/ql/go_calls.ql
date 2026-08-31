/**
 * Cross-file call edges for Go: caller function file/name -> callee file/name.
 */
import go

from FuncDef caller, FuncDef callee
where
  exists(caller.getACall()) and
  callee = caller.getACall().getACallee().(FuncDef) and
  caller.getFile() != callee.getFile()
select caller.getFile().getRelativePath(), caller.getName(), callee.getFile().getRelativePath(), callee.getName()
