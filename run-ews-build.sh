#!/bin/bash
# Launches the locally built Meron against a separate profile, so the Exchange
# work can be exercised without touching the installed Flatpak's accounts,
# cache or keychain entries.
export XDG_CONFIG_HOME="$HOME/meron-ews-test/config"
export XDG_DATA_HOME="$HOME/meron-ews-test/data"
export XDG_CACHE_HOME="$HOME/meron-ews-test/cache"
export MERON_KEYRING="${MERON_KEYRING:-off}"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"
exec "$(dirname "$(readlink -f "$0")")/build/bin/meron" "$@"
