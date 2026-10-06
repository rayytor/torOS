#!/bin/sh
# Boot out/toros.img in QEMU with UEFI firmware, 2 CPUs and 4 GB RAM (like the laptop).
#   test/qemu.sh            interactive window; changes to the image are discarded
#   test/qemu.sh selftest   headless: runs /usr/lib/toros/selftest in the guest,
#                           prints its report (out/selftest.log) and powers off;
#                           the first seconds of the screen are filmed as well
#                           (out/boot-film.txt, out/vm-boot-*.png; see test/vm.py)
set -eu
cd "$(dirname "$0")/.."

IMG=out/toros.img
VARS=out/OVMF_VARS.fd
[ -e "$IMG" ] || { echo "no $IMG; run ./build.sh first" >&2; exit 1; }
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$VARS"

set -- "${1:-gui}"
COMMON="-enable-kvm -cpu host -smp 2 -m 4096 -machine q35
	-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd
	-drive if=pflash,format=raw,file=$VARS
	-drive file=$IMG,format=raw,if=virtio,snapshot=on
	-device virtio-vga -audiodev none,id=a0 -device intel-hda -device hda-duplex,audiodev=a0
	-netdev user,id=n0 -device virtio-net-pci,netdev=n0"

case "$1" in
selftest)
	# shellcheck disable=SC2086
	rm -f out/selftest.log out/screen*.ppm out/screen*.png out/mon.sock out/vm-qmp.sock out/vm-boot-*.png
	timeout 400 qemu-system-x86_64 $COMMON -display none \
		-serial file:out/selftest.log -fw_cfg name=opt/toros/selftest,string=1 \
		-monitor unix:out/mon.sock,server,nowait -qmp unix:out/vm-qmp.sock,server,nowait &
	# What the screen shows from the firmware to the desktop (boot logo, no text)
	test/vm.py film boot 12 > out/boot-film.txt 2>&1 &
	# Screenshots of the guest display when the self-test asks for them
	for shot in idle window wifi dark; do
		for _ in $(seq 120); do
			grep -qs "TOROS-SCREENSHOT $shot" out/selftest.log && break; sleep 1
		done
		python3 - "$shot" <<'PY'
import socket, sys, time
s = socket.socket(socket.AF_UNIX); s.connect("out/mon.sock"); time.sleep(0.3); s.recv(4096)
s.send(f"screendump out/screen-{sys.argv[1]}.ppm\n".encode()); time.sleep(1.5)
PY
	done
	wait || true
	for shot in idle window wifi dark; do
		python3 -c "from PIL import Image; Image.open('out/screen-$shot.ppm').save('out/screen-$shot.png')" 2>/dev/null || true
	done
	sed -n '/TOROS-SELFTEST-BEGIN/,/TOROS-SELFTEST-END/p' out/selftest.log
	printf '\n=== boot film ===\n'; cat out/boot-film.txt
	grep -q TOROS-SELFTEST-END out/selftest.log
	;;
gui)
	# shellcheck disable=SC2086
	exec qemu-system-x86_64 $COMMON -serial null
	;;
esac
