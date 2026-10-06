/* torOS boot splash: keeps the kernel's logo on the screen until the desktop
 * is there, with a thin progress bar under it and no console text.
 *
 *   splash         as the kernel's first program (CONFIG_DEFAULT_INIT in
 *                  kernel/fragment): the same as "start", then it becomes
 *                  /sbin/init (runit)
 *   splash start   by hand, for trying it
 *   splash stop    the desktop is up (labwc's autostart)
 *   splash text    give the screen back to the console (toros-session, when
 *                  the desktop has ended or could not start)
 *
 * The kernel draws the logo in the middle of a black screen as soon as the
 * display driver is up (kernel/logo.ppm). Starting puts the first console into
 * graphics mode, which makes the console leave the screen alone: the logo
 * stays and whatever is printed from then on (runit, services, login) is kept
 * for later instead of being drawn over it. That is why this runs before
 * runit, which prints two lines of its own before its first script. A copy
 * of this program goes on in the background and draws the bar straight into
 * the frame buffer. labwc takes the display over in graphics mode as well, so
 * text never shows in between.
 *
 * The bar is a clock, not a count of steps: it is full after as long as the
 * last start took (remembered in TIMEFILE).
 *
 * If no desktop has come up after TIMEOUT seconds the console gets the screen
 * back, so that a start that hangs can be seen. The recovery kernel, where
 * messages are wanted, starts runit directly, and "start" does nothing there.
 *
 * Built by build/in-container.sh, which passes LOGO_H (the logo's height) and
 * the two colours from /usr/share/toros/palette.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <glob.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <linux/fb.h>
#include <linux/kd.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <sys/utsname.h>

#define VT       "/dev/tty1"
#define TIMEFILE "/var/lib/toros/splash-time"
#define REPORT   "/run/toros-splash.log"
#define TIMEOUT  30.0
#define BAR_W    160
#define BAR_H    4
#define BAR_GAP  44	/* from the logo's lower edge to the bar */

/* An abstract socket: it needs no file system (/run is not mounted yet when
 * the splash starts) and any user's session may write to it. */
static const char NAME[] = "\0toros-splash";

static socklen_t address(struct sockaddr_un *a)
{
	memset(a, 0, sizeof *a);
	a->sun_family = AF_UNIX;
	memcpy(a->sun_path, NAME, sizeof NAME - 1);
	return offsetof(struct sockaddr_un, sun_path) + sizeof NAME - 1;
}

static int vt_mode(const char *tty, int mode)
{
	int fd = open(tty, O_RDWR | O_NOCTTY | O_CLOEXEC), r;

	if (fd < 0)
		return -1;
	r = ioctl(fd, KDSETMODE, mode);
	close(fd);
	return r;
}

static double now(void)
{
	struct timespec t;

	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec + t.tv_nsec / 1e9;
}

static int desktop_running(void)
{
	glob_t g;
	int found = glob("/run/user/*/wayland-*", GLOB_NOSORT, NULL, &g) == 0;

	globfree(&g);
	return found;
}

/* The frame buffer, once it could be opened */
static int fb = -1;
static struct fb_var_screeninfo var;
static struct fb_fix_screeninfo fix;
static int bar_x, bar_y;

static int open_fb(void)
{
	if (fb >= 0)
		return 1;
	fb = open("/dev/fb0", O_RDWR | O_CLOEXEC);
	if (fb < 0)
		return 0;
	if (ioctl(fb, FBIOGET_VSCREENINFO, &var) || ioctl(fb, FBIOGET_FSCREENINFO, &fix)
	    || var.bits_per_pixel != 32) {
		close(fb);
		fb = -2;	/* not a screen this can draw on: no bar */
		return 0;
	}
	bar_x = ((int)var.xres - BAR_W) / 2;
	bar_y = ((int)var.yres + LOGO_H) / 2 + BAR_GAP;
	if (bar_x < 0 || bar_y + BAR_H > (int)var.yres) {
		close(fb);
		fb = -2;
		return 0;
	}
	return 1;
}

static uint32_t pixel(uint32_t rgb)
{
	return (rgb >> 16 & 0xff) << var.red.offset | (rgb >> 8 & 0xff) << var.green.offset
	       | (rgb & 0xff) << var.blue.offset;
}

/* The bar with its first "filled" pixels in the accent colour; the ends are
 * rounded by leaving out the corner pixels. */
