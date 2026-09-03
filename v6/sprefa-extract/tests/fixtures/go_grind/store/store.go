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
