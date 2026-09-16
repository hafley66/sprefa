package api

// FetchUser renders a user id as a display string.
func FetchUser(id int) string {
	return "user-" + Helper() + itoa(id)
}

// Helper shares its bare name with util.Helper -- the ambiguous-name case.
// Only service.go and main.go import THIS package.
func Helper() string {
	return "api-helper"
}

func itoa(id int) string {
	if id == 0 {
		return "0"
	}
	return "n"
}
