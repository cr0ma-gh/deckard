#!/usr/bin/env bash
# Helpers shared by boot-test.sh and install-test.sh (sourced, not executed).
#
# Expects the caller to set: $monitor (QEMU monitor socket path) before using
# hmp/type_text, and $out (output directory) before using shoot.

# Sets ovmf_code/ovmf_vars to the UEFI firmware (Ubuntu CI or Arch names).
find_ovmf() {
  ovmf_code="" ovmf_vars=""
  local pair
  for pair in /usr/share/OVMF/OVMF_CODE_4M.fd:/usr/share/OVMF/OVMF_VARS_4M.fd \
    /usr/share/edk2/x64/OVMF_CODE.4m.fd:/usr/share/edk2/x64/OVMF_VARS.4m.fd; do
    if [[ -f ${pair%%:*} ]]; then
      ovmf_code="${pair%%:*}" ovmf_vars="${pair##*:}"
      return 0
    fi
  done
  echo "error: OVMF firmware not found" >&2
  return 1
}

# Sets the accel array: KVM when available, software emulation otherwise.
find_accel() {
  accel=(-accel tcg)
  if [[ -w /dev/kvm ]]; then
    accel=(-enable-kvm -cpu host)
  else
    echo "warning: no KVM, using software emulation (slow)" >&2
  fi
}

# Sends one command to the QEMU human monitor.
hmp() { echo "$1" | socat - UNIX-CONNECT:"$monitor" >/dev/null; }

# Types text on the guest keyboard (US layout), then Enter.
type_text() {
  local text="$1" i c key
  for ((i = 0; i < ${#text}; i++)); do
    c="${text:i:1}"
    case "$c" in
      [a-z0-9]) key="$c" ;;
      [A-Z]) key="shift-${c,,}" ;;
      ' ') key="spc" ;;
      '-') key="minus" ;;
      '_') key="shift-minus" ;;
      '.') key="dot" ;;
      '/') key="slash" ;;
      '>') key="shift-dot" ;;
      '&') key="shift-7" ;;
      '|') key="shift-backslash" ;;
      '*') key="shift-8" ;;
      ';') key="semicolon" ;;
      '=') key="equal" ;;
      *)
        echo "type_text: unsupported character '$c'" >&2
        return 1
        ;;
    esac
    hmp "sendkey $key"
  done
  hmp "sendkey ret"
}

# Saves a screenshot as $out/shot-<name>.png.
shoot() {
  hmp "screendump $out/shot-$1.png -f png"
  echo "screenshot $1"
}

# Logs in as root on tty2 (no password on Deckard images).
root_console() {
  hmp "sendkey ctrl-alt-f2"
  sleep 3
  type_text root
  sleep 3
}
