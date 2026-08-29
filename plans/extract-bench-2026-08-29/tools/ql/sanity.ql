import javascript
from ImportDeclaration imp, Module m
where m = imp.getImportedModule() and m.getFile() instanceof File
select imp.getFile().getRelativePath(), m.getFile().getRelativePath()
