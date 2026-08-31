package main

import alias "example.com/b/pkgutil"

func UseAlias() int { return alias.Helper() }

// helper is unexported: pkg.helper is invisible from outside its package, so
// this call site drops with no edge and no unresolved row.
func UseUnexported() int { return alias.helper() }
