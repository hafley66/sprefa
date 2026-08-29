// A package-level func named len shadows the builtin for this whole package:
// the call below must bind to THIS def, never emit an unresolved builtin row.
package shadow

func len(xs []int) int { return 0 }

func caller(xs []int) int { return len(xs) }
