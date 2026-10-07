//! The lock screen itself: one window on every monitor, each with the clock
//! and, behind a key or a click, the password field.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{env, fs};

use gtk::gdk::{Key, ModifierType};
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib};
use gtk4_session_lock::Instance;

use crate::keys::{Keys, Typed};
use crate::pam::{self, Answer};
use crate::{READY_FD, REFUSED};

const PALETTE: &str = "/usr/share/toros/palette";
const WALLPAPERS: &str = "/usr/share/toros";
const FLAGS: &str = "/usr/share/toros/flags";
const POWER: &str = "/sys/class/power_supply";
/// The password page goes back to the clock after this long without a key,
/// a click or the pointer moving.
const BACK_TO_CLOCK: Duration = Duration::from_secs(30);
/// The caller is told "locked" when the first picture is drawn, or this long
/// after the compositor has locked (a screen that is off draws nothing).
const DRAWN_WAIT: Duration = Duration::from_millis(400);
/// When the start is timed (`Lock::bench`), the lock is kept this long.
const BENCH_STAYS: Duration = Duration::from_millis(250);
/// How often the battery is read, in ticks of the clock (seconds).
const BATTERY_EVERY: u32 = 30;

struct Lock {
    instance: Instance,
    main_loop: glib::MainLoop,
    /// the account: what PAM knows it by, and what is shown
    login: String,
    name: String,
    dark: bool,
    screens: RefCell<Vec<Rc<Screen>>>,
    /// the keyboard layout in use (us, tr); empty until toros-layout has said
    layout: RefCell<String>,
    /// where the caller waits to hear that the screen is locked
    ready: Cell<Option<RawFd>>,
    locked: Cell<bool>,
    refused: Cell<bool>,
    ticks: Cell<u32>,
    /// for timing the start (/usr/lib/toros/selftest): unlock by itself once
    /// the first picture is drawn
    bench: bool,
}

struct Screen {
    window: gtk::Window,
    pages: gtk::Stack,
    scrim: gtk::Box,
    time: gtk::Label,
    date: gtk::Label,
    sign_in: gtk::Box,
    entry: gtk::PasswordEntry,
    go: gtk::Button,
    go_face: gtk::Stack,
    spinner: gtk::Spinner,
    message: gtk::Label,
    /// the keyboard for the pointer, its button in the corner, and whether
    /// it is wanted (it shows with the password page)
    keys: Rc<Keys>,
    keys_button: gtk::Button,
    keys_on: Cell<bool>,
    layout: gtk::Button,
    flag: gtk::Picture,
    battery: gtk::Box,
    battery_icon: gtk::Image,
    battery_level: gtk::Label,
    last_input: Cell<Instant>,
    /// Alt and Shift are down and no other key was pressed since
    alt_shift: Cell<bool>,
    /// the password is being checked
    busy: Cell<bool>,
}

/// The colours the style sheet uses and whether the desktop is dark. On torOS
/// they come from the palette that the rest of the desktop is coloured from,
/// light or dark as toros-theme last set it; anywhere else from the GTK theme.
fn colours() -> (String, bool) {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"));
    let settings = gtk::Settings::default();
    let dark = match fs::read_to_string(config.join("toros/scheme")) {
        Ok(scheme) => scheme.trim() == "dark",
        Err(_) => settings.as_ref().is_some_and(|s| s.is_gtk_application_prefer_dark_theme()),
    };
    if let Some(settings) = settings {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }

    let palette: HashMap<String, String> = fs::read_to_string(PALETTE)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let mode = if dark { "dark" } else { "light" };
    let from_palette = |name: &str| {
        palette.get(&format!("{mode}_{name}")).or_else(|| palette.get(name)).map(|c| format!("#{c}"))
    };
    let mut css: String = [
        ("desktop", "desktop", "@theme_bg_color"),
        ("window", "bg", "@theme_bg_color"),
        ("surface", "popover", "@theme_base_color"),
        ("fg", "fg", "@theme_fg_color"),
        ("dim", "dim", "alpha(@theme_fg_color, 0.55)"),
        ("border", "border", "@borders"),
        ("accent", "accent", "@theme_selected_bg_color"),
        ("accent_fg", "accent_fg", "@theme_selected_fg_color"),
    ]
    .iter()
    .map(|(name, key, fallback)| {
        format!("@define-color lk_{name} {};\n", from_palette(key).unwrap_or(fallback.to_string()))
    })
    .collect();
    // (libadwaita's colour for what went wrong, as text)
    css += &format!("@define-color lk_error {};\n", if dark { "#ff938c" } else { "#c30000" });
    (css, dark)
}

