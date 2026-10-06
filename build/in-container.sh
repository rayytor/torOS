#!/bin/bash
# Runs inside the toros-builder container; see ../build.sh.
set -euo pipefail

REPO="${TOROS_MIRROR:-https://ftp.lysator.liu.se/pub/voidlinux/current}"
SRC=/src
OUT=/out
ROOT=/build/rootfs
ESP=/build/esp
USER_NAME="${TOROS_USER:-rayyan}"
ESP_MB=256
ROOT_HEADROOM_MB=1024

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

cleanup() {
	for m in dev sys proc; do
		mountpoint -q "$ROOT/$m" && umount -R "$ROOT/$m" || true
	done
}
trap cleanup EXIT

in_root() { chroot "$ROOT" /usr/bin/env -i PATH=/usr/bin:/usr/sbin HOME=/root "$@"; }

rm -rf /build && mkdir -p "$ROOT" "$ESP"
chmod 755 "$ROOT"	# becomes / on the laptop when push.sh sends the tree

step "Preparing root"
mkdir -p "$ROOT"/{proc,sys,dev,etc,var/db/xbps/keys}
cp /var/db/xbps/keys/* "$ROOT/var/db/xbps/keys/"
for m in proc sys dev; do mount --rbind "/$m" "$ROOT/$m"; done
# Files that must be in place before the kernel package is configured
for d in etc/dracut.conf.d etc/kernel.d etc/toros; do
	mkdir -p "$ROOT/$d" && cp -a "$SRC/rootfs/$d/." "$ROOT/$d/"
done

step "Installing packages"
PKGS=$(sed -e 's/#.*//' -e '/^\s*$/d' "$SRC/packages.txt" | tr '\n' ' ')
XBPS_ARCH=x86_64 xbps-install -S -y -r "$ROOT" -c /var/cache/xbps \
	-R "$REPO" -R "$REPO/nonfree" $PKGS

step "Building the kernel"
# The torOS kernel: trimmed to the laptop, boots with no initramfs. Void's
# kernel stays installed as the boot menu's recovery entry.
STOCK_KVER=$(ls "$ROOT/boot" | sed -n 's/^vmlinuz-//p' | sort -V | tail -1)
SRC="$SRC" OUT="$OUT" "$SRC/build/kernel.sh" "$ROOT" "$ROOT/boot/config-$STOCK_KVER"

step "Applying overlay"
rsync -a --no-owner --no-group "$SRC/rootfs/" "$ROOT/"
# What belongs to the one laptop and is not in the repository (local/)
[ ! -f "$SRC/local/hearing-aid" ] || install -m644 "$SRC/local/hearing-aid" "$ROOT/etc/toros/hearing-aid"
chown -R 0:0 "$ROOT/etc/sudoers.d" && chmod 440 "$ROOT/etc/sudoers.d/wheel"
echo "repository=$REPO" > "$ROOT/etc/xbps.d/00-repository-main.conf"
echo "repository=$REPO/nonfree" > "$ROOT/etc/xbps.d/10-repository-nonfree.conf"
ln -sf /usr/share/zoneinfo/Europe/Istanbul "$ROOT/etc/localtime"
sed -i -e 's/^#\(en_US.UTF-8 \)/\1/' -e 's/^#\(tr_TR.UTF-8 \)/\1/' "$ROOT/etc/default/libc-locales"
echo "LANG=en_US.UTF-8" > "$ROOT/etc/locale.conf"
in_root xbps-reconfigure -f glibc-locales

step "Boot splash"
# The kernel shows the torOS logo (kernel/logo.ppm). /usr/lib/toros/splash
# (build/splash.c) keeps it on the screen until the desktop is up, draws the
# progress bar under it and hides the console's text. The torOS kernel runs it
# as its first program (CONFIG_DEFAULT_INIT), and it then starts runit.
LOGO_H=$(sed -n '3s/^[0-9]* \([0-9]*\)$/\1/p' "$SRC/kernel/logo.ppm")
[ -n "$LOGO_H" ] || { echo "kernel/logo.ppm: no size found (run build/splash.py)" >&2; exit 1; }
( . "$SRC/rootfs/usr/share/toros/palette"
  gcc -O2 -march=goldmont-plus -Wall -Wextra -Werror -s -DLOGO_H="$LOGO_H" -DFILL="0x$accent" \
	-DTRACK="0x$dark_headerbar" -o "$ROOT/usr/lib/toros/splash" "$SRC/build/splash.c" )
