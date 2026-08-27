package main

import (
	"os"

	"meron/internal/obf"
)

const (
	// Loopback host for the OAuth redirect. Per RFC 8252 (OAuth for Native Apps)
	// we bind an ephemeral port (host:0) at flow start and build the redirect URI
	// from the OS-assigned port — both Google and Microsoft match loopback
	// redirects on host only, so the portless URI is what's registered. This
	// avoids hardcoding a port that may already be in use.
	oauthLoopbackHost = "127.0.0.1"

	// Delegated scopes for Outlook/Microsoft 365 IMAP+SMTP access. offline_access
	// yields a refresh token; openid/email/profile populate the id_token we read
	// the account email and name from.
	outlookScopes = "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access openid email profile"

	outlookTokenURL = "https://login.microsoftonline.com/common/oauth2/v2.0/token"

	// Delegated scopes for Google. Mail over IMAP/SMTP, the profile the account
	// name and address are read from, and the calendar — mail and calendar are
	// set up together, so they are consented to together. A token issued
	// without the calendar scope cannot call the Calendar API, so an account
	// added before this scope existed has to be reconnected once.
	googleScopes = "https://mail.google.com/ https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/calendar"
)

// Obfuscated Google OAuth credentials baked into release builds.
//
// These are intentionally embedded in source: the Flatpak is built on
// infrastructure we don't control, with no build-time secret injection, so the
// values have to travel with the public repo. Per Google's installed-app model
// the client secret is not confidential; the obfuscation (see internal/obf)
// only defeats plaintext scraping, not a determined extractor.
//
// Generate replacements with:  go run ./cmd/obfuscate
// then paste the printed values here.
var (
	googleClientIDObf     = "f1FFXVgbXFZAQFgZWwGpVyUNGAkfQAFVQRgJGE4G6At0ABwFDF8NFx0EBl1HH/tMPRZcCAFCCA0QARtIBFL1UjkAHBtATgAM"
	googleClientSecretObf = "CioxPD51QlcQNz0fIkvUSwoRQC4vaQM2QC4ydTtzw1Y0DQA="

	// Outlook is a public client (PKCE), so there is no client secret — only the
	// Application (client) ID. Generate the obfuscated value with the same
	// obfuscation helper used for Google and paste it here for release builds.
	outlookClientIDObf = "egQUWl8cWVZYQg1JFxyuWntTX1dbHFtMEEFZG0VUrgx7XBcN"
)

// The environment wins over the compiled credentials, so a build can be run
// against a different Google Cloud project — one with other APIs enabled, or
// a developer's own — without editing tracked source and risking a commit of
// somebody's credentials. The core already reads these same two variables
// when it refreshes a token (see `engine.rs`), so setting them covers the
// whole flow; a release Flatpak sets neither and uses the compiled values.
func googleClientID() string {
	if fromEnv := os.Getenv("MERON_GOOGLE_CLIENT_ID"); fromEnv != "" {
		return fromEnv
	}
	return obf.Decode(googleClientIDObf)
}

func googleClientSecret() string {
	if fromEnv := os.Getenv("MERON_GOOGLE_CLIENT_SECRET"); fromEnv != "" {
		return fromEnv
	}
	return obf.Decode(googleClientSecretObf)
}

func gmailOAuthConfigured() bool {
	return googleClientID() != "" && googleClientSecret() != ""
}

func outlookClientID() string {
	if outlookClientIDObf == "" {
		return ""
	}
	return obf.Decode(outlookClientIDObf)
}

func outlookOAuthConfigured() bool {
	return outlookClientID() != ""
}
