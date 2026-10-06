#!/bin/bash
# Build the torOS kernel (trimmed to the laptop, boots with no initramfs) and
# install it into the image tree. Runs inside the toros-builder container.
#   kernel.sh ROOT STOCK_CONFIG
# ROOT          image tree; its /usr/lib/firmware supplies the firmware that is
#               built into the kernel, and the result is installed into it
# STOCK_CONFIG  config of Void's kernel, the starting point
# Result in ROOT: /boot/vmlinuz-toros, /boot/config-toros and
# /usr/lib/modules/<version>-toros. The finished kernel is kept in out/cache
# and reused until the version, kernel/, its patches or this script changes.
set -euo pipefail

ROOT=$1 STOCK=$2
SRC=${SRC:-/src} OUT=${OUT:-/out}
KVER=6.18.55
KSHA=f410638061a165c12f42ab871d2f3fcd525515359b5faeee80969cff84524df9
KTAR="$OUT/cache/linux-$KVER.tar.xz"
MARCH="-march=goldmont-plus -mtune=goldmont-plus"
B=/build/linux FW=/build/kernel-firmware

# The kernel must be the same release as Void's, whose config it starts from
grep -q "^# Linux/x86\(_64\)\? $KVER Kernel" "$STOCK" || { echo "kernel.sh: Void's kernel is no longer $KVER; update KVER and KSHA" >&2; exit 1; }

# Firmware built into the kernel image, taken (and unpacked) from the image tree
FWLIST=$(sed -n 's/^CONFIG_EXTRA_FIRMWARE="\(.*\)"$/\1/p' "$SRC/kernel/fragment")
rm -rf "$FW"
for f in $FWLIST; do
	mkdir -p "$FW/$(dirname "$f")"
	if [ -e "$ROOT/usr/lib/firmware/$f.zst" ]; then zstd -dq "$ROOT/usr/lib/firmware/$f.zst" -o "$FW/$f"
	elif [ -e "$ROOT/usr/lib/firmware/$f" ]; then cp "$ROOT/usr/lib/firmware/$f" "$FW/$f"
	else echo "kernel.sh: firmware $f not found in the image" >&2; exit 1
	fi
done

KEY=$( { echo "$KVER $KSHA $MARCH"; cat "$0" "$SRC/kernel/fragment" "$SRC/kernel/lsmod-laptop.txt" "$SRC/kernel/logo.ppm" \
	"$SRC"/build/patches/kernel-*.patch "$STOCK"; \
	find "$FW" -type f | sort | xargs sha256sum; } | sha256sum | cut -c1-16)
PKG="$OUT/cache/kernel-$KEY.tar"

if [ ! -e "$PKG" ]; then
	mkdir -p "$OUT/cache"
	[ -e "$KTAR" ] || curl -fsSL -o "$KTAR" "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$KVER.tar.xz"
	echo "$KSHA  $KTAR" | sha256sum -c --quiet
	rm -rf "$B" && mkdir -p "$B" && tar -xf "$KTAR" -C "$B" --strip-components=1
	cd "$B"

	# The boot logo: torOS's (kernel/logo.ppm, from build/splash.py) in place
	# of the penguin, and shown on a quiet boot as well
	[ -e drivers/video/logo/logo_linux_clut224.ppm ]
	cp "$SRC/kernel/logo.ppm" drivers/video/logo/logo_linux_clut224.ppm
	for p in "$SRC"/build/patches/kernel-*.patch; do patch -s -p1 < "$p"; done

	# Config: stock -> only the modules the laptop loads -> kernel/fragment
	cp "$STOCK" .config
	( set +o pipefail; yes '' | make -s LSMOD="$SRC/kernel/lsmod-laptop.txt" localmodconfig > /build/localmodconfig.log 2>&1 )
	sed "s|^CONFIG_EXTRA_FIRMWARE=.*|&\nCONFIG_EXTRA_FIRMWARE_DIR=\"$FW\"|" "$SRC/kernel/fragment" > /build/kernel-fragment
	scripts/kconfig/merge_config.sh -m .config /build/kernel-fragment > /build/merge_config.log
	make -s olddefconfig
	# every line of the fragment must have survived
	bad=0
	while IFS= read -r line; do
		case "$line" in
		CONFIG_*=*) grep -qxF "$line" .config || { echo "kernel config: wanted $line" >&2; bad=1; } ;;
		"# CONFIG_"*" is not set") o=${line#\# }; o=${o%% *}
			! grep -q "^$o=" .config || { echo "kernel config: wanted $o off, got $(grep "^$o=" .config)" >&2; bad=1; } ;;
		esac
	done < /build/kernel-fragment
	[ "$bad" = 0 ] || exit 1

	make -s -j"$(nproc)" KCFLAGS="$MARCH" bzImage modules 2>&1 | grep -v '^$' | tail -n 20
	REL=$(make -s kernelrelease)
	rm -rf /build/kpkg && mkdir -p /build/kpkg/boot
	make -s INSTALL_MOD_PATH=/build/kpkg/usr INSTALL_MOD_STRIP=1 DEPMOD=true modules_install
	rm -f "/build/kpkg/usr/lib/modules/$REL/build" "/build/kpkg/usr/lib/modules/$REL/source"
	depmod -b /build/kpkg/usr "$REL"
	cp arch/x86/boot/bzImage /build/kpkg/boot/vmlinuz-toros
	cp .config /build/kpkg/boot/config-toros
	sed -i "s|^CONFIG_EXTRA_FIRMWARE_DIR=.*|CONFIG_EXTRA_FIRMWARE_DIR=\"/usr/lib/firmware\"|" /build/kpkg/boot/config-toros
	echo "$REL" > /build/kpkg/boot/version-toros
	tar -cf "$PKG.tmp" -C /build/kpkg . && mv "$PKG.tmp" "$PKG"
	cd / && rm -rf "$B"
	# keep only this kernel in the cache
	find "$OUT/cache" -maxdepth 1 -name 'kernel-*.tar' ! -name "kernel-$KEY.tar" -delete
fi

rm -rf "$ROOT"/usr/lib/modules/*-toros
tar -xf "$PKG" -C "$ROOT" --no-same-owner
cp "$ROOT/boot/config-toros" "$OUT/kernel.config"
REL=$(cat "$ROOT/boot/version-toros")
echo "kernel $REL: image $(du -h "$ROOT/boot/vmlinuz-toros" | cut -f1), $(find "$ROOT/usr/lib/modules/$REL" -name '*.ko' | wc -l) modules ($(du -sh "$ROOT/usr/lib/modules/$REL" | cut -f1))"
