#!/usr/bin/env bash
# Boots a Deckard ISO in QEMU without a window and saves a screenshot every
# 15 seconds, so the boot can be checked where there is no display (CI).
#
#   scripts/boot-test.sh <deckard.iso> <output-dir> [seconds]   # default 300 s
#
# Needs: qemu-system-x86_64, OVMF (UEFI firmware) and socat.
# Output: shot-NNN.png (NNN = seconds since start) and serial.log.
# With DECKARD_BOOT_DIAG=1 it then logs in on tty2, copies logs to serial.log
# and fails unless the deckard process of user "deck" is running.
set -euo pipefail

iso="${1:?usage: $0 <iso> <output-dir> [seconds]}"
out="${2:?usage: $0 <iso> <output-dir> [seconds]}"
duration="${3:-300}"
interval=15

[[ -f $iso ]] || {
  echo "error: ISO not found: $iso" >&2
  exit 1
}
mkdir -p "$out"

# UEFI firmware: Ubuntu (CI runners) and Arch file names.
ovmf_code="" ovmf_vars=""
for pair in /usr/share/OVMF/OVMF_CODE_4M.fd:/usr/share/OVMF/OVMF_VARS_4M.fd \
  /usr/share/edk2/x64/OVMF_CODE.4m.fd:/usr/share/edk2/x64/OVMF_VARS.4m.fd; do
  if [[ -f ${pair%%:*} ]]; then
    ovmf_code="${pair%%:*}" ovmf_vars="${pair##*:}"
    break
  fi
done
[[ -n $ovmf_code ]] || {
  echo "error: OVMF firmware not found" >&2
  exit 1
}

tmp="$(mktemp -d)"
cp "$ovmf_vars" "$tmp/vars.fd"
monitor="$tmp/monitor.sock"

accel=(-accel tcg)
if [[ -w /dev/kvm ]]; then
  accel=(-enable-kvm -cpu host)
else
  echo "warning: no KVM, booting with software emulation (slow)" >&2
fi

qemu-system-x86_64 \
  "${accel[@]}" \
  -machine q35 -smp 4 -m 4G \
  -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
  -drive if=pflash,format=raw,file="$tmp/vars.fd" \
  -cdrom "$iso" -boot d \
  -device virtio-vga -display none \
  -device qemu-xhci -device usb-tablet \
  -nic user,model=virtio-net-pci \
  -serial file:"$out/serial.log" \
  -monitor unix:"$monitor",server,nowait &
qemu_pid=$!
trap 'kill "$qemu_pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

hmp() { echo "$1" | socat - UNIX-CONNECT:"$monitor" >/dev/null; }

# Types text on the guest keyboard (US layout) through the QEMU monitor.
type_text() {
  local text="$1" i c key
  for ((i = 0; i < ${#text}; i++)); do
    c="${text:i:1}"
    case "$c" in
      [a-z0-9]) key="$c" ;;
      [A-Z]) key="shift-${c,,}" ;;
      ' ') key=spc ;;
      '-') key=minus ;;
      '_') key=shift-minus ;;
      '.') key=dot ;;
      '/') key=slash ;;
      '>') key=shift-dot ;;
      '&') key=shift-7 ;;
      '|') key=shift-backslash ;;
      '*') key=shift-8 ;;
      ';') key=semicolon ;;
      '=') key=equal ;;
      *)
        echo "type_text: unsupported character '$c'" >&2
        return 1
        ;;
    esac
    hmp "sendkey $key"
  done
  hmp "sendkey ret"
}

shoot() {
  hmp "screendump $out/shot-$1.png -f png"
  echo "screenshot $1"
}

for ((t = interval; t <= duration; t += interval)); do
  sleep "$interval"
  if ! kill -0 "$qemu_pid" 2>/dev/null; then
    echo "error: QEMU exited after ${t}s" >&2
    exit 1
  fi
  shoot "$(printf '%03d' "$t")"
done

# Optional diagnostics: log in as root on tty2 (no password on the live image)
# and copy logs to the serial port, which QEMU writes to serial.log.
if [[ ${DECKARD_BOOT_DIAG:-0} == 1 ]]; then
  echo "collecting diagnostics"
  hmp "sendkey ctrl-alt-f2"
  sleep 3
  type_text root
  sleep 3
  for cmd in \
    "pgrep -u deck -x deckard >/dev/null && echo DIAG-RESULT-OK >/dev/ttyS0 || echo DIAG-RESULT-FAIL >/dev/ttyS0" \
    "echo DIAG-id >/dev/ttyS0; id deck >/dev/ttyS0 2>&1" \
    "echo DIAG-greetd >/dev/ttyS0; journalctl -b -u greetd -o short --no-pager >/dev/ttyS0 2>&1" \
    "echo DIAG-session.log >/dev/ttyS0; cat /home/deck/.local/state/deckard/session.log >/dev/ttyS0 2>&1" \
    "echo DIAG-errors >/dev/ttyS0; journalctl -b -p err -o short --no-pager >/dev/ttyS0 2>&1" \
    "echo DIAG-end >/dev/ttyS0"; do
    type_text "$cmd"
    sleep 2
  done
  sleep 3
  shoot diag
  if grep -q DIAG-RESULT-OK "$out/serial.log"; then
    echo "boot test passed: deckard is running"
  else
    echo "error: deckard is not running after boot (see $out/serial.log)" >&2
    exit 1
  fi
fi
echo "done: $(find "$out" -name 'shot-*.png' | wc -l) screenshots in $out"