grep -qx 'CONFIG_DEFAULT_INIT="/usr/lib/toros/splash"' "$ROOT/boot/config-toros"
[ -x "$ROOT/sbin/init" ]	# what the splash starts next

step "Building zapret"
mkdir -p "$ROOT/opt"
rsync -a --no-owner --no-group "$SRC/imports/zapret/" "$ROOT/opt/zapret/"
make -s -C "$ROOT/opt/zapret" CFLAGS="-O2 -march=goldmont-plus" >/dev/null
rm -rf "$ROOT/etc/sv/zapret"
cp -a "$ROOT/opt/zapret/init.d/runit/zapret" "$ROOT/etc/sv/zapret"
chmod +x "$ROOT/etc/sv/zapret/run" "$ROOT/etc/sv/zapret/finish"
ln -sf /run/runit/supervise.zapret "$ROOT/etc/sv/zapret/supervise"
install -m755 "$SRC/imports/zapret-gtk/zapret-control" "$ROOT/usr/bin/zapret-control"
in_root useradd --no-create-home --system --shell /bin/false tpws
# zapret-gtk scan results, so the laptop does not have to run blockcheck again
install -d "$ROOT/etc/skel/.config/zapret-gtk"
install -m600 "$SRC/imports/zapret-gtk/strategies.json" "$SRC/imports/zapret-gtk/active_profile.txt" \
	"$ROOT/etc/skel/.config/zapret-gtk/"

step "Building iwd (patched)"
# Stock iwd 3.12 (and upstream) crashes on the laptop's MT7921 as soon as the
# panel asks for signal levels; see build/patches/iwd-rssi-poll-null.patch.
# Only the daemon binary of the Void package is replaced.
IWD_VER=3.12
IWD_SHA=d89a5e45c7180170e19be828f9e944a768c593758094fc57a358d0e7c4cb1a49
IWD_TAR="$OUT/cache/iwd-$IWD_VER.tar.xz"
mkdir -p "$OUT/cache"
[ -e "$IWD_TAR" ] || curl -fsSL -o "$IWD_TAR" \
	"https://www.kernel.org/pub/linux/network/wireless/iwd-$IWD_VER.tar.xz"
echo "$IWD_SHA  $IWD_TAR" | sha256sum -c --quiet
in_root xbps-query -p pkgver iwd | grep -q "^iwd-${IWD_VER}_"
mkdir -p /build/iwd && tar -xf "$IWD_TAR" -C /build/iwd --strip-components=1
patch -s -d /build/iwd -p1 < "$SRC/build/patches/iwd-rssi-poll-null.patch"
( cd /build/iwd && ./configure --prefix=/usr --libexecdir=/usr/libexec --sysconfdir=/etc \
	--localstatedir=/var --disable-systemd-service --disable-manual-pages --disable-client \
	--disable-monitor --disable-wired --disable-ofono --disable-dbus-policy \
	CFLAGS="-O2 -march=goldmont-plus" >/dev/null && make -j"$(nproc)" >/dev/null 2>&1 )
[ -x "$ROOT/usr/libexec/iwd" ]
install -s -m755 /build/iwd/src/iwd "$ROOT/usr/libexec/iwd"
in_root /usr/libexec/iwd --version

step "Building sfwbar"
SFWBAR_VER=1.0_beta17
SFWBAR_SHA=a4915bc7dd0873c45d0d6b01b070e39a91fd16cfadf730d6a9e48db68a8cd09e
SFWBAR_TAR="$OUT/cache/sfwbar-$SFWBAR_VER.tar.gz"
mkdir -p "$OUT/cache"
[ -e "$SFWBAR_TAR" ] || curl -sL -o "$SFWBAR_TAR" \
	"https://github.com/LBCrion/sfwbar/archive/refs/tags/v$SFWBAR_VER.tar.gz"
