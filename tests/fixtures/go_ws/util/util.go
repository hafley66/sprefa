package util

import "strings"

// FormatName upcases a display name. Unique bare name across the module -- the
// clean cross-package confirmed-resolution case.
func FormatName(name string) string {
	return strings.ToUpper(name)
}

// Helper shares its bare name with api.Helper -- never imported from here by
// anyone in this fixture.
func Helper() string {
	return "util-helper"
}