/// The account this runs as: its login name and the name to show.
fn account() -> (String, String) {
    let entry = unsafe { libc::getpwuid(libc::getuid()) };
    if entry.is_null() {
        let login = env::var("USER").unwrap_or_default();
        return (login.clone(), login);
    }
    let text = |field: *const libc::c_char| {
        if field.is_null() {
            String::new()
        } else {
            unsafe { std::ffi::CStr::from_ptr(field) }.to_string_lossy().into_owned()
        }
    };
    let login = text(unsafe { (*entry).pw_name });
    let full = text(unsafe { (*entry).pw_gecos });
    let name = full.split(',').next().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&login).to_string();
    (login, name)
}

/// The desktop's wallpaper at the size of this monitor, so that drawing it is
/// a copy (in software, scaling the whole picture for every frame is not).
fn wallpaper(dark: bool, monitor: &gdk::Monitor) -> Option<gdk::Texture> {
    let file = format!("{WALLPAPERS}/wallpaper-{}.png", if dark { "dark" } else { "light" });
    let area = monitor.geometry();
    let (width, height) = (area.width() * monitor.scale_factor(), area.height() * monitor.scale_factor());
    let (_, picture_width, picture_height) = gdk_pixbuf::Pixbuf::file_info(&file)?;
    // it covers the screen; what is too much on one side is cut off
    let scale = f64::max(width as f64 / picture_width as f64, height as f64 / picture_height as f64);
    let fitted = gdk_pixbuf::Pixbuf::from_file_at_scale(
        &file,
        (picture_width as f64 * scale).ceil() as i32,
        (picture_height as f64 * scale).ceil() as i32,
        false,
    )
    .ok()?;
    Some(gdk::Texture::for_pixbuf(&fitted))
}

/// The battery's level in percent and whether it is being charged, if the
/// machine has one.
fn battery() -> Option<(u32, bool)> {
    let supply = fs::read_dir(POWER).ok()?.flatten().find(|s| s.file_name().to_string_lossy().starts_with("BAT"))?;
    let read = |file: &str| fs::read_to_string(supply.path().join(file)).ok().map(|text| text.trim().to_string());
    let level: u32 = read("capacity")?.parse().ok()?;
    Some((level.min(100), read("status").is_some_and(|status| status != "Discharging")))
}