echo "$SFWBAR_SHA  $SFWBAR_TAR" | sha256sum -c --quiet
mkdir -p /build/sfwbar && tar -xf "$SFWBAR_TAR" -C /build/sfwbar --strip-components=1
# torOS patch: sfwbar lets taskbars and trays shrink to zero size, so in a bar
# that is sized to its content (the dock) running apps would never appear.
grep -q 'kclass->limit = TRUE;' /build/sfwbar/src/gui/flowgrid.c
sed -i 's/kclass->limit = TRUE;/kclass->limit = FALSE;/' /build/sfwbar/src/gui/flowgrid.c
# torOS patch: :hover state for all widgets, not only real buttons
patch -s -d /build/sfwbar -p1 < "$SRC/build/patches/sfwbar-hover.patch"
# torOS patch: the Wi-Fi window's Scan button could stay on "Scanning ..." for ever
patch -s -d /build/sfwbar -p1 < "$SRC/build/patches/sfwbar-scan-state.patch"
# torOS patch: SVG icons (Wi-Fi, brightness) kept the old text colour after a
# light/dark switch
patch -s -d /build/sfwbar -p1 < "$SRC/build/patches/sfwbar-svg-recolour.patch"
# torOS patch: the dock works like GNOME's dash (pinned applications, one icon
# per application, its windows listed on a second click)
patch -s -d /build/sfwbar -p1 < "$SRC/build/patches/sfwbar-dock.patch"
# torOS patch: now and then the panel never appeared at login. Loading its
# modules has a race (a result read from memory that is already freed) which
# ends the modules' thread and leaves the panel waiting for it for ever.
patch -s -d /build/sfwbar -p1 < "$SRC/build/patches/sfwbar-module-race.patch"
CFLAGS="-O2 -march=goldmont-plus" meson setup /build/sfwbar/build /build/sfwbar \
	--prefix=/usr --buildtype=plain -Dalsa=disabled -Dmpd=disabled -Dnm=disabled \
	-Dbsdctl=disabled -Dbuild-docs=disabled >/dev/null
ninja -C /build/sfwbar/build >/dev/null
DESTDIR="$ROOT" meson install -C /build/sfwbar/build >/dev/null
# torOS fixes to sfwbar's stock widgets (checked so a new sfwbar version that
# changes these lines fails the build instead of silently losing the fix):
W="$ROOT/usr/share/sfwbar"
# 1. Brightness: the widget sets it through systemd-logind, which torOS does not
#    have. Use brightnessctl, and scale the slider position (0..1) to percent.
grep -q 'DBusCall(SessionInterface, "SetBrightness"' "$W/backlight.source"
sed -i '/DBusCall(SessionInterface, "SetBrightness"/,/Max(min_brightness, pct )))\])/c\    Exec("brightnessctl -q set " + Str(Max(1, Min(max_brightness, Max(min_brightness, pct))), 0) + "%")' "$W/backlight.source"
sed -i 's/SetBacklight(GtkEvent("dir"))/SetBacklight(GtkEvent("dir") * 100)/' "$W/backlight.source"
grep -q 'brightnessctl -q set' "$W/backlight.source" && grep -q 'GtkEvent("dir") \* 100' "$W/backlight.source"
# 2. Confirm/cancel icons that the Adwaita icon theme does not have
grep -q "dialog-ok" "$W/session.widget" && grep -q "dialog-ok" "$W/wifi-secret.widget"
sed -i -e 's/dialog-ok/object-select-symbolic/g' -e 's/dialog-cancel/window-close-symbolic/g' \
	"$W/session.widget" "$W/wifi-secret.widget"

# 3. The level bar under the brightness icon is plain blue; use the accent
grep -q 'stroke="blue"' "$W/backlight.widget"
sed -i "s/stroke=\"blue\"/stroke=\"#$(. "$SRC/rootfs/usr/share/toros/palette" && echo "$accent")\"/" "$W/backlight.widget"
grep -q 'stroke="#[0-9a-f]\{6\}"' "$W/backlight.widget"

if in_root ldd /usr/bin/sfwbar | grep 'not found'; then
	echo "sfwbar: missing runtime libraries (add them to packages.txt)" >&2; exit 1
fi

step "Installing Thorium"
# gz83/thorium M154, SSE4 build (the laptop's Celeron N4020 has no AVX).
THORIUM_VER=154.0.8037.45
THORIUM_SHA=79f7dca16fe98aea6b870ee77f6ee6e47d8d017cfaab11205ea35749e9e337ad
THORIUM_DEB="$OUT/cache/thorium-${THORIUM_VER}_SSE4.deb"
[ -e "$THORIUM_DEB" ] || curl -fsSL -o "$THORIUM_DEB" \
	"https://github.com/gz83/thorium/releases/download/M$THORIUM_VER/thorium-browser_${THORIUM_VER}_SSE4.deb"
