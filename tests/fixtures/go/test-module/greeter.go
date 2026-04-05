// Package testmodule provides simple greeting functionality for integration testing.
package testmodule

import "fmt"

// Version is the module version
const Version = "v1.0.0"

// Greet returns a greeting message for the given name.
func Greet(name string) string {
	return fmt.Sprintf("Hello, %s!", name)
}

// GetVersion returns the module version.
func GetVersion() string {
	return Version
}
