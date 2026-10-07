//! The keyboard on the lock screen, for typing the password with the pointer.
//! toros-keyboard cannot be there: a locked screen shows nothing of another
//! program. So the lock screen has the keys itself. They are the same ones,
//! Apple's rows for the keyboard layout in use and its two pages of digits
//! and signs (keyboard/src/layout.rs), with the same icons and colours; what
//! differs is that these stay where they are and hand what is typed to the
//! lock screen instead of to the compositor.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::layout;

/// The room for a row of keys: ten keys of 38 and the nine gaps between them.
const KEYS_WIDTH: i32 = 434;
const KEY_HEIGHT: i32 = 46;
const ROW_GAP: i32 = 12;
/// Shift, delete and "#+=", and what is at least left between them and the
/// letters.
const SIDE_KEY: i32 = 50;
const SIDE_GAP: i32 = 10;
/// "123" and return.
const WIDE_KEY: i32 = 104;
const ICON: i32 = 28;
/// Delete, held: when it begins to go on, and how fast.
const REPEAT_AFTER: Duration = Duration::from_millis(450);
const REPEAT_EVERY: Duration = Duration::from_millis(90);

const PAGES: [&str; 3] = ["letters", "numbers", "signs"];

#[derive(Clone, Copy, PartialEq)]
enum Shift {
    Off,
    /// for the next letter
    Once,
    /// until it is clicked again
    Lock,
}

/// What a key does.
#[derive(Clone, Copy)]
enum Action {
    /// a letter, as Shift makes it
    Letter(char),
    Sign(char),
    Shift,
    Delete,
    Return,
    Page(&'static str),
}

/// What the keys hand on.
#[derive(Clone, Copy)]
pub enum Typed {
    Char(char),
    Delete,
    Return,
}

type Listener = Box<dyn Fn(Typed)>;

pub struct Keys {
    pub panel: gtk::Box,
    pages: gtk::Stack,
    layout: RefCell<String>,
    /// the keys with letters, which Shift changes
    letters: RefCell<Vec<(gtk::Label, char)>>,
    shift_icon: RefCell<Option<gtk::Image>>,
    shift: Cell<Shift>,
    /// counts the keys pressed and let go: delete goes on while it stays
    held: Cell<u32>,
    /// who gets what is typed
    typed: RefCell<Option<Listener>>,
}

/// One of the keys' icons. They are symbolic icons: drawn in the text colour
/// of what they are on.
fn paintable(name: &str) -> gtk::IconPaintable {
    let file = gio::File::for_uri(&format!("resource:///org/toros/Lock/icons/{name}-symbolic.svg"));
    let scale = gdk::Display::default()
        .and_then(|d| d.monitors().iter::<gdk::Monitor>().flatten().map(|m| m.scale_factor()).max())
        .unwrap_or(1);
    gtk::IconPaintable::for_file(&file, ICON, scale)
}

fn icon(name: &str) -> gtk::Image {
    let image = gtk::Image::from_paintable(Some(&paintable(name)));
    image.set_pixel_size(ICON);
    image
}

fn column() -> gtk::Box {
    gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(ROW_GAP).build()
}

impl Keys {
    pub fn new() -> Rc<Self> {
        let pages = gtk::Stack::builder().hhomogeneous(true).vhomogeneous(true).build();
        let panel = gtk::Box::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .visible(false)
            .css_classes(["keys"])
            .build();
        panel.append(&pages);
        let keys = Rc::new(Keys {
            panel,
            pages,
            layout: RefCell::new("us".to_string()),
            letters: RefCell::default(),
            shift_icon: RefCell::new(None),
            shift: Cell::new(Shift::Off),
            held: Cell::new(0),
            typed: RefCell::new(None),
        });
        keys.fill();
        keys
    }

    /// `typed` is told every character, delete and return from now on.
    pub fn connect_typed(&self, typed: impl Fn(Typed) + 'static) {
        self.typed.replace(Some(Box::new(typed)));
    }

    /// Show the letters of this keyboard layout (us, tr), if it is another.
    pub fn set_layout(self: &Rc<Self>, layout: &str) {
        if layout.is_empty() || *self.layout.borrow() == layout {
            return;
        }
        self.layout.replace(layout.to_string());
        self.fill();
    }

