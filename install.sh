#!/bin/sh
# Install torOS on the laptop's internal disk. ERASES THAT WHOLE DISK.
#   ./install.sh
# The laptop must be running torOS from the USB stick and be on this PC's
# network. This only starts /usr/bin/toros-install on the laptop (see there);
# it shows the disk and asks for the word "erase" before anything is written.
# Afterwards: switch the laptop off, take the stick out, start it. Updates
# keep coming with ./push.sh.
cd "$(dirname "$0")"
exec ./push.sh ssh toros-install "$@"
