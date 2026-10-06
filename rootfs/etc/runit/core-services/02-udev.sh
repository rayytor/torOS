# torOS: replaces runit-void's 02-udev.sh. Void starts udevd here, scans the
# devices and waits until all are set up (about 3 s on the laptop), and the
# udevd service then stops that daemon and starts its own, which costs the
# desktop another wait. Everything needed to reach the desktop is built into
# the torOS kernel, so under it nothing happens here: the udevd service does
# the scan as soon as it runs (see /etc/rc.local), and Wi-Fi, Bluetooth and
# sound are picked up by their services when their drivers arrive. With the
# recovery kernel (drivers as modules) Void's way is kept.

[ -n "$IS_CONTAINER" ] && return 0

case "$(uname -r)" in
*-toros)
    # Meanwhile, read what the desktop needs first into memory
    ( cat /usr/lib/libgallium-*.so /usr/lib/libEGL_mesa.so.0 /usr/lib/libgbm.so.1 \
          /usr/lib/libwlroots-*.so* /usr/bin/labwc /usr/bin/sfwbar /usr/lib/libgtk-3.so.0 \
          /usr/lib/libgtk-layer-shell.so.0 /usr/lib/libpipewire-0.3.so.0 /usr/bin/pipewire \
          /usr/bin/wireplumber /usr/lib/libwireplumber-0.5.so.0 > /dev/null 2>&1 & )
    ;;
*)
    msg "Starting udev and waiting for devices to settle..."
    udevd --daemon
    udevadm trigger --action=add --type=subsystems
    udevadm trigger --action=add --type=devices
    udevadm settle
    ;;
esac
