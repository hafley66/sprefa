// One call shape per import form, so the site-minting rule (a selector whose
// operand is a name an import binds carries the import path) has one row per
// case. Expected sites, in tree order, are pinned in tests/51_go_package_resolve.rs.
// ASCII-only so tree-sitter byte spans round-trip cleanly.

package findings

import (
	"strings"

	"example.com/m/alpha"
	a2 "example.com/m/beta"
	_ "example.com/m/side"
	. "example.com/m/dotted"
)

type box struct{ name string }

func (b box) Method() string { return b.name }

func Run(b box) string {
	_ = alpha.Helper()
	_ = a2.Only()
	_ = strings.TrimSpace("x")
	_ = b.Method()
	_ = Dotted()
	_ = side.Skipped()
	_ = alpha.inner.Deep()
	return local()
}

func local() string { return "" }
