package typeplane

type Quiet struct{}

func (q *Quiet) Speak() string { return "quiet" }
func (q *Quiet) Volume() int   { return 1 }
