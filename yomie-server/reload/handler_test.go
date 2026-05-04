package reload

import (
	"errors"
	"testing"
)

func TestHandlerExecute(t *testing.T) {
	h := NewHandler()
	var called []int

	h.OnReload(func() error {
		called = append(called, 1)
		return nil
	})
	h.OnReload(func() error {
		called = append(called, 2)
		return nil
	})
	h.OnReload(func() error {
		called = append(called, 3)
		return nil
	})

	err := h.Execute()
	if err != nil {
		t.Fatalf("Expected nil error, got %v", err)
	}
	if len(called) != 3 {
		t.Fatalf("Expected 3 callbacks, got %d", len(called))
	}
	for i, v := range called {
		if v != i+1 {
			t.Errorf("Expected callback %d, got %d", i+1, v)
		}
	}
}

func TestHandlerExecuteWithError(t *testing.T) {
	h := NewHandler()
	var called int

	h.OnReload(func() error {
		called++
		return nil
	})
	h.OnReload(func() error {
		called++
		return errors.New("reload failed")
	})
	h.OnReload(func() error {
		called++
		return nil
	})

	err := h.Execute()
	if err == nil {
		t.Fatal("Expected error, got nil")
	}
	if err.Error() != "reload failed" {
		t.Errorf("Expected 'reload failed', got %q", err.Error())
	}
	// All callbacks should still execute even if one fails
	if called != 3 {
		t.Errorf("Expected all 3 callbacks to run, got %d", called)
	}
}

func TestHandlerNoCallbacks(t *testing.T) {
	h := NewHandler()
	err := h.Execute()
	if err != nil {
		t.Fatalf("Expected nil error with no callbacks, got %v", err)
	}
}
