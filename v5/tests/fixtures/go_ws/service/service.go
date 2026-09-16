package service

import (
	"example.com/gows/api"
	"example.com/gows/util"
)

// UserService owns the cross-file method call exercised from main.go.
type UserService struct{}

// GetUser makes a cross-package plain function call (util.FormatName, a name
// unique in the module) and a cross-package ambiguous call (api.Helper, whose
// bare name also lives in util).
func (service *UserService) GetUser(id int) string {
	tag := api.Helper()
	return util.FormatName("user-" + tag)
}
