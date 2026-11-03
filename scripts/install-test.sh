#!/usr/bin/env bash
# End-to-end test of the installer in QEMU:
#   1. boot the ISO from a virtual USB stick, with an empty virtual disk and
#      enough memory for archiso's copytoram (like a real Steam Deck: the
#      stick is unmounted after boot);
#   2. log in as root on tty2 and run "deckard-install install /dev/vda --yes";
#   3. power off, then boot from the disk alone and check that Deckard runs
#      (scripts/boot-test.sh with diagnostics).
#
#   scripts/install-test.sh <deckard.iso> <output-dir>
#
# Needs: qemu-system-x86_64 + qemu-img, OVMF and socat. Uses KVM if available.
set -euo pipefail

iso="${1:?usage: $0 <iso> <output-dir>}"
out="${2:?usage: $0 <iso> <output-dir>}"
here="$(dirname "${BASH_SOURCE[0]}")"
# shellcheck source=lib/qemu-common.sh
source "$here/lib/qemu-common.sh"

[[ -f $iso ]] || {
  echo "error: ISO not found: $iso" >&2
  exit 1
}
mkdir -p "$out"
find_ovmf
find_accel

disk="$out/disk.qcow2"
rm -f "$disk"
qemu-img create -q -f qcow2 "$disk" 24G

tmp="$(mktemp -d)"
cp "$ovmf_vars" "$tmp/vars.fd"
monitor="$tmp/monitor.sock"

echo "==> Booting the ISO with an empty disk"
qemu-system-x86_64 \
  "${accel[@]}" \
  -machine q35 -smp 4 -m 6G \
  -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
  -drive if=pflash,format=raw,file="$tmp/vars.fd" \
  -drive file="$disk",if=virtio,format=qcow2 \
  -device virtio-vga -display none \
  -device qemu-xhci -device usb-tablet \
  -drive file="$iso",if=none,id=stick,format=raw,readonly=on \
  -device usb-storage,drive=stick,bootindex=0 \
  -nic user,model=virtio-net-pci \
  -serial file:"$out/install-serial.log" \
  -monitor unix:"$monitor",server,nowait &
qemu_pid=$!
trap 'kill "$qemu_pid" 2>/dev/null || true; rm -rf "$tmp"' EXIT

sleep 100
shoot install-000-live
echo "==> Running the installer"
root_console
# Short lines with pauses: long typed lines can get scrambled.
type_text "mountpoint -q /run/archiso/bootmnt && echo BOOTMNT-mounted >/dev/ttyS0 || echo BOOTMNT-unmounted >/dev/ttyS0"
sleep 3
type_text "deckard-install list >/dev/ttyS0 2>&1"
sleep 5
type_text "deckard-install install /dev/vda --yes >/dev/ttyS0 2>&1; sleep 2; poweroff"

# Wait for the installer to finish and the VM to power off (max 20 minutes).
for ((t = 60; t <= 1200; t += 60)); do
  sleep 60
  if ! kill -0 "$qemu_pid" 2>/dev/null; then
    break
  fi
  shoot "install-$(printf '%04d' "$t")"
done
if kill -0 "$qemu_pid" 2>/dev/null; then
  echo "error: the installer did not finish in 20 minutes" >&2
  exit 1
fi
trap 'rm -rf "$tmp"' EXIT

echo "==> Installer output"
tr -d '\r' <"$out/install-serial.log" | grep -aE 'BOOTMNT-|^(/dev/|PROGRESS|DONE|ERROR)' | tail -n 30
# Only the empty virtio disk may be offered: never the USB stick we booted from.
if tr -d '\r' <"$out/install-serial.log" | grep -E '^/dev/' | grep -qv '^/dev/vda'; then
  echo "error: the installer offered a disk other than /dev/vda (the boot stick?)" >&2
  exit 1
fi
grep -q '^DONE' <(tr -d '\r' <"$out/install-serial.log") || {
  echo "error: the installer did not report DONE" >&2
  exit 1
}

echo "==> Booting the installed disk"
DECKARD_BOOT_DIAG=1 bash "$here/boot-test.sh" "$disk" "$out/installed" 150
echo "install test passed"
