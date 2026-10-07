//! toros-lock: the lock screen.
//!
//!   toros-lock    lock the desktop. It returns once the screen is locked and
//!                 the lock screen is drawn (at once if it already is locked),
//!                 so what comes next, a suspend for one, finds it locked.
//!
//! It works like Windows' and GNOME's. First the wallpaper with the time and
//! the date. A key or a click brings the account's name and a password field;
//! a letter that was typed for it is already in the field. Enter or the arrow
//! has PAM check the password (/etc/pam.d/toros-lock). Esc empties the field,
//! then goes back to the clock, and so does half a minute without a key. In
//! the corner are a keyboard for the pointer, the keyboard layout's flag and
//! the battery. A click on the flag, Super+Space or Alt+Shift goes to the
//! next layout, because the desktop's own keys are off while it is locked;
//! and the keyboard is the lock screen's own (keys.rs), because toros-keyboard
//! cannot be shown on a locked screen.
//!
//! The compositor does the locking (ext-session-lock): nothing else is shown
//! or gets a key until this program says so, and if this program dies the
//! screen stays locked with nothing on it. That is why it runs as two: the
//! lock screen, and a watcher that only waits for it and starts it again
//! should it end without having unlocked.

mod keys;
// what is on the keys: the on-screen keyboard's rows
#[allow(dead_code)]
#[path = "../../keyboard/src/layout.rs"]
mod layout;
mod pam;
mod screen;

use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{self, Command, ExitCode, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::env;

/// How the lock screen ends when the compositor would not lock: starting it
/// again would not help.
pub const REFUSED: u8 = 3;
/// The lock screen is told where to say "locked" in this variable.
pub const READY_FD: &str = "TOROS_LOCK_READY_FD";
/// How long the caller is kept waiting for the lock at most, in ms.
const READY_WAIT: i32 = 5000;
/// A lock screen that ended sooner than this did not get to work; after this
/// many of them in a row the watcher gives up.
const TOO_SOON: Duration = Duration::from_secs(5);
const GIVE_UP_AFTER: u32 = 5;

fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        None => lock(),
        Some("--screen") => screen::run(),
        Some(_) => {
            eprintln!("usage: toros-lock");
            ExitCode::from(2)
        }
    }
}

fn runtime_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Start the watcher and wait until its lock screen says the screen is locked.
fn lock() -> ExitCode {
    // One lock screen at a time: the watcher holds this file's lock for as
    // long as it runs.
    let Ok(file) = File::create(runtime_dir().join("toros-lock")) else {
        eprintln!("toros-lock: cannot write to {}", runtime_dir().display());
        return ExitCode::FAILURE;
    };
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return ExitCode::SUCCESS;
    }
    let mut ends: [RawFd; 2] = [0; 2];
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return ExitCode::FAILURE;
    }
    match unsafe { libc::fork() } {
        -1 => return ExitCode::FAILURE,
        0 => {
            // the watcher: on its own, whatever becomes of the caller
            unsafe {
                libc::close(ends[0]);
                libc::setsid();
            }
            watch(ends[1])
        }
        _ => {}
    }
    unsafe { libc::close(ends[1]) };
    let mut wait = libc::pollfd { fd: ends[0], events: libc::POLLIN, revents: 0 };
    let mut byte = 0u8;
    let locked = unsafe {
        libc::poll(&mut wait, 1, READY_WAIT) > 0 && libc::read(ends[0], &mut byte as *mut u8 as *mut libc::c_void, 1) == 1
    };
    if locked {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Run the lock screen until it has unlocked. `ready` is the pipe to the
/// caller, which every lock screen started here gets.
fn watch(ready: RawFd) -> ! {
    let mut too_soon = 0;
    loop {
        let started = Instant::now();
        // (/proc/self/exe: this very program, also after an update has put
        // another file in its place)
        let ended = Command::new("/proc/self/exe")
            .arg0("toros-lock")
            .arg("--screen")
            .env(READY_FD, ready.to_string())
            .stdin(Stdio::null())
            .status();
        match ended.ok().and_then(|status| status.code()) {
            Some(0) => process::exit(0),
            Some(code) if code == REFUSED as i32 => process::exit(1),
            _ => {}
        }
        too_soon = if started.elapsed() < TOO_SOON { too_soon + 1 } else { 0 };
        if too_soon >= GIVE_UP_AFTER {
            eprintln!("toros-lock: the lock screen keeps ending, giving up");
            process::exit(1);
        }
        sleep(Duration::from_millis(200));
    }
}
