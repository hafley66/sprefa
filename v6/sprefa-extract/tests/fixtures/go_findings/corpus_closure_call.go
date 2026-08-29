// Expected: the f() site carries callee f with a span covering the callee
// expression and resolves to the closure / func-valued parameter's node.
// Observed on typescript-go: the callee-name site span is 1 byte (just `f`)
// and no resolved_edge is emitted for the f() call in run(); the run(f) call
// does resolve. Same shape at the 9 checker.go sites calling func-typed
// parameters (f(u) style in type walkers).
package main

func run(f func() int) int {
	return f()
}

func main() {
	f := func() int { return 1 }
	_ = run(f)
}
