//go:build windows

package agent

import (
	"fmt"
	"os/exec"
)

// captureScreenshotPlatform captures a screenshot on Windows using
// PowerShell and .NET System.Drawing. Returns JPEG bytes.
func captureScreenshotPlatform() ([]byte, error) {
	// PowerShell script to capture the primary screen and write JPEG to stdout.
	script := `
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
$gfx = [System.Drawing.Graphics]::FromImage($bmp)
$gfx.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
$gfx.Dispose()
$ms = New-Object System.IO.MemoryStream
$bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Jpeg)
$bmp.Dispose()
$bytes = $ms.ToArray()
$ms.Dispose()
[System.Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)
`
	cmd := exec.Command("powershell", "-NoProfile", "-NonInteractive", "-Command", script)
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("screenshot capture failed: %w", err)
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("screenshot returned empty data")
	}
	return out, nil
}