impl Screen {
    fn build(lock: &Rc<Lock>, monitor: &gdk::Monitor) -> Rc<Self> {
        let vertical = || gtk::Box::builder().orientation(gtk::Orientation::Vertical).halign(gtk::Align::Center);

        // the clock
        let time = gtk::Label::builder().css_classes(["time"]).build();
        let date = gtk::Label::builder().css_classes(["date"]).build();
        let clock = vertical().valign(gtk::Align::Start).css_classes(["clock"]).build();
        clock.append(&time);
        clock.append(&date);

        // the account and its password
        let avatar = gtk::Image::builder()
            .icon_name("avatar-default-symbolic")
            .pixel_size(48)
            .halign(gtk::Align::Center)
            .css_classes(["avatar"])
            .build();
        let name = gtk::Label::builder().label(&lock.name).css_classes(["name"]).build();
        let entry = gtk::PasswordEntry::builder().placeholder_text("Password").show_peek_icon(true).hexpand(true).build();
        // The eye and the Caps Lock warning in the field come with tooltips,
        // and a tooltip is a window of its own, which a lock screen cannot
        // have. (GTK gives the eye a new one each time it is clicked.)
        let mut part = entry.first_child();
        while let Some(widget) = part {
            if widget.is::<gtk::Image>() {
                widget.set_has_tooltip(false);
                widget.connect_has_tooltip_notify(|widget| {
                    if widget.has_tooltip() {
                        widget.set_has_tooltip(false);
                    }
                });
            }
            part = widget.next_sibling();
        }
        let spinner = gtk::Spinner::new();
        let go_face = gtk::Stack::new();
        go_face.add_named(&gtk::Image::from_icon_name("go-next-symbolic"), Some("arrow"));
        go_face.add_named(&spinner, Some("spinner"));
        let go = gtk::Button::builder()
            .child(&go_face)
            .valign(gtk::Align::Center)
            .can_focus(false)
            .css_classes(["go"])
            .build();
        let field = gtk::Box::builder().css_classes(["field"]).build();
        field.append(&entry);
        field.append(&go);
        let message = gtk::Label::builder().css_classes(["message"]).build();
        let sign_in = vertical().valign(gtk::Align::Center).css_classes(["signin"]).build();
        sign_in.append(&avatar);
        sign_in.append(&name);
        sign_in.append(&field);
        sign_in.append(&message);

        let pages = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(200)
            .build();
        pages.add_named(&clock, Some("clock"));
        pages.add_named(&sign_in, Some("signin"));

        // the corner: the keyboard for the pointer, the keyboard layout and
        // the battery
        let keys = Keys::new();
        let keys_button = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("input-keyboard-symbolic"))
            .can_focus(false)
            .build();
        let flag = gtk::Picture::builder().can_shrink(false).valign(gtk::Align::Center).build();
        let layout = gtk::Button::builder().child(&flag).can_focus(false).visible(false).build();
        let battery_icon = gtk::Image::new();
        let battery_level = gtk::Label::new(None);
        let battery = gtk::Box::builder().visible(false).css_classes(["battery"]).build();
        battery.append(&battery_icon);
        battery.append(&battery_level);
        let corner = gtk::Box::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .spacing(2)
            .css_classes(["corner"])
            .build();
        corner.append(&keys_button);
        corner.append(&layout);
        corner.append(&battery);

