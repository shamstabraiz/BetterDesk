//go:build !darwin

package agent

// checkScreenRecordingPermission is a no-op on non-macOS platforms.
// On macOS this function verifies that the Screen Recording permission is
// granted before starting a capture session (see desktop_darwin.go).
func checkScreenRecordingPermission() error {
	return nil
}
