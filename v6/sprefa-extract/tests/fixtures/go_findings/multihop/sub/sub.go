package sub

type Sub struct{}

func NewSub() *Sub { return &Sub{} }

func (Sub) Hello() string { return "" }
