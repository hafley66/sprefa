package typeplane

type Loud struct{}

func (l *Loud) Speak() string { return "LOUD" }
func (l *Loud) Volume() int   { return 100 }
