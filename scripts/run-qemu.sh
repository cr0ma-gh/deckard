#!/usr/bin/env bash
# Boots a Deckard ISO in QEMU with UEFI, to check boot, autologin and the menu
# before using the image on the Steam Deck.
#
#   scripts/run-qemu.sh [path/to/deckard.iso]   # default: newest ./out/*.iso
#
# Needs: pacman -S --needed qemu-desktop edk2-ovmf
# In a VM the session uses cage and software Vulkan (see session/deckard-session):
# expect a slow but working menu. gamescope is only tested on real hardware.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
iso="${1:-$(ls -t "$repo_root"/out/*.iso 2>/dev/null | head -n1 || true)}"
[[ -n $iso && -f $iso ]] || {
  echo "error: no ISO found; build one with scripts/build-iso.sh" >&2
  exit 1
}

ovmf_code=/usr/share/edk2/x64/OVMF_CODE.4m.fd
ovmf_vars_template=/usr/share/edk2/x64/OVMF_VARS.4m.fd
[[ -f $ovmf_code ]] || {
  echo "error: OVMF not found ($ovmf_code): pacman -S edk2-ovmf" >&2
  exit 1
}
vars="$(mktemp --suffix=.fd)"
trap 'rm -f "$vars"' EXIT
cp "$ovmf_vars_template" "$vars"

accel=()
if [[ -w /dev/kvm ]]; then
  accel=(-enable-kvm -cpu host)
else
  echo "warning: /dev/kvm not available, the VM will be very slow" >&2
fi

exec qemu-system-x86_64 \
  "${accel[@]}" \
  -machine q35 -smp 4 -m 4G \
  -drive if=pflash,format=raw,readonly=on,file="$ovmf_code" \
  -drive if=pflash,format=raw,file="$vars" \
  -cdrom "$iso" -boot d \
  -device virtio-vga -display gtk \
  -device qemu-xhci -device usb-tablet \
  -audiodev pa,id=snd -device intel-hda -device hda-output,audiodev=snd \
  -nic user,model=virtio-net-pci
