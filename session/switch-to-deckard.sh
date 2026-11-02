#!/usr/bin/env bash
# Switch from Steam to Deckard.
#
# Writes "deckard" to ~/.local/state/deckard/next-session and asks Steam
# to shut down so deckard-session launches Deckard.
set -euo pipefail

state_dir="${XDG_STATE_HOME:-$HOME/.local/state}/deckard"
mkdir -p "$state_dir"

echo "deckard" > "$state_dir/next-session"

if command -v steam >/dev/null 2>&1; then
  steam -shutdown || true
fi
