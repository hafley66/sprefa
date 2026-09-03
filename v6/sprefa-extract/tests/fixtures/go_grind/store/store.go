package store

type Widget struct{}

func (w *Widget) Tag() string {
	return "widget"
}

type Shop struct {
	shelf map[string]*Widget
}

func (s *Shop) Widget() *Widget {
	return &Widget{}
}

func (s *Shop) Lookup(name string) (*Widget, bool) {
	w, ok := s.shelf[name]
	return w, ok
}

type Receipt struct{}

func (r *Receipt) Total() int {
	return 0
}

func (s *Shop) Sell(name string) (*Widget, *Receipt) {
	return &Widget{}, &Receipt{}
}

func (s *Shop) Receipt() *Receipt {
	return &Receipt{}
}
