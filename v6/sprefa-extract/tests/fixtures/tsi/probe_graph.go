// The go twin of probe_graph.rs: every form the syntax tier states a row for.
package probe

import "fmt"

type Node[T any, K comparable] struct {
	Value T
	Next  *Node[T, K]
	Tags  []string
	Index map[K]int64
	Base
}

type Base struct {
	ID int64
}

type Shape interface {
	Area(scale float64) float64
	fmt.Stringer
}

type Number interface {
	~int | string
}

func (n *Node[T, K]) Render(width int, pretty bool) (string, error) {
	return "", nil
}

func (n *Node[T, K]) Len() int {
	return 0
}

func Sum[T Number](values []T) T {
	var total T
	return total
}

type Label = string

type Meters int64

const Limit int64 = 10

const Loose = 3

var Flag bool

var Head *Node[int64, string]

func Encode(payload []byte, wide bool) byte {
	return payload[0]
}
