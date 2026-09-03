package main

import (
	"fmt"

	"goprobe/shapes"
)

func describe(drawer shapes.Drawer) string {
	return drawer.Draw()
}

func widestSquare(sides []int) int {
	squares := make([]shapes.Square, 0, len(sides))
	for _, side := range sides {
		squares = append(squares, shapes.Square{Side: side})
	}
	return shapes.Widest(squares)
}

func main() {
	circle := shapes.Circle{Radius: 2, Label: shapes.Tag{Text: "c"}}
	fmt.Println(describe(circle))
	fmt.Println(widestSquare([]int{1, 2, 3}))
}