echo "$THORIUM_SHA  $THORIUM_DEB" | sha256sum -c --quiet
mkdir -p /build/thorium && ( cd /build/thorium && ar x "$THORIUM_DEB" data.tar.xz )
T=opt/chromium.org/thorium
# Leave out the test shell, chromedriver and Qt shims (about 250 MB), and all
# interface languages except English and Turkish
tar -xf /build/thorium/data.tar.xz -C "$ROOT" --no-same-owner --keep-directory-symlink \
	--exclude="./$T/thorium_shell*" --exclude="./$T/chromedriver" --exclude="./$T/content_shell.pak" \
	--exclude="./$T/shell_resources.pak" --exclude="./$T/libqt*_shim.so" --exclude="./$T/apparmor.d" \
	--exclude='./usr/bin/thorium-shell' --exclude='./usr/bin/thorium-browser' --exclude='./usr/share/applications/thorium-shell.desktop' \
	--exclude='./usr/share/applications/org.chromium.Thorium.desktop' \
	--exclude='./usr/share/gnome-control-center' --exclude='./etc/cron.daily' \
	./opt ./usr
find "$ROOT/$T/locales" -type f ! -name 'en-US*' ! -name 'en-GB*' ! -name 'tr*' -delete
rm -rf /build/thorium
chown root:root "$ROOT/$T/chrome-sandbox" && chmod 4755 "$ROOT/$T/chrome-sandbox"
cp "$SRC/rootfs/$T/initial_preferences" "$ROOT/$T/initial_preferences"
# /usr/bin/thorium-browser is torOS's wrapper, which also runs the
# screen-sharing portal while the browser is open
ln -sf /usr/lib/toros/browser "$ROOT/usr/bin/thorium-browser"
for s in 16 24 32 48 64 128 256; do
	install -Dm644 "$ROOT/$T/product_logo_$s.png" "$ROOT/usr/share/icons/hicolor/${s}x${s}/apps/thorium-browser.png"
done
# The menu entry offered the test shell that is not installed
sed -i '/^\[Desktop Action content-shell\]/,/^$/d; s/content-shell;//' "$ROOT/usr/share/applications/thorium-browser.desktop"
if in_root env LD_LIBRARY_PATH=/$T/lib ldd /$T/thorium | grep 'not found'; then
	echo "Thorium: missing runtime libraries (add them to packages.txt)" >&2; exit 1
fi
in_root /usr/bin/thorium-browser --version

step "Building Oynaz"
# Built from the user's working copy (mounted read-only at /oynaz) for the
# laptop's CPU. Crates and build output are cached in out/cache.
[ -f /oynaz/Cargo.toml ] || { echo "Oynaz source not found (set OYNAZ_SRC)" >&2; exit 1; }
mkdir -p /build/oynaz "$OUT/cache/cargo" "$OUT/cache/oynaz-target"
( cd /oynaz && tar -cf - --exclude=./target . ) | tar -xf - -C /build/oynaz
# torOS patch: let Oynaz's sign-in find the Thorium profile (the only browser here)
grep -q '^const CHROMIUM_BROWSERS: \[(&str, &str, &str); 3\] = \[$' /build/oynaz/src/auth.rs
sed -i -e 's/^const CHROMIUM_BROWSERS: \[(&str, &str, &str); 3\] = \[$/const CHROMIUM_BROWSERS: [(\&str, \&str, \&str); 4] = [\n    ("Thorium", "thorium", "chromium"),/' /build/oynaz/src/auth.rs
grep -q '("Thorium", "thorium", "chromium"),' /build/oynaz/src/auth.rs
( cd /build/oynaz && CARGO_HOME="$OUT/cache/cargo" CARGO_TARGET_DIR="$OUT/cache/oynaz-target" \
	RUSTFLAGS="-C target-cpu=goldmont-plus" cargo build --release --locked 2>&1 | tail -n 3 )
install -Dm755 "$OUT/cache/oynaz-target/release/oynaz" "$ROOT/usr/bin/oynaz"
install -Dm644 /build/oynaz/data/io.github.oynaz.Oynaz.desktop "$ROOT/usr/share/applications/io.github.oynaz.Oynaz.desktop"
install -Dm644 /build/oynaz/data/icons/hicolor/scalable/apps/io.github.oynaz.Oynaz.svg \
	"$ROOT/usr/share/icons/hicolor/scalable/apps/io.github.oynaz.Oynaz.svg"
if in_root ldd /usr/bin/oynaz | grep 'not found'; then
	echo "Oynaz: missing runtime libraries (add them to packages.txt)" >&2; exit 1
