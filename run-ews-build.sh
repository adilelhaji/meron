#!/bin/bash
# Launches the locally built Meron against a separate profile, so the Exchange
# work can be exercised without touching the installed Flatpak's accounts,
# cache or keychain entries.
export XDG_CONFIG_HOME="$HOME/meron-ews-test/config"
export XDG_DATA_HOME="$HOME/meron-ews-test/data"
export XDG_CACHE_HOME="$HOME/meron-ews-test/cache"
# The file keyring keeps credentials inside this profile rather than in the
# desktop's secret service, so the test build neither reads nor writes the
# real Meron's stored passwords. "off" would keep them in memory only, and an
# account would lose its password on every restart.
# Google OAuth credentials for a test build, kept outside the repository so
# they can never be committed. Optional: without it the build uses the
# credentials compiled into the source, whose Google Cloud project does not
# offer the calendar permission.
[ -f "$HOME/.meron-google-creds" ] && . "$HOME/.meron-google-creds"

export MERON_EWS_DEBUG="${MERON_EWS_DEBUG:-1}"
export MERON_KEYRING="${MERON_KEYRING:-file}"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
exec "$(dirname "$(readlink -f "$0")")/build/bin/meron" "$@"
