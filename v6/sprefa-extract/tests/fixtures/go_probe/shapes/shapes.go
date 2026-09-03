package shapes

// Drawer is the interface the call through an interface goes through.
type Drawer interface {
	Draw() string
	Area() int
}

type Square struct {
	Side int
}

func (s Square) Draw() string {
	return "square"
}

func (s Square) Area() int {
	return s.Side * s.Side
}

type Circle struct {
	Radius int
	Label  Tag
}

type Tag struct {
	Text string
}

func (c Circle) Draw() string {
	return "circle"
}

func (c Circle) Area() int {
	return 3 * c.Radius * c.Radius
}

func Widest[T Drawer](items []T) int {
	widest := 0
	for _, item := range items {
		if area := item.Area(); area > widest {
			widest = area
		}
	}
	return widest
}
