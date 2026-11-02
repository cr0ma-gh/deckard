#!/usr/bin/env bash
# shellcheck disable=SC2034
# Appended to releng's profiledef.sh by scripts/build-iso.sh: later assignments
# win, so only names and extra permissions are changed here; boot modes and the
# rest stay as tested upstream.

iso_name="deckard"
iso_label="DECKARD_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="Deckard <https://github.com/cr0ma-gh/deckard>"
iso_application="Deckard live image for Steam Deck LCD"
# Each path listed explicitly: the user's home must belong to "deck" (uid 1000).
file_permissions+=(
  ["/home/deck"]="1000:1000:750"
  ["/home/deck/.config"]="1000:1000:755"
  ["/home/deck/.config/deckard"]="1000:1000:755"
  ["/home/deck/.config/deckard/config.toml"]="1000:1000:644"
  ["/etc/sudoers.d/deckard"]="0:0:440"
)
