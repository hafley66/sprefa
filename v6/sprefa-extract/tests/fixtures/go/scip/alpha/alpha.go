// Package alpha is one of two same-name Helper providers, so the AST
// name-match across the corpus is ambiguous and scip's import-aware
// resolution is what settles a call through the import (ScipOverride).
package alpha

// Helper answers with the alpha marker.
func Helper() string {
	return "alpha"
}
