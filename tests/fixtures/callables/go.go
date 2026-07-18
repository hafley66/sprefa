// Fixture: every EMITTED Go callable kind for examples/callable-coverage.dl.
// func declaration -> function; method (receiver) -> method; func literal ->
// lambda.

package callables

// free function -> function
func FreeFunction(seed int) int {
	// func literal bound to a variable -> lambda
	double := func(factor int) int {
		return factor * 2
	}
	total := 0
	// func literal passed to a call -> lambda
	apply(func(value int) {
		total += value + seed
	})
	return double(total)
}

func apply(fn func(int)) {
	fn(1)
}

type Widget struct {
	size int
}

// method (receiver) -> method
func (w Widget) Area() int {
	return w.size * w.size
}
