//go:build !linux

package main

// preferX11Window is a Linux concern: the other platforms have no GDK backend
// to choose between.
func preferX11Window() {}
