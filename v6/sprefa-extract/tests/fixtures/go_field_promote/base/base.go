package base

// Widget owns the method a caller reaches through a promoted field.
type Widget struct{}

// Ring is the method.
func (w *Widget) Ring() string { return "ring" }
