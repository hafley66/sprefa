// A file that declares its OWN Helper and also calls the imported one. Go
// binds `alpha.Helper` in package alpha; the local Helper is not a candidate.
package caller

import "example.com/shadow/alpha"

func Helper() int { return 0 }

func Run() int { return alpha.Helper() }
