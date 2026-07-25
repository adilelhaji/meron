package main

import (
	"os"
	"path/filepath"
	"testing"
)

// envMap turns a fixed map into the getenv function detectChannelFrom takes.
func envMap(pairs map[string]string) func(string) string {
	return func(key string) string { return pairs[key] }
}

func TestDetectChannelFrom(t *testing.T) {
	cases := []struct {
		name        string
		goos        string
		exe         string
		env         map[string]string
		wantKind    string
		wantManaged bool
		wantTarget  string
	}{
		{
			name:        "snap wins over the linux path heuristics",
			goos:        "linux",
			exe:         "/snap/meron/12/bin/meron",
			env:         map[string]string{"SNAP": "/snap/meron/12"},
			wantKind:    channelSnap,
			wantManaged: true,
		},
		{
			name:        "flatpak",
			goos:        "linux",
			exe:         "/app/bin/meron",
			env:         map[string]string{"FLATPAK_ID": "jp.nonbili.meron"},
			wantKind:    channelFlatpak,
			wantManaged: true,
		},
		{
			name:       "appimage targets the image, not the mount",
			goos:       "linux",
			exe:        "/tmp/.mount_meronXY/usr/bin/meron",
			env:        map[string]string{"APPIMAGE": "/home/u/Apps/Meron.AppImage"},
			wantKind:   channelAppImage,
			wantTarget: "/home/u/Apps/Meron.AppImage",
		},
		{
			name:       "plain linux binary",
			goos:       "linux",
			exe:        "/opt/meron/meron",
			wantKind:   channelTarball,
			wantTarget: "/opt/meron/meron",
		},
		{
			name:        "appx runs from the store directory",
			goos:        "windows",
			exe:         `C:\Program Files\WindowsApps\Meron_0.1.12_x64__abc\meron.exe`,
			wantKind:    channelAppx,
			wantManaged: true,
		},
		{
			name:       "portable zip has no uninstaller beside it",
			goos:       "windows",
			exe:        `C:\Users\u\Downloads\meron\meron.exe`,
			wantKind:   channelPortable,
			wantTarget: `C:\Users\u\Downloads\meron\meron.exe`,
		},
		{
			name:       "macOS app bundle",
			goos:       "darwin",
			exe:        "/Applications/Meron.app/Contents/MacOS/meron",
			wantKind:   channelDMG,
			wantTarget: "/Applications/Meron.app",
		},
		{
			name:     "macOS binary outside a bundle",
			goos:     "darwin",
			exe:      "/Users/u/go/bin/meron",
			wantKind: channelUnknown,
		},
		{
			name:     "unsupported OS",
			goos:     "freebsd",
			exe:      "/usr/local/bin/meron",
			wantKind: channelUnknown,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := detectChannelFrom(tc.goos, tc.exe, envMap(tc.env))
			if got.Kind != tc.wantKind {
				t.Errorf("Kind = %q, want %q", got.Kind, tc.wantKind)
			}
			if got.Managed != tc.wantManaged {
				t.Errorf("Managed = %v, want %v", got.Managed, tc.wantManaged)
			}
			if got.Target != tc.wantTarget {
				t.Errorf("Target = %q, want %q", got.Target, tc.wantTarget)
			}
		})
	}
}

// The NSIS install is told apart from the portable zip by the uninstaller the
// Wails NSIS template drops beside the exe, so this case needs a real file.
func TestDetectChannelFromNSIS(t *testing.T) {
	dir := t.TempDir()
	exe := filepath.Join(dir, "meron.exe")
	if err := os.WriteFile(exe, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "uninstall.exe"), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	got := detectChannelFrom("windows", exe, envMap(nil))
	if got.Kind != channelNSIS {
		t.Fatalf("Kind = %q, want %q", got.Kind, channelNSIS)
	}
	if got.Target != exe {
		t.Fatalf("Target = %q, want %q", got.Target, exe)
	}
}

func TestSelfUpdatable(t *testing.T) {
	cases := []struct {
		channel updateChannel
		want    bool
	}{
		{updateChannel{Kind: channelAppImage, Target: "/x.AppImage"}, true},
		{updateChannel{Kind: channelSnap, Managed: true}, false},
		{updateChannel{Kind: channelUnknown}, false},
		// A known kind with no target (path resolution failed) is not usable.
		{updateChannel{Kind: channelTarball}, false},
	}
	for _, tc := range cases {
		if got := tc.channel.SelfUpdatable(); got != tc.want {
			t.Errorf("%+v.SelfUpdatable() = %v, want %v", tc.channel, got, tc.want)
		}
	}
}
