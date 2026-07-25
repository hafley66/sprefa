// Package gamma exercises the go scip ratchet's three outcomes: a
// same-package call (NameResolve), a cross-package call through the import
// (name-match ambiguous across alpha/beta -> ScipOverride), and a stdlib
// call (external to the corpus -> no v6 edge).
package gamma

import (
	"strings"

	"example.com/fixture/alpha"
)

// Run calls local, then alpha.Helper through the import, then a stdlib func.
func Run() string {
	return local() + alpha.Helper() + strings.TrimSpace(" x ")
}

func local() string {
	return "gamma"
}
