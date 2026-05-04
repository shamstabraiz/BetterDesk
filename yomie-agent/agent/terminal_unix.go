//go:build !windows

package agent

import (
	"io"
	"log"
	"os"
	"os/exec"

	"github.com/creack/pty"
)

// StartTerminal spawns a PTY shell on Unix systems.
func StartTerminal(id string, cols, rows int, shell string, onOutput func([]byte)) (*TerminalSession, error) {
	if shell == "" {
		shell = os.Getenv("SHELL")
		if shell == "" {
			shell = "/bin/sh"
		}
	}

	cmd := exec.Command(shell)
	cmd.Env = append(os.Environ(), "TERM=xterm-256color")

	ptmx, err := pty.StartWithSize(cmd, &pty.Winsize{
		Cols: uint16(cols),
		Rows: uint16(rows),
	})
	if err != nil {
		return nil, err
	}

	// Read goroutine — stream PTY output to callback
	go func() {
		buf := make([]byte, 8192)
		for {
			n, readErr := ptmx.Read(buf)
			if n > 0 {
				chunk := make([]byte, n)
				copy(chunk, buf[:n])
				onOutput(chunk)
			}
			if readErr != nil {
				if readErr != io.EOF {
					log.Printf("[terminal:%s] Read error: %v", id, readErr)
				}
				break
			}
		}
	}()

	ts := &TerminalSession{
		ID: id,
		writeFn: func(data []byte) error {
			_, err := ptmx.Write(data)
			return err
		},
		resizeFn: func(c, r int) error {
			return pty.Setsize(ptmx, &pty.Winsize{Cols: uint16(c), Rows: uint16(r)})
		},
		closeFn: func() error {
			ptmx.Close()
			return cmd.Process.Kill()
		},
	}
	return ts, nil
}
