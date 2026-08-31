package types

// Widget is the type a callee package's func returns, written qualified in
// that callee's own signature.
type Widget struct{}

// Ping is the method a caller reaches on a bound value.
func (w *Widget) Ping() string { return "ping" }
