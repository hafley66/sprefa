package shadowpkg

import . "example.com/a/pkgutil2"

// Go forbids one identifier in both a file block and its package block, so
// this package-level Widget can only live outside main's package.
func Widget() int { return 99 }

func UseShadowed() int { return Widget() }
