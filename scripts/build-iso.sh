#!/usr/bin/env bash
# Builds the Deckard live ISO on Arch Linux (WSL2, a container or CI), as root:
#
#   sudo scripts/build-iso.sh [output-dir]      # default: ./out
#
# Needs: pacman -S --needed archiso base-devel rust pkgconf git
#
# Steps:
#   1. build the deckard package from this checkout (as an unprivileged user,
#      makepkg refuses root) into a local pacman repository;
#   2. start from archiso's official "releng" profile (tested BIOS/UEFI boot)
#      and apply os/archiso/: extra packages, names, overlay files, services;
#   3. run mkarchiso.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$(realpath -m "${1:-$repo_root/out}")"
work="${DECKARD_WORK_DIR:-/var/tmp/deckard-build}"
releng=/usr/share/archiso/configs/releng
builder=deckard-builder

die() {
  echo "error: $*" >&2
  exit 1
}
step() { echo "==> $*"; }

[[ $EUID -eq 0 ]] || die "run as root (mkarchiso needs it): sudo $0"
[[ -f /etc/arch-release ]] || die "this script needs Arch Linux"
for cmd in mkarchiso makepkg repo-add cargo pkgconf; do
  command -v "$cmd" >/dev/null || die "missing '$cmd': pacman -S --needed archiso base-devel rust pkgconf"
done
[[ -d $releng ]] || die "archiso's releng profile not found in $releng"

rm -rf "$work"
mkdir -p "$work"/{src,pkg,build,repo} "$out_dir"

# ---------------------------------------------------------------- 1. package
step "Copying sources"
# A copy the build user can read (home directories are often mode 700).
tar -C "$repo_root" --exclude=frontend/target -cf - frontend session LICENSE |
  tar -C "$work/src" -xf -
cp "$repo_root/packages/deckard/PKGBUILD" "$work/pkg/"

id "$builder" &>/dev/null ||
  useradd --system --create-home --home-dir "/var/lib/$builder" "$builder"
chown -R "$builder:" "$work"

step "Installing the package's dependencies"
# makepkg runs unprivileged and only checks that depends/makedepends are
# installed, so install them here (as root) from the PKGBUILD itself.
mapfile -t deps < <(
  # shellcheck disable=SC2016 # the inner bash expands these, not this one
  bash -c 'source "$1"; printf "%s\n" "${depends[@]}" "${makedepends[@]}"' _ \
    "$repo_root/packages/deckard/PKGBUILD"
)
pacman -S --needed --noconfirm --asdeps "${deps[@]}"

step "Building the deckard package"
(
  cd "$work/pkg"
  # HOME must be the builder's: cargo keeps its registry there.
  runuser -u "$builder" -- env \
    HOME="/var/lib/$builder" \
    DECKARD_SRC="$work/src" BUILDDIR="$work/build" PKGDEST="$work/repo" \
    makepkg --noconfirm --nocheck
)
repo-add --quiet "$work/repo/deckard.db.tar.gz" "$work"/repo/*.pkg.tar.zst

# ---------------------------------------------------------------- 2. profile
step "Preparing the archiso profile"
profile="$work/profile"
cp -r "$releng" "$profile"
custom="$repo_root/os/archiso"

# releng logs root in on tty1; greetd owns tty1 here.
rm -f "$profile/airootfs/etc/systemd/system/getty@tty1.service.d/autologin.conf"

# Packages: releng's list plus ours, without comments or duplicates.
grep -hv '^\s*\(#\|$\)' "$profile/packages.x86_64" "$custom/packages.x86_64" |
  awk '!seen[$0]++' >"$profile/packages.new"
mv "$profile/packages.new" "$profile/packages.x86_64"

# Names and permissions (later assignments in the sourced file win).
cat "$custom/profiledef.overrides.sh" >>"$profile/profiledef.sh"

# Overlay files (greetd, user, sudo, hostname, default config).
cp -a "$custom/airootfs/." "$profile/airootfs/"

# Enable multilib for Steam (releng has it commented out by default).
sed -i '/^#\[multilib\]/,/^#Include/ s/^#//' "$profile/pacman.conf"

# Local repository with the deckard package.
cat >>"$profile/pacman.conf" <<EOF

[deckard]
SigLevel = Optional TrustAll
Server = file://$work/repo
EOF

# Services, enabled the way "systemctl enable" would (no presets in archiso).
link() { mkdir -p "$(dirname "$profile/airootfs/$2")" && ln -sf "$1" "$profile/airootfs/$2"; }
link /usr/lib/systemd/system/greetd.service etc/systemd/system/display-manager.service
link /usr/lib/systemd/system/bluetooth.service etc/systemd/system/bluetooth.target.wants/bluetooth.service
link /usr/lib/systemd/system/bluetooth.service etc/systemd/system/dbus-org.bluez.service
link /usr/lib/systemd/user/pipewire.socket etc/systemd/user/sockets.target.wants/pipewire.socket
link /usr/lib/systemd/user/pipewire-pulse.socket etc/systemd/user/sockets.target.wants/pipewire-pulse.socket
link /usr/lib/systemd/user/wireplumber.service etc/systemd/user/pipewire.service.wants/wireplumber.service

# ---------------------------------------------------------------- 3. image
step "Running mkarchiso"
mkarchiso -v -w "$work/iso" -o "$out_dir" "$profile"

step "Done"
ls -lh "$out_dir"/*.iso
