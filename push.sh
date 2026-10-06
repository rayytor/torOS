#!/bin/sh
# Send the last build to the laptop over Wi-Fi and restart it. No sudo needed.
#   ./push.sh           update the laptop and reboot it
#   ./push.sh -b        run ./build.sh first
#   ./push.sh -n        update without rebooting
#   ./push.sh reboot    just reboot the laptop
#   ./push.sh off       power it off
#   ./push.sh recovery  reboot it once with the recovery kernel (Void's, with an
#                       initramfs); the start after that is the torOS kernel again
#   ./push.sh ssh [CMD] root shell on the laptop, or run one command
#   ./push.sh run CMD   run CMD as the desktop user inside the running desktop
#                       (Wayland, D-Bus and audio work: GUI apps, notify-send, wpctl)
#   ./push.sh shot FILE screenshot of the laptop's screen as a PNG
#   ./push.sh copy      this PC's clipboard -> the laptop's clipboard
#   ./push.sh copy TEXT put TEXT on the laptop's clipboard (or pipe text in)
#   ./push.sh paste     the laptop's clipboard -> this PC's clipboard, and print it
# The laptop is found on the local network by its SSH host key, so its address
# does not matter. To name it yourself: TOROS_HOST=192.168.1.20 ./push.sh
#
# Everything is replaced by the build except what belongs to the laptop: /home,
# saved Wi-Fi networks, Bluetooth pairings and logs (see the list below). Files
# in the home directory that come from /etc/skel are refreshed too.
set -eu
cd "$(dirname "$0")"
HERE=$PWD

[ -e keys/push ] && [ -e keys/host_ed25519.pub ] || { echo "no keys/ yet; run ./build.sh first" >&2; exit 1; }
HOSTKEY=$(cut -d' ' -f2 keys/host_ed25519.pub)
PORT=22

# Is the torOS host key behind this address?
is_toros() {
	ssh-keyscan -T 2 -t ed25519 -p "$PORT" "$1" 2>/dev/null | grep -qF "$HOSTKEY"
}

# Print the laptop's address: the one given, the one used last time, or
# whichever machine on this PC's network answers with the torOS host key.
find_laptop() {
	if [ -n "${TOROS_HOST:-}" ]; then
		is_toros "${TOROS_HOST%:*}" && echo "${TOROS_HOST%:*}"
		return
	fi
	last=$(cat keys/last-host 2>/dev/null || true)
	if [ -n "$last" ] && is_toros "$last"; then echo "$last"; return; fi
	addrs=$(for dev in $(ip -4 route show default | sed -n 's/.* dev \([^ ]*\).*/\1/p' | sort -u); do
		ip -4 -o addr show dev "$dev" scope global | awk '{print $4}'
	done | while IFS=./ read -r a b c _; do seq -f "$a.$b.$c.%g" 1 254; done)
	[ -n "$addrs" ] || return 0
	# shellcheck disable=SC2086
	ssh-keyscan -T 2 -t ed25519 $addrs 2>/dev/null | awk -v k="$HOSTKEY" '$3 == k {print $1; exit}'
}

