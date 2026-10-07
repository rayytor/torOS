//! Checking the account's password with PAM: as much of libpam as that takes.
//! The rules are in /etc/pam.d/toros-lock.

use std::ffi::{c_char, c_int, c_void, CString};
use std::ptr;

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_PERM_DENIED: c_int = 6;
const PAM_AUTH_ERR: c_int = 7;
const PAM_USER_UNKNOWN: c_int = 10;
const PAM_MAXTRIES: c_int = 11;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

#[repr(C)]
struct Conversation {
    talk: extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    data: *mut c_void,
}

#[link(name = "pam")]
extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conversation: *const Conversation,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
}

pub enum Answer {
    Right,
    Wrong,
    /// PAM could not check at all (its own error number)
    Failed(i32),
}

/// What PAM asks its questions through. Every question that wants something
/// typed is answered with the password (`data`).
extern "C" fn talk(count: c_int, messages: *mut *const Message, responses: *mut *mut Response, data: *mut c_void) -> c_int {
    if count <= 0 || messages.is_null() || responses.is_null() {
        return PAM_CONV_ERR;
    }
    // PAM frees the answers itself, so they come from malloc
    let answers = unsafe { libc::calloc(count as usize, std::mem::size_of::<Response>()) } as *mut Response;
    if answers.is_null() {
        return PAM_BUF_ERR;
    }
    for i in 0..count as usize {
        let message = unsafe { *messages.add(i) };
        if message.is_null() {
            continue;
        }
        let style = unsafe { (*message).style };
        if style == PAM_PROMPT_ECHO_OFF || style == PAM_PROMPT_ECHO_ON {
            unsafe { (*answers.add(i)).text = libc::strdup(data as *const c_char) };
        }
    }
    unsafe { *responses = answers };
    PAM_SUCCESS
}

/// Is this the password of `user`? Takes as long as PAM does (it waits a
/// while after a wrong one), so it is not for the thread that draws.
pub fn check(user: &str, mut password: Vec<u8>) -> Answer {
    password.retain(|&byte| byte != 0);
    password.push(0);
    let Ok(user) = CString::new(user) else { return Answer::Failed(-1) };
    let conversation = Conversation { talk, data: password.as_mut_ptr() as *mut c_void };
    let mut handle = ptr::null_mut();
    let mut code = unsafe { pam_start(c"toros-lock".as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    if code == PAM_SUCCESS {
        code = unsafe { pam_authenticate(handle, 0) };
        unsafe { pam_end(handle, code) };
    }
    // the copy that was handed over does not stay in memory
    for byte in password.iter_mut() {
        unsafe { ptr::write_volatile(byte, 0) };
    }
    match code {
        PAM_SUCCESS => Answer::Right,
        PAM_AUTH_ERR | PAM_USER_UNKNOWN | PAM_MAXTRIES | PAM_PERM_DENIED => Answer::Wrong,
        other => Answer::Failed(other),
    }
}
