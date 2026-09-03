package scipconformance

// Speaker is the interface two package-level types satisfy, so the index has an
// implements pair to carry for each.
type Speaker interface {
	Speak() string
}

type Dog struct{}

func (d Dog) Speak() string { return "woof" }

type Cat struct{}

func (c Cat) Speak() string { return "meow" }

// Mute satisfies nothing: the negative case a recall count needs.
type Mute struct{}