case "${TOROS_HOST:-}" in *:*) PORT=${TOROS_HOST##*:} ;; esac
SSH="ssh -F /dev/null -p $PORT -i $HERE/keys/push -o IdentitiesOnly=yes -o BatchMode=yes
	-o UserKnownHostsFile=$HERE/keys/known_hosts -o GlobalKnownHostsFile=/dev/null
	-o HostKeyAlias=toros -o StrictHostKeyChecking=yes -o ConnectTimeout=5 -o LogLevel=ERROR"
SSH=$(echo $SSH)	# one line: rsync splits it on spaces

connect() {
	HOST=$(find_laptop) || true
	[ -n "$HOST" ] || { echo "Laptop not found. Is it on, awake and on the same network as this PC?" >&2; exit 1; }
	[ -n "${TOROS_HOST:-}" ] || echo "$HOST" > keys/last-host
}
remote() { $SSH "root@$HOST" "$@"; }

restart() {	# $1 = reboot | poweroff
	old=$(remote cat /proc/sys/kernel/random/boot_id)
	remote "sync; (sleep 1; $1) </dev/null >/dev/null 2>&1 &"
	[ "$1" = reboot ] || { echo "Laptop is powering off."; return; }
	printf "Rebooting"
	for _ in $(seq 60); do
		sleep 3; printf .
		HOST=$(find_laptop) && [ -n "$HOST" ] || continue
		new=$(remote cat /proc/sys/kernel/random/boot_id 2>/dev/null) || continue
		[ "$new" != "$old" ] || continue
		[ -n "${TOROS_HOST:-}" ] || echo "$HOST" > keys/last-host
		echo " back up at $HOST: build $(remote cat /etc/toros/build-id), kernel $(remote uname -r)"
		return
	done
	echo " not back after 3 minutes; look at the laptop." >&2; exit 1
}

# Run shell code as the desktop user inside the laptop's desktop session (SSH
# logs in as root). The code travels base64-encoded so any quoting survives.
in_session() {
	code=$(printf '%s\n' \
		'p=$(pgrep -u "$(id -u)" -x labwc | head -n1)' \
		'[ -n "$p" ] || { echo "the desktop is not running on the laptop" >&2; exit 1; }' \
		'export XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=wayland-0' \
		'export DBUS_SESSION_BUS_ADDRESS="$(tr "\0" "\n" < /proc/$p/environ | sed -n "s/^DBUS_SESSION_BUS_ADDRESS=//p")"' \
		'[ -r /etc/locale.conf ] && . /etc/locale.conf && export LANG' \
		'cd' "$1" | base64 -w0)
	remote "su \$(id -nu 1000) -c 'eval \"\$(echo $code | base64 -d)\"'"
}

BUILD=no REBOOT=yes
case "${1:-}" in
reboot)	connect; restart reboot; exit ;;
off)	connect; restart poweroff; exit ;;
recovery) connect; remote /usr/lib/toros/bootonce recovery; restart reboot; exit ;;
ssh)	shift; connect
	if [ $# -eq 0 ]; then exec $SSH -t "root@$HOST"; else exec $SSH "root@$HOST" "$@"; fi ;;
run)	shift; [ $# -gt 0 ] || { echo "usage: $0 run CMD" >&2; exit 1; }
	connect; in_session "$*"; exit ;;
shot)	[ $# -eq 2 ] || { echo "usage: $0 shot FILE.png" >&2; exit 1; }
	connect; in_session 'grim -' > "$2"; echo "$2"; exit ;;
copy)	shift
	TMP=$(mktemp); trap 'rm -f "$TMP"' EXIT
	if [ $# -gt 0 ]; then printf '%s' "$*"; elif [ -t 0 ]; then wl-paste -n 2>/dev/null || true; else cat; fi > "$TMP"
	[ -s "$TMP" ] || { echo "nothing to send: this PC's clipboard has no text" >&2; exit 1; }
	connect
	in_session 'wl-copy >/dev/null 2>&1' < "$TMP" || { echo "could not reach the laptop's desktop session" >&2; exit 1; }
	echo "On the laptop's clipboard ($(wc -c < "$TMP") bytes). Paste there with Ctrl+V, in the terminal Ctrl+Shift+V."
	exit ;;
paste)	TMP=$(mktemp); trap 'rm -f "$TMP"' EXIT
	connect
	in_session 'wl-paste -n 2>/dev/null' > "$TMP" || true
	[ -s "$TMP" ] || { echo "the laptop's clipboard has no text" >&2; exit 1; }
	wl-copy < "$TMP" 2>/dev/null || echo "(wl-copy not available here; printing only)" >&2
	cat "$TMP"; echo
	exit ;;
-b)	BUILD=yes ;;
-n)	REBOOT=no ;;
"")	;;
*)	sed -n '2,18s/^# \{0,1\}//p' "$0"; exit 1 ;;
esac

[ "$BUILD" = no ] || ./build.sh
TREE=$(podman volume inspect toros-tree --format '{{.Mountpoint}}' 2>/dev/null) || TREE=
[ -n "$TREE" ] && podman unshare test -e "$TREE/rootfs/etc/toros/build-id" || { echo "no build to send; run ./build.sh first" >&2; exit 1; }
NEW=$(podman unshare cat "$TREE/rootfs/etc/toros/build-id")

connect
echo "Laptop at $HOST runs build $(remote cat /etc/toros/build-id 2>/dev/null || echo unknown); sending build $NEW"
remote mountpoint -q /boot || { echo "/boot is not mounted on the laptop, so the kernel cannot be updated" >&2; exit 1; }

# The files are owned by root and system users inside the build's user
# namespace; "podman unshare" lets rsync read them with the right owners.
# Excluded paths are neither sent nor deleted on the laptop.
podman unshare rsync -aHX --numeric-ids --delete-delay --info=progress2 -e "$SSH" \
	--exclude=/proc --exclude=/sys --exclude=/dev --exclude=/run --exclude=/tmp --exclude=/boot \
	--exclude='/home/*' --exclude='/mnt/*' --exclude='/media/*' --exclude=/lost+found \
	--exclude=/etc/machine-id --exclude=/etc/resolv.conf --exclude=/etc/adjtime \
	--exclude='/var/log/*' --exclude='/var/cache/*' --exclude='/var/tmp/*' \
	--exclude=/var/lib/iwd --exclude=/var/lib/bluetooth --exclude=/var/lib/dbus \
	--exclude=/var/lib/seedrng --exclude=/var/lib/alsa \
	--filter='P supervise/' \
	"$TREE/rootfs/" "root@$HOST:/"
# Files from /etc/skel in the home directories; nothing else there is touched.
# The zapret scan results are left alone in case they were redone on the laptop.
podman unshare rsync -aHX --numeric-ids -e "$SSH" --exclude='/*/.config/zapret-gtk/' \
	"$TREE/rootfs/home/" "root@$HOST:/home/"
# Kernel, initramfs and boot loader. Compared by content: the ESP is FAT, which
# keeps no owners or permissions and only coarse local-time timestamps.
podman unshare rsync -rtc --delete-delay -e "$SSH" "$TREE/esp/" "root@$HOST:/boot/"
remote sync

if [ "$REBOOT" = yes ]; then
	restart reboot
else
	echo "Sent. Changes to the kernel, services and the desktop take effect after ./push.sh reboot"
fi
