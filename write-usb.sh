#!/bin/sh
# Write out/toros.img to the torOS USB stick. ERASES THE WHOLE STICK.
#   ./write-usb.sh            the stick that already holds torOS (found by label)
#   ./write-usb.sh /dev/sdX   a new stick
# Asks for the sudo password and for confirmation. Only removable USB disks are
# accepted. Saved Wi-Fi networks and Bluetooth pairings on the old stick are
# carried over, so the laptop comes back online by itself and push.sh can reach
# it. torOS grows to fill the stick on first boot. After this one write,
# updates go over Wi-Fi with ./push.sh.
set -eu
cd "$(dirname "$0")"
[ "$(id -u)" = 0 ] || exec sudo "$PWD/${0##*/}" "$@"

IMG=${TOROS_IMG:-out/toros.img}
[ -e "$IMG" ] || { echo "no $IMG; run ./build.sh first" >&2; exit 1; }

DISK=${1:-}
if [ -z "$DISK" ]; then
	[ -b /dev/disk/by-label/torOS ] || { echo "no torOS stick found; plug it in, or name a new stick: $0 /dev/sdX" >&2; exit 1; }
	DISK=/dev/$(lsblk -no PKNAME "$(readlink -f /dev/disk/by-label/torOS)")
fi
[ "$(lsblk -dno TYPE,TRAN,RM "$DISK" 2>/dev/null | awk '{print $1, $2, $3}')" = "disk usb 1" ] || { echo "$DISK is not a removable USB disk; refusing" >&2; exit 1; }
[ "$(blockdev --getsize64 "$DISK")" -ge "$(blockdev --getsize64 "$IMG" 2>/dev/null || stat -c %s "$IMG")" ] || { echo "$DISK is smaller than the image" >&2; exit 1; }

lsblk -o NAME,SIZE,FSTYPE,LABEL,MODEL "$DISK"
printf 'Erase EVERYTHING on %s and write torOS to it? [y/N] ' "$DISK"
read -r answer
[ "$answer" = y ] || [ "$answer" = Y ] || { echo "Nothing written."; exit 1; }

# Wait for the kernel to show partition $2 of disk $1, print its device name
part() {
	udevadm settle 2>/dev/null || true
	for _ in 1 2 3 4 5 6 7 8 9 10; do
		p=$(lsblk -lnpo NAME,TYPE "$1" | awk '$2 == "part" {print $1}' | sed -n "${2}p")
		[ -n "$p" ] && [ -b "$p" ] && { echo "$p"; return; }
		sleep 1
	done
	return 1
}

MNT=$(mktemp -d) KEEP=$(mktemp -d)
trap 'umount "$MNT" 2>/dev/null || true; rmdir "$MNT" 2>/dev/null || true; rm -rf "$KEEP"' EXIT
for p in $(lsblk -lnpo NAME "$DISK" | tail -n +2); do
	while findmnt -n -S "$p" >/dev/null; do umount "$p"; done
done

# Wi-Fi networks and Bluetooth pairings from the torOS that is on the stick now
OLD=$(lsblk -lnpo NAME,LABEL "$DISK" | awk '$2 == "torOS" {print $1; exit}')
if [ -n "$OLD" ] && mount -o ro "$OLD" "$MNT" 2>/dev/null; then
	for d in iwd bluetooth; do
		[ -d "$MNT/var/lib/$d" ] && cp -a "$MNT/var/lib/$d" "$KEEP/$d"
	done
	umount "$MNT"
	echo "Keeping saved Wi-Fi networks: $(find "$KEEP/iwd" -maxdepth 1 -type f 2>/dev/null | wc -l), Bluetooth devices: $(find "$KEEP/bluetooth" -mindepth 2 -maxdepth 2 -type d -name '*:*' 2>/dev/null | wc -l)"
fi

dd if="$IMG" of="$DISK" bs=4M oflag=direct conv=fsync status=progress
sync
partprobe "$DISK" 2>/dev/null || blockdev --rereadpt "$DISK"

NEW=$(part "$DISK" 2) || { echo "the new torOS partition did not appear; unplug the stick, plug it in and run this again" >&2; exit 1; }
if [ -n "$(ls "$KEEP")" ]; then
	mount "$NEW" "$MNT"
	for d in iwd bluetooth; do
		[ -d "$KEEP/$d" ] && { rm -rf "$MNT/var/lib/$d"; cp -a "$KEEP/$d" "$MNT/var/lib/$d"; }
	done
	umount "$MNT"
fi
sync
echo "Done. Remove the stick, boot the laptop from it, then use ./push.sh for updates."
