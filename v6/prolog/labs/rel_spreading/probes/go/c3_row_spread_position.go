// C3 row spread in a call position in Go: the spread must be the FINAL
// argument, so a spread followed by explicit arguments is refused.
package main

import "fmt"

func takesThree(id int64, name string, extra int64) {
	fmt.Println(id, name, extra)
}

func variadic(values ...int64) int64 {
	total := int64(0)
	for _, v := range values {
		total += v
	}
	return total
}

func main() {
	pair := []int64{1, 2}

	// (1) spread then an explicit argument: refused
	fmt.Println(variadic(pair..., 5))

	// (2) spread alone as the final argument: accepted
	fmt.Println(variadic(pair...))

	// (3) spreading into a fixed-arity signature: no such form
	row := []interface{}{int64(1), "n", int64(5)}
	takesThree(row...)
}