fi
# (the hardware decoders, vavp9dec and friends, only register on a machine with the GPU)
for e in gtk4paintablesink vp9dec opusdec uridecodebin3 pulsesink souphttpsrc; do
	in_root gst-inspect-1.0 --exists "$e" || echo "WARNING: GStreamer element $e is missing"
done

step "Building zapret-gtk"
# The GTK4/libadwaita front end for zapret, from a pinned commit (0.5.4, the
# version whose root helper is imports/zapret-gtk/zapret-control).
ZGTK_REV=5908baa88196c61703f017e4943108f62835c605
ZGTK_SHA=c689b038232273ad7416219e782e3e930580431faec10cb5e084e7d39b24cdc7
ZGTK_TAR="$OUT/cache/zapret-gtk-$ZGTK_REV.tar.gz"
[ -e "$ZGTK_TAR" ] || curl -fsSL -o "$ZGTK_TAR" "https://github.com/Taygun86/zapret-gtk/archive/$ZGTK_REV.tar.gz"
echo "$ZGTK_SHA  $ZGTK_TAR" | sha256sum -c --quiet
mkdir -p /build/zapret-gtk "$OUT/cache/zapret-gtk-target" && tar -xf "$ZGTK_TAR" -C /build/zapret-gtk --strip-components=1
# torOS patch: the app asks for the password at every start to write a
# temporary polkit rule. torOS has no password dialog; its rule is permanent
# (/etc/polkit-1/rules.d/90-zapret-gtk.rules), so point the app's check at it.
grep -q 'let rule_file = Path::new("/run/polkit-1/rules.d/90-zapret-gtk.rules");' /build/zapret-gtk/src/main.rs
sed -i 's|let rule_file = Path::new("/run/polkit-1/rules.d/90-zapret-gtk.rules");|let rule_file = Path::new("/etc/polkit-1/rules.d/90-zapret-gtk.rules");|' /build/zapret-gtk/src/main.rs
# the helper in imports/ must be the one this version expects, or the app
# would try to replace it
grep -q '# VERSION: 9' /build/zapret-gtk/src/main.rs && grep -q '^# VERSION: 9$' "$ROOT/usr/bin/zapret-control" \
	&& grep -q 'apply-strategy' "$ROOT/usr/bin/zapret-control"
( cd /build/zapret-gtk && CARGO_HOME="$OUT/cache/cargo" CARGO_TARGET_DIR="$OUT/cache/zapret-gtk-target" \
	RUSTFLAGS="-C target-cpu=goldmont-plus" cargo build --release --locked 2>&1 | tail -n 3 )
install -Dm755 "$OUT/cache/zapret-gtk-target/release/zapret-gtk" "$ROOT/usr/bin/zapret-gtk"
install -Dm644 /build/zapret-gtk/zapret-gtk.desktop "$ROOT/usr/share/applications/zapret-gtk.desktop"
install -Dm644 /build/zapret-gtk/zapretgtk512.png "$ROOT/usr/share/icons/hicolor/512x512/apps/zapret-gtk.png"
install -Dm644 /build/zapret-gtk/zapretgtk128.png "$ROOT/usr/share/icons/hicolor/128x128/apps/zapret-gtk.png"
if in_root ldd /usr/bin/zapret-gtk | grep 'not found'; then
	echo "zapret-gtk: missing runtime libraries (add them to packages.txt)" >&2; exit 1
fi

step "Apps and portals"
# The file manager would start a file indexer (localsearch) that reads the
# whole home directory; without these files it searches by itself instead.
rm -f "$ROOT"/usr/share/dbus-1/services/org.freedesktop.LocalSearch3*.service \
	"$ROOT"/usr/share/dbus-1/services/org.freedesktop.Tracker3.Miner.*.service \
	"$ROOT"/etc/xdg/autostart/localsearch-*.desktop
# Two helpers must not be started by D-Bus on its own, because with current
# GLib every GTK app asks for them and they would then run all the time
# (about 45 MB): the portal (started by /usr/lib/toros/browser) and gvfsd
# (started by toros-files).
for f in org.freedesktop.portal.Desktop.service org.gtk.vfs.Daemon.service; do
	[ -e "$ROOT/usr/share/dbus-1/services/$f" ] || { echo "missing: $f" >&2; exit 1; }
	rm "$ROOT/usr/share/dbus-1/services/$f"
