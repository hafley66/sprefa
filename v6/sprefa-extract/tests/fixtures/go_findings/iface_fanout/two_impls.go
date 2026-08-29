package main

import "fmt"

// Shape has two implementers: every x.M()/x.N() site fans out to both.
type Shape interface {
	M() string
	N() string
}

type Circle struct{}

func (Circle) M() string { return "circle.M" }
func (Circle) N() string { return "circle.N" }

type Square struct{}

func (Square) M() string { return "square.M" }
func (Square) N() string { return "square.N" }

func draw(s Shape) {
	fmt.Println(s.M())
	fmt.Println(s.N())
}

func main() {
	draw(Circle{})
	draw(Square{})
}
