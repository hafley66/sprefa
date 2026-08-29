package main

import (
	. "example.com/a/pkgutil2"
	_ "example.com/a/blankpkg"
	"example.com/a/vendorlike/yaml.v3"
	"github.com/pkg/errors"
)

// yaml.Node: the qualifier "yaml" is the target directory's OWN package
// clause name, never "yaml.v3" (the import path's last segment).
type Wrapper struct {
	N yaml.Node
}

// bare, ambiguous corpus-wide (pkgutil2 AND pkgutil3 both export Widget):
// only the dot import disambiguates to pkgutil2.
func UseDot() int { return Widget() }

func UseExternal() error { return errors.New("boom") }
