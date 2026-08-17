package sample

type Cell struct {
	Value int
}

func Allocating(rows []int, cols []int) []Cell {
	out := []Cell{}
	for _, row := range rows {
		for _, col := range cols {
			cell := Cell{Value: Add(row, col)}
			out = append(out, cell)
		}
	}
	return out
}

func Plain(limit int) int {
	total := 0
	for total < limit {
		total = Add(total, 1)
	}
	return Add(total, 0)
}

func Add(left int, right int) int {
	return left + right
}
