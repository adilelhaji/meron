//go:build linux

package main

import "os"

// preferX11Window asks GTK for the X11 backend when the session is Wayland.
//
// Under Wayland, the window this app gets will not grow past the size it is
// created at, and maximizing expands it to one side only — the user cannot
// size their own window. Through XWayland the compositor gets a window it can
// resize normally.
//
// Must run before Wails starts GTK, which reads the variable at init.
//
// The desktop session exports GDK_BACKEND=wayland itself, so respecting an
// existing value would mean never applying this at all; the opt-out is a
// variable of this app's own, which also allows comparing the two.
func preferX11Window() {
	if choice := os.Getenv("MERON_GDK_BACKEND"); choice != "" {
		_ = os.Setenv("GDK_BACKEND", choice)
		return
	}
	// Not a Wayland session: whatever GTK picks is already right.
	if os.Getenv("WAYLAND_DISPLAY") == "" {
		return
	}
	// No X server to fall back to — XWayland is absent or disabled. Forcing
	// x11 here would leave the app unable to open a window at all, which is
	// far worse than a window that resizes badly.
	if os.Getenv("DISPLAY") == "" {
		return
	}
	_ = os.Setenv("GDK_BACKEND", "x11")
}