        // the wallpaper, and over it what dims it behind the password page
        let scrim = gtk::Box::builder().css_classes(["scrim"]).build();
        let layers = gtk::Overlay::new();
        match wallpaper(lock.dark, monitor) {
            Some(picture) => {
                let picture = gtk::Picture::for_paintable(&picture);
                picture.set_content_fit(gtk::ContentFit::Cover);
                picture.set_can_shrink(true);
                layers.set_child(Some(&picture));
            }
            None => layers.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0))),
        }
        layers.add_overlay(&scrim);
        layers.add_overlay(&pages);
        layers.add_overlay(&keys.panel);
        layers.add_overlay(&corner);

        let window = gtk::Window::builder().decorated(false).css_classes(["lock"]).child(&layers).build();
        // (the keys have Apple's colours, light and dark, and not the palette's)
        if lock.dark {
            window.add_css_class("dark");
        }

        let screen = Rc::new(Screen {
            window,
            pages,
            scrim,
            time,
            date,
            sign_in,
            entry,
            go,
            go_face,
            spinner,
            message,
            keys,
            keys_button,
            keys_on: Cell::new(false),
            layout,
            flag,
            battery,
            battery_icon,
            battery_level,
            last_input: Cell::new(Instant::now()),
            alt_shift: Cell::new(false),
            busy: Cell::new(false),
        });

        screen.entry.connect_activate(clone!(
            #[weak]
            lock,
            #[weak]
            screen,
            move |_| lock.check(&screen)
        ));
        screen.go.connect_clicked(clone!(
            #[weak]
            lock,
            #[weak]
            screen,
            move |_| lock.check(&screen)
        ));
        // what was wrong with the last password goes when the next is begun
        screen.entry.connect_changed(clone!(
            #[weak]
            screen,
            move |entry| {
                if !entry.text().is_empty() {
                    screen.message.set_label("");
                }
            }
        ));
        screen.layout.connect_clicked(clone!(
            #[weak]
            lock,
            move |_| lock.layout(true)
        ));
        screen.keys_button.connect_clicked(clone!(
            #[weak]
            screen,
            move |_| screen.toggle_keys()
        ));
        screen.keys.connect_typed(clone!(
            #[weak]
            lock,
            #[weak]
            screen,
            move |typed| lock.typed(&screen, typed)
        ));

        // The keys are looked at before the field gets them
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(clone!(
            #[weak]
            lock,
            #[weak]
            screen,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, held| lock.key(&screen, key, held)
        ));
        keys.connect_key_released(clone!(
            #[weak]
            lock,
            #[weak]
            screen,
            move |_, key, _, _| {
                // Alt+Shift, let go with no other key in between
                if screen.alt_shift.replace(false) && (is_shift(key) || is_alt(key)) {
                    lock.layout(true);
                }
            }
        ));
        screen.window.add_controller(keys);
        // a click on the clock brings the password page
        let click = gtk::GestureClick::new();
        click.connect_released(clone!(
            #[weak]
            screen,
            move |_, _, _, _| {
                screen.last_input.set(Instant::now());
                if screen.on_clock() {
                    screen.show_sign_in();
                }
            }
        ));
        screen.window.add_controller(click);
        // A menu is a window of its own, which a lock screen cannot have: the
        // field's is not opened (nor by the Menu key, see `key`)
        let menu = gtk::GestureClick::builder().button(3).propagation_phase(gtk::PropagationPhase::Capture).build();
        menu.connect_pressed(|menu, _, _, _| {
            menu.set_state(gtk::EventSequenceState::Claimed);
        });
        screen.window.add_controller(menu);
        let pointer = gtk::EventControllerMotion::new();
        pointer.connect_motion(clone!(
            #[weak]
            screen,
            move |_, _, _| screen.last_input.set(Instant::now())
        ));
        screen.window.add_controller(pointer);

        // The first picture is on the screen: the caller can go on
        screen.window.connect_realize(clone!(
            #[weak]
            lock,
            move |window| {
                let Some(frames) = window.frame_clock() else { return };
                frames.connect_after_paint(clone!(
                    #[weak]
                    lock,
                    move |_| {
                        if lock.locked.get() {
                            lock.drawn();
                        }
                    }
                ));
            }
        ));
        screen
    }

    fn on_clock(&self) -> bool {
        self.pages.visible_child_name().as_deref() == Some("clock")
    }

    fn show_sign_in(&self) {
        self.pages.set_visible_child_name("signin");
        self.scrim.add_css_class("on");
        self.place_keys();
        self.entry.grab_focus();
    }

    fn show_clock(&self) {
        self.entry.set_text("");
        self.message.set_label("");
        self.pages.set_visible_child_name("clock");
        self.scrim.remove_css_class("on");
        self.keys.reset();
        self.place_keys();
    }

    /// The keyboard's button was clicked: show the keys, or put them away.
    /// At the clock that is a wish to type the password.
    fn toggle_keys(&self) {
        self.last_input.set(Instant::now());
        if self.on_clock() {
            self.keys_on.set(true);
            self.show_sign_in();
        } else {
            self.keys_on.set(!self.keys_on.get());
            self.place_keys();
        }
    }

    /// The keys are there with the password page, if they are wanted; the
    /// account and the field then move up to make room.
    fn place_keys(&self) {
        let shown = self.keys_on.get() && !self.on_clock();
        self.keys.panel.set_visible(shown);
        for (widget, class) in [(self.sign_in.upcast_ref::<gtk::Widget>(), "typing"), (self.keys_button.upcast_ref(), "on")] {
            if shown {
                widget.add_css_class(class);
            } else {
                widget.remove_css_class(class);
            }
        }
    }

    /// While the password is checked nothing more can be typed, and the arrow
    /// is a spinner.
    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        self.entry.set_sensitive(!busy);
        self.go.set_sensitive(!busy);
        self.spinner.set_spinning(busy);
        self.go_face.set_visible_child_name(if busy { "spinner" } else { "arrow" });
    }

    fn refuse(&self, why: &str) {
        self.set_busy(false);
        self.message.set_label(why);
        self.last_input.set(Instant::now());
        if !self.on_clock() {
            self.entry.grab_focus();
        }
    }

    fn show_time(&self, time: &str, date: &str) {
        if self.time.label() != time {
            self.time.set_label(time);
        }
        if self.date.label() != date {
            self.date.set_label(date);
        }
    }

    fn show_layout(&self, code: &str) {
        let flag = format!("{FLAGS}/{code}.svg");
        let known = !code.is_empty() && Path::new(&flag).exists();
        if known {
            self.flag.set_filename(Some(&flag));
        }
        self.layout.set_visible(known);
        self.keys.set_layout(code);
    }

    fn show_battery(&self, battery: Option<(u32, bool)>) {
        if let Some((level, charging)) = battery {
            // the icons the status panel shows: Adwaita's, in tens
            let tens = (level + 5) / 10;
            let state = match (charging, tens >= 10) {
                (false, _) => "",
                (true, false) => "-charging",
                (true, true) => "-charged",
            };
            self.battery_icon.set_icon_name(Some(&format!("battery-level-{}{state}-symbolic", tens * 10)));
            self.battery_level.set_label(&format!("{level} %"));
        }
        self.battery.set_visible(battery.is_some());
    }
}