    /// The first page and no Shift, as when it has not been used.
    pub fn reset(&self) {
        self.pages.set_visible_child_name(PAGES[0]);
        self.set_shift(Shift::Off);
    }

    /// Make the pages of keys, anew when the layout has changed.
    fn fill(self: &Rc<Self>) {
        let shown = self.pages.visible_child_name();
        for name in PAGES {
            if let Some(old) = self.pages.child_by_name(name) {
                self.pages.remove(&old);
            }
        }
        self.letters.borrow_mut().clear();
        let layout = self.layout.borrow().clone();
        self.pages.add_named(&self.letters_page(&layout), Some(PAGES[0]));
        self.pages.add_named(&self.signs_page(layout::numbers(&layout), "#+=", PAGES[2]), Some(PAGES[1]));
        self.pages.add_named(&self.signs_page(layout::signs(&layout), "123", PAGES[1]), Some(PAGES[2]));
        self.pages.set_visible_child_name(shown.as_deref().unwrap_or(PAGES[0]));
        self.set_shift(self.shift.get());
    }

    fn key(self: &Rc<Self>, face: &impl IsA<gtk::Widget>, width: i32, action: Action) -> gtk::Box {
        face.set_hexpand(true);
        face.set_halign(gtk::Align::Center);
        let key = gtk::Box::builder().width_request(width).height_request(KEY_HEIGHT).css_classes(["key"]).build();
        key.append(face);
        // (the face is in the middle of its key; the key keeps its width)
        key.set_hexpand(false);
        // A key acts when it is pressed, not when it is let go.
        let click = gtk::GestureClick::new();
        click.connect_pressed(clone!(
            #[weak(rename_to = keys)]
            self,
            #[weak]
            key,
            move |click, presses, _, _| {
                click.set_state(gtk::EventSequenceState::Claimed);
                key.set_state_flags(gtk::StateFlags::ACTIVE, false);
                keys.press(action, presses);
            }
        ));
        let up = clone!(
            #[weak(rename_to = keys)]
            self,
            #[weak]
            key,
            move || {
                key.unset_state_flags(gtk::StateFlags::ACTIVE);
                keys.held.set(keys.held.get().wrapping_add(1));
            }
        );
        click.connect_released({
            let up = up.clone();
            move |_, _, _, _| up()
        });
        click.connect_cancel(move |_, _| up());
        key.add_controller(click);
        key
    }

    fn word_key(self: &Rc<Self>, word: &str, width: i32, action: Action) -> gtk::Box {
        let key = self.key(&gtk::Label::new(Some(word)), width, action);
        key.add_css_class("word");
        key
    }

    /// A row of keys with a character each.
    fn row(self: &Rc<Self>, characters: &str, width: i32, gap: i32, letters: bool) -> gtk::Box {
        let row = gtk::Box::builder().spacing(gap).halign(gtk::Align::Center).build();
        for c in characters.chars() {
            let label = gtk::Label::new(Some(&c.to_string()));
            if letters {
                self.letters.borrow_mut().push((label.clone(), c));
            }
            row.append(&self.key(&label, width, if letters { Action::Letter(c) } else { Action::Sign(c) }));
        }
        row
    }

    /// The third row: a key at each end and the characters between them.
    fn ends(left: &gtk::Box, middle: &gtk::Box, right: &gtk::Box) -> gtk::CenterBox {
        gtk::CenterBox::builder().start_widget(left).center_widget(middle).end_widget(right).build()
    }

    /// The last row: to another page, the space bar, return.
    fn bottom(self: &Rc<Self>, word: &str, page: &'static str) -> gtk::Box {
        let space = self.key(&gtk::Label::new(None), -1, Action::Sign(' '));
        space.set_hexpand(true);
        let row = gtk::Box::builder().spacing(6).build();
        row.append(&self.word_key(word, WIDE_KEY, Action::Page(page)));
        row.append(&space);
        row.append(&self.key(&icon("return"), WIDE_KEY, Action::Return));
        row
    }

