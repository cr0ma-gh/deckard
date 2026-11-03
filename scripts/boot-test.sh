#!/usr/bin/env bash
# Boots a Deckard ISO (or an installed disk image) in QEMU without a window and
# saves a screenshot every 15 seconds, so the boot can be checked where there
# is no display (CI).
#
#   scripts/boot-test.sh <deckard.iso|disk.qcow2> <output-dir> [seconds]   # default 300 s
#
# Needs: qemu-system-x86_64, OVMF (UEFI firmware) and socat.
# Output: shot-NNN.png (NNN = seconds since start) and serial.log.
# With DECKARD_BOOT_DIAG=1 it then logs in on tty2, copies logs to serial.log
# and fails unless the deckard process of user "deck" is running.
set -euo pipefail

image="${1:?usage: $0 <iso|qcow2> <output-dir> [seconds]}"
out="${2:?usage: $0 <iso|qcow2> <output-dir> [seconds]}"
duration="${3:-300}"
interval=15

# shellcheck source=lib/qemu-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/qemu-common.sh"

[[ -f $image ]] || {
  echo "error: image not found: $image" >&2
  exit 1
}
mkdir -p "$out"
find_ovmf
find_accel

case "$image" in
  *.qcow2) media=(-drive "file=$image,if=virtio,format=qcow2") ;;
  *) media=(-cdrom "$image" -boot d) ;;
esac

tmp="$(mktemp -d)"
cp "$ovmf_vars" "$tmp/vars.fd"
monitor="$tmp/monitor.sock"

qemu-system-x86_64 \
  "${accel[@]}" \
  -machine q35 -smp 4 -m 4G \
  -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
  -drive if=pflash,format=raw,file="$tmp/vars.fd" \
  "${media[@]}" \
  -device virtio-vga -display none \
  -device qemu-xhci -device usb-tablet \
  -nic user,model=virtio-net-pci \
  -serial file:"$out/serial.log" \
  -monitor unix:"$monitor",server,nowait &
qemu_pid=$!
trap 'kill "$qemu_pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

for ((t = interval; t <= duration; t += interval)); do
  sleep "$interval"
  if ! kill -0 "$qemu_pid" 2>/dev/null; then
    echo "error: QEMU exited after ${t}s" >&2
    exit 1
  fi
  shoot "$(printf '%03d' "$t")"
done

# Optional diagnostics: log in as root on tty2 and copy logs to the serial
# port, which QEMU writes to serial.log.
if [[ ${DECKARD_BOOT_DIAG:-0} == 1 ]]; then
  echo "collecting diagnostics"
  root_console
  for cmd in \
    "pgrep -u deck -x deckard >/dev/null && echo DIAG-RESULT-OK >/dev/ttyS0 || echo DIAG-RESULT-FAIL >/dev/ttyS0" \
    "echo DIAG-id >/dev/ttyS0; id deck >/dev/ttyS0 2>&1" \
    "echo DIAG-greetd >/dev/ttyS0; journalctl -b -u greetd -o short --no-pager >/dev/ttyS0 2>&1" \
    "echo DIAG-session.log >/dev/ttyS0; cat /home/deck/.local/state/deckard/session.log >/dev/ttyS0 2>&1" \
    "echo DIAG-network >/dev/ttyS0; nmcli -t general status >/dev/ttyS0 2>&1; runuser -u deck -- nmcli -t general permissions >/dev/ttyS0 2>&1" \
    "echo DIAG-errors >/dev/ttyS0; journalctl -b -p err -o short --no-pager >/dev/ttyS0 2>&1" \
    "echo DIAG-end >/dev/ttyS0"; do
    type_text "$cmd"
    # Let the command finish: keys typed while it runs can get lost.
    sleep 5
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
