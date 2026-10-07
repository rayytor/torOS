//! Typing: the keyboard is one more keyboard to the compositor
//! (virtual-keyboard-unstable-v1, as wtype uses it), with a keymap of its own
//! that has every character of `layout::all` on a key. What is typed goes to
//! the window that has the keys, whatever layout the real keyboard is in.
//!
//! This is a connection of its own beside GTK's.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::time::Instant;
use std::{env, process};

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use crate::layout;

/// wl_keyboard's name for a keymap written as XKB text.
const XKB_V1: u32 = 1;
const PRESSED: u32 = 1;
const RELEASED: u32 = 0;
/// The keys that are not characters, by their place in the keymap.
const BACKSPACE: u32 = 1;
const RETURN: u32 = 2;
const FIRST_CHAR: u32 = 3;

#[derive(Clone, Copy)]
pub enum Key {
    Char(char),
    BackSpace,
    Return,
}

struct State;

pub struct Typist {
    queue: EventQueue<State>,
    keyboard: ZwpVirtualKeyboardV1,
    /// where each character is in the keymap
    codes: HashMap<char, u32>,
    start: Instant,
}

impl Typist {
    /// Become a keyboard. `None` where the compositor does not let programs
    /// be one.
    pub fn connect(layout: &str) -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, queue) = registry_queue_init::<State>(&conn).ok()?;
        let handle = queue.handle();
        let manager: ZwpVirtualKeyboardManagerV1 = globals.bind(&handle, 1..=1, ()).ok()?;
        let seat: WlSeat = globals.bind(&handle, 1..=1, ()).ok()?;
        let keyboard = manager.create_virtual_keyboard(&seat, &handle, ());
        let codes = layout::all().into_iter().zip(FIRST_CHAR..).collect();
        let mut typist = Typist { queue, keyboard, codes, start: Instant::now() };
        typist.set_layout(layout).then_some(typist)
    }

    /// Give the compositor the keymap, named after the layout the keyboard
    /// shows: the panel's flag is the one of the keyboard that typed last.
    pub fn set_layout(&mut self, layout: &str) -> bool {
        let text = self.keymap(&layout::xkb_name(layout));
        let Ok(file) = keymap_file(&text) else { return false };
        self.keyboard.keymap(XKB_V1, file.as_fd(), text.len() as u32 + 1);
        self.keyboard.modifiers(0, 0, 0, 0);
        self.queue.roundtrip(&mut State).is_ok()
    }

    /// Press a key and let it go.
    pub fn tap(&mut self, key: Key) {
        let code = match key {
            Key::BackSpace => BACKSPACE,
            Key::Return => RETURN,
            Key::Char(c) => match self.codes.get(&c) {
                Some(code) => *code,
                None => return,
            },
        };
        let time = self.start.elapsed().as_millis() as u32;
        self.keyboard.key(time, code, PRESSED);
        self.keyboard.key(time, code, RELEASED);
        // (sends it, and reads what the compositor had to say meanwhile)
        let _ = self.queue.roundtrip(&mut State);
    }

    /// The keymap as XKB text: one key for each character, at <K1> onwards.
    /// A keyboard's keys are numbered from 8 on in XKB.
    fn keymap(&self, name: &str) -> String {
        let mut keys = vec![(BACKSPACE, "BackSpace".to_string()), (RETURN, "Return".to_string())];
        keys.extend(self.codes.iter().map(|(c, code)| (*code, format!("U{:04X}", u32::from(*c)))));
        keys.sort();
        let codes: String = keys.iter().map(|(code, _)| format!("<K{code}> = {};\n", code + 8)).collect();
        let symbols: String = keys.iter().map(|(code, symbol)| format!("key <K{code}> {{ [ {symbol} ] }};\n")).collect();
        format!(
            "xkb_keymap {{\n\
             xkb_keycodes \"toros\" {{\nminimum = 8;\nmaximum = 255;\n{codes}}};\n\
             xkb_types \"toros\" {{ include \"complete\" }};\n\
             xkb_compatibility \"toros\" {{ include \"complete\" }};\n\
             xkb_symbols \"toros\" {{\nname[Group1] = \"{}\";\n{symbols}}};\n\
             }};\n",
            name.replace(['"', '\\'], "")
        )
    }
}

/// A keymap is handed over as a file: one with no name, in memory.
fn keymap_file(text: &str) -> io::Result<File> {
    let dir = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    let path = dir.join(format!("toros-keyboard-{}.keymap", process::id()));
    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&path)?;
    let _ = fs::remove_file(&path);
    file.write_all(text.as_bytes())?;
    file.write_all(&[0])?;
    file.flush()?;
    Ok(file)
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ZwpVirtualKeyboardManagerV1);
delegate_noop!(State: ZwpVirtualKeyboardV1);