done
[ -x "$ROOT/usr/libexec/xdg-desktop-portal" ] && [ -x "$ROOT/usr/libexec/gvfsd" ]
# Files goes through toros-files however it is started
grep -q '^Exec=nautilus ' "$ROOT/usr/share/applications/org.gnome.Nautilus.desktop"
sed -i 's|^Exec=nautilus |Exec=toros-files |' "$ROOT/usr/share/applications/org.gnome.Nautilus.desktop"
for f in org.gnome.Nautilus.service org.freedesktop.FileManager1.service; do
	grep -q '^Exec=/usr/bin/nautilus ' "$ROOT/usr/share/dbus-1/services/$f"
	sed -i 's|^Exec=/usr/bin/nautilus |Exec=/usr/bin/toros-files |' "$ROOT/usr/share/dbus-1/services/$f"
done
chown -R 0:0 "$ROOT/etc/polkit-1/rules.d" && chmod 755 "$ROOT/etc/polkit-1/rules.d" && chmod 644 "$ROOT"/etc/polkit-1/rules.d/*.rules
for f in usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.wlr.service \
	usr/share/dbus-1/system-services/org.freedesktop.PolicyKit1.service \
	usr/share/applications/org.gnome.TextEditor.desktop usr/share/applications/org.gnome.Loupe.desktop \
	usr/share/applications/org.gnome.Papers.desktop usr/share/applications/com.ezratweaver.AdwBluetooth.desktop; do
	[ -e "$ROOT/$f" ] || { echo "missing: /$f" >&2; exit 1; }
done

step "Installing adw-gtk3"
# The GTK3 theme that looks like libadwaita (the panel and the Bluetooth
# manager are GTK3). Not packaged by Void; the release archive holds the
# finished theme, light and dark.
ADW_GTK3_VER=6.5
ADW_GTK3_SHA=a81780fadfc432be0fc3d89c4ebb41aa28e4f032d42c36f9789c57dd10cfa41c
ADW_GTK3_TAR="$OUT/cache/adw-gtk3v$ADW_GTK3_VER.tar.xz"
[ -e "$ADW_GTK3_TAR" ] || curl -fsSL -o "$ADW_GTK3_TAR" \
	"https://github.com/lassekongo83/adw-gtk3/releases/download/v$ADW_GTK3_VER/adw-gtk3v$ADW_GTK3_VER.tar.xz"
echo "$ADW_GTK3_SHA  $ADW_GTK3_TAR" | sha256sum -c --quiet
mkdir -p "$ROOT/usr/share/themes"
tar -xf "$ADW_GTK3_TAR" -C "$ROOT/usr/share/themes" --no-same-owner
[ -s "$ROOT/usr/share/themes/adw-gtk3/gtk-3.0/gtk.css" ] && [ -s "$ROOT/usr/share/themes/adw-gtk3-dark/gtk-3.0/gtk.css" ]

step "Desktop defaults"
in_root glib-compile-schemas /usr/share/glib-2.0/schemas
# toros-theme must be able to write every themed file from the palette, in
# both light and dark, with no placeholder left unfilled
for scheme in light dark; do
	rm -rf "$ROOT/tmp/theme-check" && mkdir -p "$ROOT/tmp/theme-check/.config/toros"
	echo "$scheme" > "$ROOT/tmp/theme-check/.config/toros/scheme"
	chroot "$ROOT" /usr/bin/env -i PATH=/usr/bin HOME=/tmp/theme-check /usr/bin/toros-theme files
	if grep -rn '@[a-z_]*@' "$ROOT/tmp/theme-check"; then
		echo "toros-theme: a colour is missing from the palette ($scheme)" >&2; exit 1
	fi
	for f in .config/fuzzel/fuzzel.ini .config/mako/config .config/foot/foot.ini .config/swaylock/config \
		.config/gtk-3.0/gtk.css .config/gtk-4.0/gtk.css .local/share/themes/torOS/labwc/themerc \
		.local/share/themes/torOS/labwc/max_toggled_hover-inactive.svg; do
		[ -s "$ROOT/tmp/theme-check/$f" ] || { echo "toros-theme did not write $f ($scheme)" >&2; exit 1; }
	done
	[ -s "$ROOT/usr/share/toros/wallpaper-$scheme.png" ] || { echo "wallpaper-$scheme.png is missing" >&2; exit 1; }
done
rm -rf "$ROOT/tmp/theme-check"
in_root gtk-update-icon-cache -q -f /usr/share/icons/hicolor 2>/dev/null || true
in_root update-desktop-database -q /usr/share/applications 2>/dev/null || true

step "Audio and session configuration"
# One "pipewire" process starts WirePlumber and the PulseAudio server too
mkdir -p "$ROOT/etc/pipewire/pipewire.conf.d" "$ROOT/etc/alsa/conf.d"
ln -sf /usr/share/examples/wireplumber/10-wireplumber.conf "$ROOT/etc/pipewire/pipewire.conf.d/"
ln -sf /usr/share/examples/pipewire/20-pipewire-pulse.conf "$ROOT/etc/pipewire/pipewire.conf.d/"
for f in 50-pipewire.conf 99-pipewire-default.conf; do
	[ -e "$ROOT/usr/share/alsa/alsa.conf.d/$f" ] && ln -sf "/usr/share/alsa/alsa.conf.d/$f" "$ROOT/etc/alsa/conf.d/"
done
# blueman runs on demand only: no login autostart, and the menu entry goes
# through the wrapper that stops its background processes afterwards
rm -f "$ROOT/etc/xdg/autostart/blueman.desktop"
sed -i 's|^Exec=.*|Exec=toros-bluetooth-manager|' "$ROOT/usr/share/applications/blueman-manager.desktop"
in_root fc-cache -f >/dev/null 2>&1 || true
in_root gtk-update-icon-cache -q -f /usr/share/icons/Adwaita 2>/dev/null || true

step "Creating user $USER_NAME"
GROUPS_WANTED="wheel audio video input kvm bluetooth _seatd network storage"
GROUPS_OK=""
for g in $GROUPS_WANTED; do
	grep -q "^$g:" "$ROOT/etc/group" && GROUPS_OK="${GROUPS_OK:+$GROUPS_OK,}$g"
done
in_root useradd -m -s /bin/bash -G "$GROUPS_OK" "$USER_NAME"
# chpasswd goes through PAM and silently does nothing in this chroot, so set the hash directly
in_root usermod -p "$(openssl passwd -6 toros)" "$USER_NAME"
grep -qF "$USER_NAME:\$6\$" "$ROOT/etc/shadow" || { echo "password for $USER_NAME was not set" >&2; exit 1; }
# No password works for root, but the account is not "locked" ("!"), which
# would also stop sshd from accepting push.sh's key
in_root usermod -p '*' root
# tty2's conf is a symlink to this file, so autologin is limited to tty1 here
cat > "$ROOT/etc/sv/agetty-tty1/conf" <<EOL
GETTY_ARGS="--noclear"
[ "\$tty" = tty1 ] && GETTY_ARGS="--autologin $USER_NAME --noclear"
BAUD_RATE=38400
TERM_NAME=linux
EOL

step "Remote updates"
# push.sh on the desktop PC logs in as root with keys/push; passwords are not
# accepted (/etc/ssh/sshd_config.d/toros.conf).
[ -f "$SRC/keys/push.pub" ] && [ -f "$SRC/keys/host_ed25519" ] || { echo "keys/ not found (build.sh creates it)" >&2; exit 1; }
install -d -m700 "$ROOT/root/.ssh"
install -m600 "$SRC/keys/push.pub" "$ROOT/root/.ssh/authorized_keys"
rm -f "$ROOT"/etc/ssh/ssh_host_*
install -m600 "$SRC/keys/host_ed25519" "$ROOT/etc/ssh/ssh_host_ed25519_key"
install -m644 "$SRC/keys/host_ed25519.pub" "$ROOT/etc/ssh/ssh_host_ed25519_key.pub"
grep -q '^Include /etc/ssh/sshd_config.d/\*.conf' "$ROOT/etc/ssh/sshd_config"
in_root /usr/bin/sshd -t
# Lets push.sh tell which build the laptop is running
date -u +%Y-%m-%d_%H:%M:%S > "$ROOT/etc/toros/build-id"

step "Enabling services"
SVDIR="$ROOT/etc/runit/runsvdir/default"
rm -f "$SVDIR"/agetty-tty[3-6]
for s in udevd dbus seatd iwd bluetoothd acpid zapret sshd agetty-tty1 agetty-tty2; do
	[ -d "$ROOT/etc/sv/$s" ] || { echo "missing service: $s" >&2; exit 1; }
	ln -sfn "/etc/sv/$s" "$SVDIR/$s"
done

step "Boot files"
# Two boot menu entries: torOS (the kernel built above; its command line is
# built in) and recovery (Void's kernel with its initramfs, written by
# /etc/kernel.d/post-install/60-toros-boot). The menu only shows when a key
# (Space) is held down while the laptop starts.
#
# The torOS entry starts toros.efi: the same kernel inside systemd's stub,
# together with the logo (kernel/logo.bmp). The stub puts the logo on the
# screen before the kernel runs; on the laptop the kernel itself cannot show
# anything for its first 1.5 s. vmlinuz-toros stays on the ESP as well: if
# the stub ever fails on some firmware, "linux /vmlinuz-toros" in toros.conf
# starts the kernel without it.
KVER=$(ls "$ROOT/boot" | sed -n 's/^vmlinuz-//p' | grep -vx toros | sort -V | tail -1)
[ -e "$ROOT/boot/initramfs-$KVER.img" ] || { echo "initramfs for $KVER missing" >&2; exit 1; }
in_root /etc/kernel.d/post-install/60-toros-boot linux "$KVER"
grep -q "^linux   /vmlinuz-$KVER\$" "$ROOT/boot/loader/entries/recovery.conf"
[ -s "$ROOT/boot/vmlinuz-toros" ]
ukify build --linux="$ROOT/boot/vmlinuz-toros" --splash="$SRC/kernel/logo.bmp" \
	--uname="$(cat "$ROOT/boot/version-toros")" --os-release="@$ROOT/etc/os-release" \
	--stub=/usr/lib/systemd/boot/efi/linuxx64.efi.stub --output="$ROOT/boot/toros.efi" >/dev/null
[ -s "$ROOT/boot/toros.efi" ]
printf 'title   torOS\nefi     /toros.efi\n' > "$ROOT/boot/loader/entries/toros.conf"
printf 'default toros.conf\ntimeout 0\neditor  yes\n' > "$ROOT/boot/loader/loader.conf"
cleanup
cp -r "$ROOT/boot/." "$ESP/"
rm -rf "${ROOT:?}/boot" && mkdir "$ROOT/boot"
mkdir -p "$ESP/EFI/BOOT"
cp /usr/lib/systemd/boot/efi/systemd-bootx64.efi "$ESP/EFI/BOOT/BOOTX64.EFI"
rm -rf "$ROOT"/var/cache/xbps/* "$ROOT"/tmp/* 2>/dev/null || true

step "Exporting the file tree"
# The same files as in the image, kept in the podman volume toros-tree for
# push.sh. Written only here, so the volume always holds a complete build.
mkdir -p /tree/rootfs /tree/esp
rsync -aHX --delete "$ROOT/" /tree/rootfs/
rsync -a --delete "$ESP/" /tree/esp/

step "Assembling disk image"
ROOT_MB=$(( $(du -sm "$ROOT" | cut -f1) * 115 / 100 + ROOT_HEADROOM_MB ))
truncate -s "${ESP_MB}M" /build/esp.img
mkfs.vfat -F 32 -n TOROS_ESP /build/esp.img >/dev/null
mcopy -s -i /build/esp.img "$ESP"/* ::
truncate -s "${ROOT_MB}M" /build/root.img
mke2fs -q -t ext4 -L torOS -m 1 -d "$ROOT" /build/root.img

IMG="$OUT/toros.img"
rm -f "$IMG"
truncate -s "$(( 1 + ESP_MB + ROOT_MB + 1 ))M" "$IMG"
sfdisk -q "$IMG" <<EOL
label: gpt
start=1MiB, size=${ESP_MB}MiB, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, name="TOROS_ESP"
start=$(( 1 + ESP_MB ))MiB, size=${ROOT_MB}MiB, type=4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709, name="torOS"
EOL
dd if=/build/esp.img of="$IMG" bs=1M seek=1 conv=notrunc,sparse status=none
dd if=/build/root.img of="$IMG" bs=1M seek=$(( 1 + ESP_MB )) conv=notrunc,sparse status=none

step "Done"
echo "kernel:      $(cat "$ESP/version-toros") (recovery: $KVER)"
echo "root files:  $(du -sh "$ROOT" | cut -f1)"
echo "ESP files:   $(du -sh "$ESP" | cut -f1)"
echo "image:       $IMG ($(du -h --apparent-size "$IMG" | cut -f1))"
cat "$ESP"/loader/entries/*.conf
ls -lh "$ESP" | tail -n +2