static void draw(int filled)
{
	uint32_t line[BAR_W];
	int x, y;

	for (x = 0; x < BAR_W; x++)
		line[x] = pixel(x < filled ? FILL : TRACK);
	for (y = 0; y < BAR_H; y++) {
		int skip = (y == 0 || y == BAR_H - 1) ? 1 : 0;
		off_t at = (off_t)(bar_y + y + var.yoffset) * fix.line_length
			   + (off_t)(bar_x + skip + var.xoffset) * 4;

		if (pwrite(fb, line + skip, (BAR_W - 2 * skip) * 4, at) < 0)
			return;
	}
}

static void report(const char *how, double took, int vt_ok)
{
	FILE *f = fopen(REPORT, "w");

	if (!f)
		return;
	fprintf(f, "splash: %s after %.1f s; console hidden: %s; bar: ", how, took, vt_ok ? "yes" : "NO");
	if (fb >= 0)
		fprintf(f, "%dx%d at %d,%d on a %ux%u screen\n", BAR_W, BAR_H, bar_x, bar_y, var.xres, var.yres);
	else
		fprintf(f, "not drawn (%s)\n", fb == -1 ? "no /dev/fb0" : "unsuitable frame buffer");
	fclose(f);
}

static void run(int sock, int vt_ok)
{
	double start = now(), expected = 5.0, t;
	int filled = -1;
	char cmd = 0;
	FILE *f = fopen(TIMEFILE, "r");

	if (f) {
		if (fscanf(f, "%lf", &expected) != 1)
			expected = 5.0;
		fclose(f);
		expected = expected < 1.0 ? 1.0 : expected > 20.0 ? 20.0 : expected;
	}
	for (;;) {
		struct pollfd p = { .fd = sock, .events = POLLIN };
		int n;

		t = now() - start;
		if (t > TIMEOUT)
			break;
		if (open_fb()) {
			n = t >= expected ? BAR_W : (int)(t / expected * BAR_W);
			if (n != filled)
				draw(filled = n);
		}
		if (poll(&p, 1, 40) > 0 && recv(sock, &cmd, 1, 0) == 1 && (cmd == 's' || cmd == 't'))
			break;
	}
	t = now() - start;
	if (cmd == 's') {
		/* the desktop has the screen; remember how long it took */
		mkdir("/var/lib/toros", 0755);
		f = fopen(TIMEFILE, "w");
		if (f) {
			fprintf(f, "%.1f\n", t);
			fclose(f);
		}
		report("desktop up", t, vt_ok);
	} else if (cmd == 't') {
		vt_mode(VT, KD_TEXT);
		report("ended by the session", t, vt_ok);
	} else if (desktop_running()) {
		/* never told, but a desktop is there: the screen is not ours to take */
		report("no word from the desktop", t, vt_ok);
	} else {
		vt_mode(VT, KD_TEXT);
		report("NO DESKTOP, console shown", t, vt_ok);
	}
}

static int start(void)
{
	struct utsname u;
	struct sockaddr_un a;
	socklen_t len = address(&a);
	size_t n;
	int sock, vt_ok, null;

	if (uname(&u) || (n = strlen(u.release)) < 6 || strcmp(u.release + n - 6, "-toros"))
		return 0;
	sock = socket(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC, 0);
	if (sock < 0 || bind(sock, (struct sockaddr *)&a, len))
		return 0;	/* already running */
	vt_ok = vt_mode(VT, KD_GRAPHICS) == 0;
	switch (fork()) {
	case -1:
		vt_mode(VT, KD_TEXT);
		return 1;
	case 0:
		break;
	default:
		return 0;
	}
	setsid();
	null = open("/dev/null", O_RDWR);
	if (null >= 0) {
		dup2(null, 0);
		dup2(null, 1);
		dup2(null, 2);
		if (null > 2)
			close(null);
	}
	run(sock, vt_ok);
	_exit(0);
}

/* Tell the running splash; returns 0 when it was there to hear it */
static int tell(char cmd)
{
	struct sockaddr_un a;
	socklen_t len = address(&a);
	int sock = socket(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC, 0);

	return sock < 0 || sendto(sock, &cmd, 1, 0, (struct sockaddr *)&a, len) != 1;
}

int main(int argc, char **argv)
{
	const char *cmd = argc > 1 ? argv[1] : "";

	if (getpid() == 1) {
		start();
		argv[0] = "init";
		execv("/sbin/init", argv);
		return 1;
	}
	if (!strcmp(cmd, "start"))
		return start();
	if (!strcmp(cmd, "stop")) {
		tell('s');
		return 0;
	}
	if (!strcmp(cmd, "text")) {
		/* without a splash to ask: the console this was called on */
		if (tell('t'))
			vt_mode("/dev/tty", KD_TEXT);
		return 0;
	}
	fprintf(stderr, "usage: splash start|stop|text\n");
	return 1;
}
