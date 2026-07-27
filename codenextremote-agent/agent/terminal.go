package agent

// TerminalSession wraps a platform-specific shell process.
type TerminalSession struct {
	ID       string
	closeFn  func() error
	writeFn  func([]byte) error
	resizeFn func(int, int) error
}

// Write sends data to the shell's stdin.
func (ts *TerminalSession) Write(data []byte) error {
	if ts.writeFn != nil {
		return ts.writeFn(data)
	}
	return nil
}

// Resize changes the terminal dimensions (cols × rows).
func (ts *TerminalSession) Resize(cols, rows int) error {
	if ts.resizeFn != nil {
		return ts.resizeFn(cols, rows)
	}
	return nil
}

// Close shuts down the shell process.
func (ts *TerminalSession) Close() error {
	if ts.closeFn != nil {
		return ts.closeFn()
	}
	return nil
}
