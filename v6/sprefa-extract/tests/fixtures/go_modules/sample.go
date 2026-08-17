// go_modules/sample.go: one line per row of the module-specifier mapping.
// ASCII-only so byte offsets stay simple.

package sample

import "fmt"

import (
	"os"
	alias "path/filepath"
	_ "embed"
	. "strings"
)

func Use(count int) int {
	return count
}
