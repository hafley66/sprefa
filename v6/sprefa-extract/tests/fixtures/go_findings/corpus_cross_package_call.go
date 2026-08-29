// Expected: the call to helper() resolves across the package boundary to the
// imported package's function and emits resolved_edge {caller: main, callee: helper}.
// Observed on typescript-go: parse-only --resolve has no import handling, so a
// pkg-qualified call into another package never resolves (727+ sites in
// internal/lsp/lsproto/lsp_generated.go alone; dominant unresolved class).
package main

import "fmt"

func main() {
	fmt.Println("x")
}