fn is_shift(key: Key) -> bool {
    matches!(key, Key::Shift_L | Key::Shift_R)
}

/// (Alt pressed while Shift is held comes as "Meta")
fn is_alt(key: Key) -> bool {
    matches!(key, Key::Alt_L | Key::Alt_R | Key::Meta_L | Key::Meta_R)
}

impl Lock {
    /// A key was pressed on `screen`. What is not used here goes on to the
    /// password field.
    fn key(self: &Rc<Self>, screen: &Rc<Screen>, key: Key, held: ModifierType) -> glib::Propagation {
        screen.last_input.set(Instant::now());
        if is_shift(key) || is_alt(key) {
            screen.alt_shift.set(
                (is_shift(key) && held.contains(ModifierType::ALT_MASK))
                    || (is_alt(key) && held.contains(ModifierType::SHIFT_MASK)),
            );
            return glib::Propagation::Proceed;
        }
        screen.alt_shift.set(false);
        // the other keys that only go with another one do nothing by themselves
        if matches!(
            key,
            Key::Control_L
                | Key::Control_R
                | Key::Super_L
                | Key::Super_R
                | Key::Hyper_L
                | Key::Hyper_R
                | Key::Caps_Lock
                | Key::Num_Lock
                | Key::ISO_Level3_Shift
        ) {
            return glib::Propagation::Proceed;
        }
        if key == Key::space && held.contains(ModifierType::SUPER_MASK) {
            self.layout(true);
            return glib::Propagation::Stop;
        }
        // the field's menu and the emoji chooser: windows of their own (see `build`)
        if key == Key::Menu
            || (key == Key::F10 && held.contains(ModifierType::SHIFT_MASK))
            || (held.contains(ModifierType::CONTROL_MASK) && matches!(key, Key::period | Key::semicolon))
        {
            return glib::Propagation::Stop;
        }

        if screen.on_clock() {
            screen.show_sign_in();
            // a letter that was typed at the clock is the password's first
            let typed = held.intersection(ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK).is_empty();
            if let Some(letter) = key.to_unicode().filter(|c| typed && !c.is_control() && !c.is_whitespace()) {
                screen.entry.set_text(letter.encode_utf8(&mut [0; 4]));
                screen.entry.set_position(-1);
            }
            return glib::Propagation::Stop;
        }
        if key == Key::Escape {
            if !screen.busy.get() {
                if screen.entry.text().is_empty() {
                    screen.show_clock();
                } else {
                    screen.entry.set_text("");
                }
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    /// Have PAM check what is in the field of `screen`, and unlock if it is
    /// the password.
    fn check(self: &Rc<Self>, screen: &Rc<Screen>) {
        if screen.busy.get() || screen.entry.text().is_empty() {
            return;
        }
        let password = screen.entry.text().as_bytes().to_vec();
        screen.entry.set_text("");
        screen.message.set_label("");
        screen.set_busy(true);
        let login = self.login.clone();
        glib::spawn_future_local(clone!(
            #[strong(rename_to = lock)]
            self,
            #[strong]
            screen,
            async move {
                // (PAM waits two seconds after a wrong password)
                match gio::spawn_blocking(move || pam::check(&login, password)).await {
                    Ok(Answer::Right) => lock.instance.unlock(),
                    Ok(Answer::Wrong) => screen.refuse("The password is incorrect. Try again."),
                    Ok(Answer::Failed(code)) => {
                        eprintln!("toros-lock: PAM could not check the password (error {code})");
                        screen.refuse("The password could not be checked.");
                    }
                    Err(_) => screen.refuse("The password could not be checked."),
                }
            }
        ));
    }

    /// A key of the keyboard on the screen was clicked: it goes into the
    /// password field as if it had been typed.
    fn typed(self: &Rc<Self>, screen: &Rc<Screen>, typed: Typed) {
        screen.last_input.set(Instant::now());
        if screen.busy.get() {
            return;
        }
        let entry = &screen.entry;
        match typed {
            Typed::Char(letter) => {
                entry.delete_selection();
                let mut at = entry.position();
                entry.insert_text(letter.encode_utf8(&mut [0; 4]), &mut at);
                entry.set_position(at);
            }
            Typed::Delete => {
                let at = entry.position();
                if entry.selection_bounds().is_some() {
                    entry.delete_selection();
                } else if at > 0 {
                    entry.delete_text(at - 1, at);
                }
            }
            Typed::Return => self.check(screen),
        }
    }

    /// Show the keyboard layout in use, after going to the next one if asked
    /// (toros-layout: it is the compositor's keyboard that changes).
    fn layout(self: &Rc<Self>, next: bool) {
        let command = if next { "toros-layout next; toros-layout get" } else { "toros-layout get" };
        glib::spawn_future_local(clone!(
            #[strong(rename_to = lock)]
            self,
            async move {
                let code = gio::spawn_blocking(move || {
                    let said = Command::new("sh").args(["-c", command]).output().ok()?;
                    Some(String::from_utf8_lossy(&said.stdout).trim().to_string())
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                for screen in lock.screens.borrow().iter() {
                    screen.show_layout(&code);
                }
                lock.layout.replace(code);
            }
        ));
    }

    /// Once a second: the clock, now and then the battery, and back to the
    /// clock where nobody is typing.
    fn tick(&self) {
        let now = glib::DateTime::now_local().ok();
        let format = |how: &str| now.as_ref().and_then(|now| now.format(how).ok()).unwrap_or_default();
        let (time, date) = (format("%H:%M"), format("%A, %-e %B"));
        let battery = self.ticks.get().is_multiple_of(BATTERY_EVERY).then(battery);
        self.ticks.set(self.ticks.get().wrapping_add(1));
        for screen in self.screens.borrow().iter() {
            screen.show_time(&time, &date);
            if let Some(battery) = battery {
                screen.show_battery(battery);
            }
            if !screen.on_clock() && !screen.busy.get() && screen.last_input.get().elapsed() > BACK_TO_CLOCK {
                screen.show_clock();
            }
        }
    }

    /// The screen is locked and the lock screen is on it: tell the caller.
    fn drawn(&self) {
        let Some(ready) = self.ready.take() else { return };
        if ready >= 0 {
            unsafe {
                libc::write(ready, b"L".as_ptr() as *const libc::c_void, 1);
                libc::close(ready);
            }
        }
        if self.bench {
            // Not at once: labwc gives a lock screen the keyboard a moment
            // after its first picture, and if the lock is gone by then it
            // gives the keyboard to no later lock screen either.
            let instance = self.instance.clone();
            glib::timeout_add_local_once(BENCH_STAYS, move || instance.unlock());
        }
    }
}

pub fn run() -> ExitCode {
    // (the watcher starts this through /proc/self/exe, which would give it
    // the name "exe")
    unsafe { libc::prctl(libc::PR_SET_NAME, c"toros-lock".as_ptr()) };
    // Drawing in software starts faster and needs far less memory than
    // drawing with the GPU, and little here moves.
    env::set_var("GSK_RENDERER", env::var("TOROS_LOCK_RENDERER").unwrap_or("cairo".to_string()));
    // Started for a suspend it comes from root's hook and not from labwc,
    // whose environment (/etc/xdg/labwc/environment) it then lacks
    for (name, value) in [("GTK_A11Y", "none"), ("NO_AT_BRIDGE", "1"), ("GTK_IM_MODULE", "simple")] {
        if env::var_os(name).is_none() {
            env::set_var(name, value);
        }
    }
    if gtk::init().is_err() || !gtk4_session_lock::is_supported() {
        eprintln!("toros-lock: no Wayland compositor that can lock the session");
        return ExitCode::from(REFUSED);
    }

    gio::resources_register_include!("icons.gresource").expect("the icons are part of the program");
    let (colours, dark) = colours();
    if let Some(display) = gdk::Display::default() {
        let style = gtk::CssProvider::new();
        style.load_from_string(&(colours + include_str!("style.css")));
        gtk::style_context_add_provider_for_display(&display, &style, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let (login, name) = account();
    let lock = Rc::new(Lock {
        instance: Instance::new(),
        main_loop: glib::MainLoop::new(None, false),
        login,
        name,
        dark,
        screens: RefCell::default(),
        layout: RefCell::default(),
        // (-1: nobody waits, but the first picture is still told apart)
        ready: Cell::new(Some(env::var(READY_FD).ok().and_then(|fd| fd.parse().ok()).unwrap_or(-1))),
        locked: Cell::new(false),
        refused: Cell::new(false),
        ticks: Cell::new(0),
        bench: env::var_os("TOROS_LOCK_BENCH").is_some(),
    });

    lock.instance.connect_monitor(clone!(
        #[weak]
        lock,
        move |instance, monitor| {
            let screen = Screen::build(&lock, monitor);
            screen.show_layout(&lock.layout.borrow());
            instance.assign_window_to_monitor(&screen.window, monitor);
            lock.screens.borrow_mut().push(screen);
            lock.ticks.set(0);
            lock.tick();
        }
    ));
    lock.instance.connect_locked(clone!(
        #[weak]
        lock,
        move |_| {
            lock.locked.set(true);
            glib::timeout_add_local_once(
                DRAWN_WAIT,
                clone!(
                    #[weak]
                    lock,
                    move || lock.drawn()
                ),
            );
        }
    ));
    // Another program holds the lock, or the compositor gave none
    lock.instance.connect_failed(clone!(
        #[weak]
        lock,
        move |_| {
            lock.refused.set(true);
            lock.main_loop.quit();
        }
    ));
    lock.instance.connect_unlocked(clone!(
        #[weak]
        lock,
        move |_| lock.main_loop.quit()
    ));
    if !lock.instance.lock() {
        eprintln!("toros-lock: the session could not be locked");
        return ExitCode::from(REFUSED);
    }
    lock.layout(false);
    glib::timeout_add_seconds_local(
        1,
        clone!(
            #[weak]
            lock,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                lock.tick();
                glib::ControlFlow::Continue
            }
        ),
    );
    lock.main_loop.run();

    // the compositor has to hear "unlock" before this program is gone, or it
    // would keep the screen locked
    if let Some(display) = gdk::Display::default() {
        display.sync();
    }
    if lock.refused.get() {
        eprintln!("toros-lock: the session could not be locked (is it locked already?)");
        return ExitCode::from(REFUSED);
    }
    ExitCode::SUCCESS
}
