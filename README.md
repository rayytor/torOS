<p align="center"><img src="torOS.png" width="120" alt="torOS logo"></p>

# torOS

A small Linux system built for one laptop: a Lenovo IdeaPad 1i 14IGL7
(Celeron N4020, 4 GB RAM, 64 GB eMMC). It is based on Void Linux, boots a
custom kernel with no initramfs straight into a Wayland desktop, and is rebuilt
as a whole disk image on a desktop PC and sent to the laptop over Wi-Fi.

This is a personal learning project, not a distribution. It only runs
properly on that laptop model, and it is published so the build can be read.

| Light | Dark | Boot splash |
|---|---|---|
| ![Desktop, light](docs/desktop-light.png) | ![Desktop, dark](docs/desktop-dark.png) | ![Boot splash](docs/boot-splash.png) |

(Screenshots are from the QEMU test VM.)

## Measured on the laptop

| | Result | Target |
|---|---|---|
| Kernel start to usable desktop (internal eMMC) | 5.2 s | 6 s |
| Idle RAM with desktop, panel, audio, Bluetooth, Wi-Fi | 162 MB | 150 MB (not met) |
| GTK4 app start, cold after prewarm / warm | 1.1 s / about 0.5 s | 1.5 s / 0.5 s |

Started from a USB stick, the same laptop took 13 s with Void's stock kernel
and 7.9 s with this one. The
logs behind these numbers are in [hardware/](hardware/); device addresses in
them have been replaced with made-up ones.

## What is in it

- **Image build in a container.** [build.sh](build.sh) runs
  [build/in-container.sh](build/in-container.sh) in a rootless podman
  container: it installs the packages in [packages.txt](packages.txt) with
  `xbps-install`, lays [rootfs/](rootfs/) over them, compiles the few programs
  Void does not package, and writes a GPT disk image (ESP + ext4). Everything
  downloaded from outside Void's repositories is pinned by version and SHA-256.
- **Custom kernel, no initramfs.** [build/kernel.sh](build/kernel.sh) starts
  from Void's config, keeps only the modules the laptop loads
  (`localmodconfig` with [kernel/lsmod-laptop.txt](kernel/lsmod-laptop.txt)),
  then applies [kernel/fragment](kernel/fragment): storage, file system and
  display drivers built in, the command line built in, firmware for the GPU
  and CPU embedded. The build fails if a line of the fragment does not survive.
  Void's kernel stays on the disk as a recovery boot entry.
- **Boot splash without Plymouth.** The logo is shown by the boot loader's
  stub, then by the kernel in the same place, then kept by a 250-line C program
  ([build/splash.c](build/splash.c)) that the kernel runs as its first process
  before it hands over to runit.
- **A one-second black screen, found and removed.** The compositor's first mode
  set switched the panel off and on because of one connector property;
  [build/patches/kernel-edp-max-bpc.patch](build/patches/kernel-edp-max-bpc.patch)
  brings it from 1.1 s down to about 20 ms.
- **Two upstream bugs patched.** iwd crashed in a loop on the MT7921 Wi-Fi chip
  (a null pointer in its signal polling), and the sfwbar panel sometimes never
  appeared because of a use-after-free between two threads. Both fixes are in
  [build/patches/](build/patches/).
- **Desktop.** labwc, sfwbar (patched into a dock and a status strip), fuzzel,
  foot and mako, all coloured from one palette file so that they match
  libadwaita apps in light and dark.
- **Hearing aid support.** A Phonak hearing aid connects over Bluetooth
  Classic with one click or `Super+B`, and sound follows it
  ([rootfs/usr/bin/toros-bt](rootfs/usr/bin/toros-bt)).
- **Updates over Wi-Fi.** [push.sh](push.sh) finds the laptop on the local
  network by its SSH host key, rsyncs the new build onto the running system
  and reboots it. It can also take screenshots and run commands there.
- **Tests in QEMU.** [test/qemu.sh](test/qemu.sh) boots the image under UEFI
  and runs a self-test that reports over the serial port;
  [test/vm.py](test/vm.py) drives the VM by hand and can film the boot.

## Layout

```
build.sh, build/      image build (container), kernel build, splash, patches
packages.txt          Void packages in the image
rootfs/               files laid over the image (/etc, /usr)
kernel/               kernel options, the laptop's module list, boot logo
imports/zapret/       zapret source (see below) and its configuration
hardware/             logs and hardware facts collected from the laptop
test/                 QEMU test scripts
push.sh               send a build to the laptop over Wi-Fi
write-usb.sh          write the image to a USB stick
install.sh            install from the stick onto the laptop's internal disk
```

## Building

Needs Linux with rootless podman, and QEMU with OVMF for the tests.

```bash
./build.sh
```

```bash
test/qemu.sh selftest
```

Things to know before trying:

- The build also compiles Oynaz, a separate YouTube client of mine that is not
  published yet, and stops if its source is not at `~/Projects/oynaz` (or
  `OYNAZ_SRC`).
- `build.sh` creates SSH keys in `keys/` and builds them into the image, and
  the image has a fixed first password. Both are fine for a laptop on a home
  network that only its owner updates, and wrong for anything else.
- The kernel and several programs are compiled for the laptop's CPU family
  (`-march=goldmont-plus`) and the kernel only has this laptop's drivers.
- The hearing aid's address is read from `local/hearing-aid` on the build PC,
  which is not in the repository.

## Known gaps

Idle RAM is above the target. Suspend and resume, hardware video decoding in
the browser and screen sharing have not been checked on the custom kernel, and
the hearing aid does not yet reconnect by itself after being switched off and
on. There is no first-run setup and no update path other than `push.sh`.

## Credits and licences

torOS is put together from other people's work:
[Void Linux](https://voidlinux.org/), the Linux kernel,
[labwc](https://github.com/labwc/labwc),
[sfwbar](https://github.com/LBCrion/sfwbar), iwd, PipeWire, BlueZ,
[adw-gtk3](https://github.com/lassekongo83/adw-gtk3),
[Thorium](https://github.com/Alex313031/thorium) and
[zapret-gtk](https://github.com/Taygun86/zapret-gtk). None of these are in the
repository; the build downloads them.

[imports/zapret/](imports/zapret/) is a copy of
[bol-van/zapret](https://github.com/bol-van/zapret) at commit `87e0586`, under
its own MIT licence ([imports/zapret/docs/LICENSE.txt](imports/zapret/docs/LICENSE.txt)),
with my configuration added.

The files in [build/patches/](build/patches/) change the Linux kernel (GPL-2.0),
iwd (LGPL-2.1) and sfwbar (GPL-3.0) and fall under those licences. Everything
else here is under the [MIT licence](LICENSE).

The project was built with a lot of help from Claude Code.
