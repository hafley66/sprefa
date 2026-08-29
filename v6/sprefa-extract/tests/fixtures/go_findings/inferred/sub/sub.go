package sub

type Sub struct{}

func (s *Sub) Hello() string { return "hello" }

func NewSub() *Sub { return &Sub{} }
