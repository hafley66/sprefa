// Speaker has two full implementers (loud.go, quiet.go) and one partial
// (mute.go: no Volume). The partial implementer must win no edge at all.
package typeplane

type Speaker interface {
	Speak() string
	Volume() int
}

func announce(s Speaker) string {
	return s.Speak()
}
