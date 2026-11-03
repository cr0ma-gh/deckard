#!/usr/bin/env bash
# Boots a Deckard ISO in QEMU without a window and saves a screenshot every
# 15 seconds, so the boot can be checked where there is no display (CI).
#
#   scripts/boot-test.sh <deckard.iso> <output-dir> [seconds]   # default 300 s
#
# Needs: qemu-system-x86_64, OVMF (UEFI firmware) and socat.
# Output: shot-NNN.png (NNN = seconds since start) and serial.log.
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

for ((t = interval; t <= duration; t += interval)); do
  sleep "$interval"
  if ! kill -0 "$qemu_pid" 2>/dev/null; then
    echo "error: QEMU exited after ${t}s" >&2
    exit 1
  fi
  shot="$out/shot-$(printf '%03d' "$t").png"
  echo "screendump $shot -f png" | socat - UNIX-CONNECT:"$monitor" >/dev/null
  echo "screenshot at ${t}s"
done
echo "done: $(find "$out" -name 'shot-*.png' | wc -l) screenshots in $out"
