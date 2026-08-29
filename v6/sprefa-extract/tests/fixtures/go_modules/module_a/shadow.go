package main

import . "example.com/a/pkgutil2"

// this file's OWN Widget shadows the dot-imported pkgutil2.Widget: the
// same-file leg always wins over any import.
func Widget() int { return 99 }

func UseShadowed() int { return Widget() }
