# .bash_profile

[ -f "$HOME/.bashrc" ] && . "$HOME/.bashrc"

# Start the desktop on the autologin console. If it exits or fails, you are
# left at this shell; run "toros-session" to start it again.
if [ -z "$WAYLAND_DISPLAY" ] && [ "$(tty)" = /dev/tty1 ]; then
	toros-session
fi
