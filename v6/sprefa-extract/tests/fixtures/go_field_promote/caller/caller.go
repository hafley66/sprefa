package caller

import "example.com/fieldpromote/lib"

// Decoy is a same-named method on an unrelated type, so a corpus-wide name
// guess cannot fake the promoted-field binding.
type Decoy struct{}

// Ring is the decoy method.
func (d *Decoy) Ring() string { return "decoy" }

// UseOuter calls through a field Outer promotes from its embed.
func UseOuter(o *lib.Outer) {
	o.Part.Ring()
}