    fn letters_page(self: &Rc<Self>, layout: &str) -> gtk::Box {
        let rows = layout::letters(layout);
        // Ten letters in a row are Apple's keys; more of them are narrower
        let most = rows.iter().map(|row| row.chars().count()).max().unwrap_or(10) as i32;
        let gap = if most > 10 { 5 } else { 6 };
        let width = (KEYS_WIDTH - (most - 1) * gap) / most;
        let third = rows[2].chars().count() as i32;
        let side = ((KEYS_WIDTH - third * width - (third - 1) * gap) / 2 - SIDE_GAP).min(SIDE_KEY);

        let shift_icon = icon("shift");
        let shift = self.key(&shift_icon, side, Action::Shift);
        self.shift_icon.replace(Some(shift_icon));
        let delete = self.key(&icon("backspace"), side, Action::Delete);

        let page = column();
        page.append(&self.row(rows[0], width, gap, true));
        page.append(&self.row(rows[1], width, gap, true));
        page.append(&Self::ends(&shift, &self.row(rows[2], width, gap, true), &delete));
        page.append(&self.bottom("123", PAGES[1]));
        page
    }

    /// A page of digits and signs; the key where Shift is goes to the other.
    fn signs_page(self: &Rc<Self>, rows: [&str; 3], other: &str, other_page: &'static str) -> gtk::Box {
        let turn = self.word_key(other, SIDE_KEY, Action::Page(other_page));
        let delete = self.key(&icon("backspace"), SIDE_KEY, Action::Delete);
        let page = column();
        page.append(&self.row(rows[0], 38, 6, false));
        page.append(&self.row(rows[1], 38, 6, false));
        page.append(&Self::ends(&turn, &self.row(rows[2], 54, 8, false), &delete));
        page.append(&self.bottom("ABC", PAGES[0]));
        page
    }

    fn tell(&self, typed: Typed) {
        if let Some(typed_to) = self.typed.borrow().as_ref() {
            typed_to(typed);
        }
    }

    fn press(self: &Rc<Self>, action: Action, presses: i32) {
        match action {
            Action::Letter(letter) => {
                let shift = self.shift.get();
                let capital = layout::upper(letter, &self.layout.borrow());
                self.tell(Typed::Char(if shift == Shift::Off { letter } else { capital }));
                if shift == Shift::Once {
                    self.set_shift(Shift::Off);
                }
            }
            Action::Sign(sign) => self.tell(Typed::Char(sign)),
            Action::Return => self.tell(Typed::Return),
            Action::Delete => {
                self.tell(Typed::Delete);
                self.go_on_deleting();
            }
            Action::Shift => self.set_shift(match self.shift.get() {
                // the second of two quick clicks
                _ if presses % 2 == 0 => Shift::Lock,
                Shift::Off => Shift::Once,
                Shift::Once | Shift::Lock => Shift::Off,
            }),
            Action::Page(page) => self.pages.set_visible_child_name(page),
        }
    }

    /// Delete was pressed: for as long as no key is let go, it goes on.
    fn go_on_deleting(self: &Rc<Self>) {
        let held = self.held.get();
        glib::timeout_add_local_once(
            REPEAT_AFTER,
            clone!(
                #[weak(rename_to = keys)]
                self,
                move || {
                    if keys.held.get() != held {
                        return;
                    }
                    keys.tell(Typed::Delete);
                    glib::timeout_add_local(
                        REPEAT_EVERY,
                        clone!(
                            #[weak]
                            keys,
                            #[upgrade_or]
                            glib::ControlFlow::Break,
                            move || {
                                if keys.held.get() != held {
                                    return glib::ControlFlow::Break;
                                }
                                keys.tell(Typed::Delete);
                                glib::ControlFlow::Continue
                            }
                        ),
                    );
                }
            ),
        );
    }

    fn set_shift(&self, shift: Shift) {
        self.shift.set(shift);
        let layout = self.layout.borrow();
        for (label, letter) in self.letters.borrow().iter() {
            let shown = if shift == Shift::Off { *letter } else { layout::upper(*letter, &layout) };
            label.set_label(&shown.to_string());
        }
        if let Some(image) = self.shift_icon.borrow().as_ref() {
            image.set_paintable(Some(&paintable(match shift {
                Shift::Off => "shift",
                Shift::Once => "shift-on",
                Shift::Lock => "caps",
            })));
        }
    }
}
