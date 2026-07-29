// C7 spread inside a function signature in Go (the host-decl analog).
// EXPECTED: no parameter-list splice form exists; only a trailing variadic.
package main

import "fmt"

type CommonInputs struct {
	Repo string
	Rev  string
}

// (1) splicing a declared shape into the parameter list: no syntax
func fetchRow(CommonInputs..., endpoint string) (int64, string) {
	return 200, endpoint
}

// (2) the supported spelling passes the shape as ONE parameter
func fetchRow2(common CommonInputs, endpoint string) (int64, string) {
	return 200, common.Repo + endpoint
}

func main() {
	fmt.Println(fetchRow(CommonInputs{"r", "abc"}, "/stars"))
	fmt.Println(fetchRow2(CommonInputs{"r", "abc"}, "/stars"))
}
