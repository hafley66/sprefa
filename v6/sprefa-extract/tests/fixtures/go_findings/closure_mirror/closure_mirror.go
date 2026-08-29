// Expected: every closure-caller edge (a call whose covering def is a Lambda)
// gets ONE mirror edge onto the innermost NAMED enclosing def; nested closures
// mirror to `outer`, never to an outer closure; the package-level literal's
// body mints no def, so its call emits no row at all.
// Observed: the three closure-caller sites (helper/wrap in step, helper in
// inner, helper at package level) — the two in-named-fn closures each carry a
// mirror to `outer`; the package-level site resolves to nothing.
package mirror

var pkgLevel = func() {
	helper()
}

func helper() {}

func wrap() {}

func outer() {
	step := func() {
		helper()
		wrap()
	}
	inner := func() {
		step()
		helper()
	}
	step()
	inner()
}
