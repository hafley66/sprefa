// Mute declares Speak but never Volume: it must not implement Speaker.
package typeplane

type Mute struct{}

func (m *Mute) Speak() string { return "" }
