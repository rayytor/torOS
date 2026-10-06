#!/bin/sh
# Build the torOS disk image inside a Void Linux container (rootless podman).
# Result: out/toros.img, a GPT disk image (ESP + ext4 root) that boots in QEMU
# (test/qemu.sh) and from a USB stick (write-usb.sh), and the same files as a
# tree in the podman volume toros-tree, which push.sh sends to the laptop.
set -eu
cd "$(dirname "$0")"

mkdir -p out
# Keys for push.sh: one to log in to the laptop with, and the laptop's SSH host
# key. The host key is the same in every image, so the PC still recognises the
# laptop after the stick is rewritten.
mkdir -p keys && chmod 700 keys
[ -e keys/push ] || ssh-keygen -q -t ed25519 -N '' -C toros-push -f keys/push
[ -e keys/host_ed25519 ] || ssh-keygen -q -t ed25519 -N '' -C toros-host -f keys/host_ed25519
echo "toros $(cut -d' ' -f1,2 keys/host_ed25519.pub)" > keys/known_hosts
podman build -q -t toros-builder -f build/Containerfile build >/dev/null
exec podman run --rm --privileged \
	-v toros-xbps-cache:/var/cache/xbps \
	-v "$PWD":/src:ro \
	-v "$PWD/out":/out \
	-v toros-tree:/tree \
	-v "${OYNAZ_SRC:-$HOME/Projects/oynaz}":/oynaz:ro \
	-e TOROS_USER="${TOROS_USER:-rayyan}" \
	${TOROS_MIRROR:+-e TOROS_MIRROR="$TOROS_MIRROR"} \
	toros-builder /src/build/in-container.sh
