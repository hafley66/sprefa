/**
 * @name require() in an ES module
 * @kind problem
 * @problem.severity error
 * @id hafley/require-in-esm
 */
import javascript

from CallExpr c
where c.getCalleeName() = "require" and exists(ImportDeclaration i | i.getTopLevel() = c.getTopLevel())
select c, "require() in a file that uses import; undefined under node ESM."
