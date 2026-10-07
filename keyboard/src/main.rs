//! toros-keyboard: a keyboard on the screen for the pointer, after Apple's.
//!
//!   toros-keyboard    show the keyboard (Super+Ctrl+O as on Windows, and
//!                     "On-screen keyboard" in the overview). Run again, it
//!                     closes; if it was put away at the edge of the screen,
//!                     it comes back out instead.
//!
//! A click on a key types into the window that has the keys: the keyboard
//! never takes them itself. It shows the letters of the keyboard layout in
//! use (toros-layout), and Apple's two pages of digits and signs behind
//! "123". Shift is for one letter; clicked twice, for all until it is clicked
//! again. Delete goes on for as long as it is held.
//!
//! It floats above everything and is moved by its edge or the bar under the
//! keys. Thrown or dragged over the left or right edge of the screen, or
//! swiped there with two fingers, it slides out and leaves a tab with an
//! arrow there, as a video on an iPhone does; a click on the tab brings it
//! back. The overview, which covers the screen when it opens, has the keyboard
//! come above it, so its search can be typed into.

mod layout;
mod typing;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{env, fs};

use adw::prelude::*;
use gtk::glib::clone;
use gtk::{cairo, gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use typing::{Key, Typist};

const APP_ID: &str = "org.toros.Keyboard";
const SYSTEM_ENVIRONMENT: &str = "/etc/xdg/labwc/environment";

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

/// Where the keyboard first is: in the middle, this far from the bottom of
/// the screen (above the dock).
const REST: f64 = 84.0;
/// The nearest it stays to an edge of the screen.
const EDGE: f64 = 8.0;
/// How far out of the screen it goes: its shadow goes along.
const SHADOW: f64 = 40.0;
const TAB: (f64, f64) = (22.0, 96.0);
const TAB_ARROW: i32 = 20;
/// The tab comes in over the last part of the keyboard's way out.
const TAB_FROM: f64 = 0.6;
/// Let go with this much of its width over the edge, the keyboard goes away.
const OVER: f64 = 0.3;
/// A throw counts as where the keyboard would be this much later.
const THROW: f64 = 0.15;
/// The pointer's speed is taken from this long before it let go.
const TRAIL: Duration = Duration::from_millis(100);
/// Two fingers have to go this far sideways, and a pause this long begins a
/// new swipe.
const SWIPE: f64 = 70.0;
const SWIPE_PAUSE: Duration = Duration::from_millis(300);
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

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
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

/// Where the two things on the screen are.
#[derive(Clone, PartialEq)]
struct Places {
    panel: gdk::Rectangle,
    tab: gdk::Rectangle,
    /// how much of the tab is there, 0 to 1
    tab_shown: f64,
}

struct Ui {
    app: adw::Application,
    window: gtk::ApplicationWindow,
    /// the whole screen; the keyboard and the tab lie on it
    stage: gtk::Overlay,
    panel: gtk::Box,
    pages: gtk::Stack,
    tab: adw::Bin,
    arrow: gtk::Image,
    typist: RefCell<Typist>,
    layout: RefCell<String>,
    /// the keys with letters, which Shift changes
    letters: RefCell<Vec<(gtk::Label, char)>>,
    shift_icon: RefCell<Option<gtk::Image>>,
    shift: Cell<Shift>,
    /// counts the keys pressed and let go: delete goes on while it stays
    held: Cell<u32>,
    size: Cell<(f64, f64)>,
    /// the keyboard's top left corner while it is out
    home: Cell<Option<(f64, f64)>>,
    /// how far it is on its way out: 0 here, 1 gone with the tab in its place
    away: Cell<f64>,
    side: Cell<Side>,
    /// where `place` last put things
    placed: RefCell<Option<Places>>,
    /// put away, or on its way there
    stashed: Cell<bool>,
    motion: RefCell<Option<adw::SpringAnimation>>,
    /// where `home` was when the pointer took hold of the keyboard
    drag: Cell<Option<(f64, f64)>>,
    /// where the keyboard was a moment ago, for the speed of a throw
    trail: RefCell<VecDeque<(Instant, f64)>>,
    /// how far two fingers have swiped, when they last did, and whether that
    /// has been acted on
    swipe: Cell<(f64, Option<Instant>, bool)>,
    watch: Option<gio::FileMonitor>,
}

fn config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"))
}

/// The keyboard layout in use: the first of labwc's list, which toros-layout
/// writes anew to change it.
fn layout_in_use() -> String {
    let first = |file: &Path| {
        let text = fs::read_to_string(file).ok()?;
        let list = text.lines().rev().find_map(|line| line.strip_prefix("XKB_DEFAULT_LAYOUT="))?;
        list.split(',').next().map(|code| code.trim().to_string()).filter(|code| !code.is_empty())
    };
    first(&config_dir().join("labwc/environment"))
        .or_else(|| first(Path::new(SYSTEM_ENVIRONMENT)))
        .unwrap_or_else(|| "us".to_string())
}

/// One of the program's own icons. They are symbolic icons: drawn in the text
/// colour of what they are on.
fn paintable(name: &str) -> gtk::IconPaintable {
    let file = gio::File::for_uri(&format!("resource:///org/toros/Keyboard/icons/{name}-symbolic.svg"));
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

impl Ui {
    fn build(app: &adw::Application, typist: Typist, layout: String) -> Rc<Ui> {
        let pages = gtk::Stack::builder().hhomogeneous(true).vhomogeneous(true).build();
        let grabber = gtk::Box::builder().css_classes(["grabber"]).halign(gtk::Align::Center).build();
        let panel = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["panel"]).build();
        panel.append(&pages);
        panel.append(&grabber);

        let arrow = icon("pull-left");
        arrow.set_pixel_size(TAB_ARROW);
        let tab = adw::Bin::builder().child(&arrow).css_classes(["tab", "on-right"]).opacity(0.0).build();

        // (nothing is drawn on it: it is as large as the screen and says so)
        let floor = gtk::DrawingArea::new();
        let stage = gtk::Overlay::builder().child(&floor).build();
        stage.add_overlay(&panel);
        stage.add_overlay(&tab);

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Keyboard")
            .decorated(false)
            .css_classes(["keyboard"])
            .child(&stage)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some("toros-keyboard"));
        window.set_layer(Layer::Overlay);
        // the whole screen, dock and status panel included: the tab is at the
        // very edge. Clicks only reach it where the keyboard or the tab is
        // (see `place`).
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        // the keys stay with the window that is typed into
        window.set_keyboard_mode(KeyboardMode::None);

        // read again when toros-layout changes the layout
        let own = gio::File::for_path(config_dir().join("labwc/environment"));
        let watch = own.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE).ok();

        let ui = Rc::new(Ui {
            app: app.clone(),
            window,
            stage,
            panel,
            pages,
            tab,
            arrow,
            typist: RefCell::new(typist),
            layout: RefCell::new(layout),
            letters: RefCell::default(),
            shift_icon: RefCell::default(),
            shift: Cell::new(Shift::Off),
            held: Cell::new(0),
            size: Cell::new((0.0, 0.0)),
            home: Cell::new(None),
            away: Cell::new(0.0),
            side: Cell::new(Side::Right),
            placed: RefCell::default(),
            stashed: Cell::new(false),
            motion: RefCell::default(),
            drag: Cell::new(None),
            trail: RefCell::default(),
            swipe: Cell::new((0.0, None, false)),
            watch,
        });
        ui.fill();

        let style = adw::StyleManager::default();
        let dress = clone!(
            #[weak]
            ui,
            move |style: &adw::StyleManager| {
                if style.is_dark() {
                    ui.window.add_css_class("dark");
                } else {
                    ui.window.remove_css_class("dark");
                }
            }
        );
        dress(&style);
        style.connect_dark_notify(dress);

        if let Some(watch) = &ui.watch {
            watch.connect_changed(clone!(
                #[weak]
                ui,
                move |_, _, _, _| ui.read_layout()
            ));
        }

        // until the keyboard has its place, no click is the window's
        ui.window.connect_realize(|window| {
            if let Some(surface) = window.surface() {
                surface.set_input_region(Some(&cairo::Region::create()));
            }
        });
        floor.connect_resize(clone!(
            #[weak]
            ui,
            move |_, width, height| {
                ui.size.set((f64::from(width), f64::from(height)));
                if ui.motion.borrow().is_none() && ui.drag.get().is_none() {
                    ui.home.set(Some(ui.kept(ui.home())));
                }
                // (the screen is being laid out right now, and what is on it
                // is put in its place next; the clicks go with this picture)
                ui.claim();
            }
        ));
        ui.stage.connect_get_child_position(clone!(
            #[weak]
            ui,
            #[upgrade_or]
            None,
            move |_, child| {
                let places = ui.places();
                Some(if child == ui.tab.upcast_ref::<gtk::Widget>() { places.tab } else { places.panel })
            }
        ));

        // Moving it: by anything of it that is not a key (a key keeps the
        // click that begins on it)
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(clone!(
            #[weak]
            ui,
            move |drag, x, y| {
                let state = if ui.grab(x, y) { gtk::EventSequenceState::Claimed } else { gtk::EventSequenceState::Denied };
                drag.set_state(state);
            }
        ));
        drag.connect_drag_update(clone!(
            #[weak]
            ui,
            move |_, dx, dy| ui.drag_to(dx, dy)
        ));
        drag.connect_drag_end(clone!(
            #[weak]
            ui,
            move |_, _, _| ui.let_go()
        ));
        ui.stage.add_controller(drag);

        // Two fingers sideways on the touchpad put it away on that side, and
        // on the tab bring it back. (A mouse wheel that tilts does the same.)
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::HORIZONTAL);
        scroll.connect_scroll(clone!(
            #[weak]
            ui,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |scroll, dx, _| {
                ui.swiped(if scroll.unit() == gdk::ScrollUnit::Wheel { dx * (SWIPE + 1.0) } else { dx });
                glib::Propagation::Stop
            }
        ));
        ui.stage.add_controller(scroll);

        let click = gtk::GestureClick::new();
        click.connect_pressed(|click, _, _, _| {
            click.set_state(gtk::EventSequenceState::Claimed);
        });
        click.connect_released(clone!(
            #[weak]
            ui,
            move |_, _, _, _| {
                if ui.stashed.get() {
                    ui.come_back();
                }
            }
        ));
        ui.tab.add_controller(click);

        ui
    }

    // ---- the keys -------------------------------------------------------

    /// Make the three pages for the layout in use.
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

    /// Look which layout is in use, and show its letters if it is another.
    fn read_layout(self: &Rc<Self>) {
        let layout = layout_in_use();
        if *self.layout.borrow() == layout {
            return;
        }
        self.typist.borrow_mut().set_layout(&layout);
        self.layout.replace(layout);
        self.fill();
    }

    fn key(self: &Rc<Self>, face: &impl IsA<gtk::Widget>, width: i32, action: Action) -> adw::Bin {
        let key = adw::Bin::builder()
            .child(face)
            .width_request(width)
            .height_request(KEY_HEIGHT)
            .css_classes(["key"])
            .build();
        // A key acts when it is pressed, not when it is let go.
        let click = gtk::GestureClick::new();
        click.connect_pressed(clone!(
            #[weak(rename_to = ui)]
            self,
            #[weak]
            key,
            move |click, presses, _, _| {
                click.set_state(gtk::EventSequenceState::Claimed);
                key.set_state_flags(gtk::StateFlags::ACTIVE, false);
                ui.press(action, presses);
            }
        ));
        let up = clone!(
            #[weak(rename_to = ui)]
            self,
            #[weak]
            key,
            move || {
                key.unset_state_flags(gtk::StateFlags::ACTIVE);
                ui.held.set(ui.held.get().wrapping_add(1));
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

    fn word_key(self: &Rc<Self>, word: &str, width: i32, action: Action) -> adw::Bin {
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
    fn ends(left: &adw::Bin, middle: &gtk::Box, right: &adw::Bin) -> gtk::CenterBox {
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

    fn tap(&self, key: Key) {
        self.typist.borrow_mut().tap(key);
    }

    fn press(self: &Rc<Self>, action: Action, presses: i32) {
        match action {
            Action::Letter(letter) => {
                let shift = self.shift.get();
                let capital = layout::upper(letter, &self.layout.borrow());
                self.tap(Key::Char(if shift == Shift::Off { letter } else { capital }));
                if shift == Shift::Once {
                    self.set_shift(Shift::Off);
                }
            }
            Action::Sign(sign) => self.tap(Key::Char(sign)),
            Action::Return => self.tap(Key::Return),
            Action::Delete => {
                self.tap(Key::BackSpace);
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
                #[weak(rename_to = ui)]
                self,
                move || {
                    if ui.held.get() != held {
                        return;
                    }
                    ui.tap(Key::BackSpace);
                    glib::timeout_add_local(
                        REPEAT_EVERY,
                        clone!(
                            #[weak]
                            ui,
                            #[upgrade_or]
                            glib::ControlFlow::Break,
                            move || {
                                if ui.held.get() != held {
                                    return glib::ControlFlow::Break;
                                }
                                ui.tap(Key::BackSpace);
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

    // ---- where it is ----------------------------------------------------

    fn panel_size(&self) -> (f64, f64) {
        let width = self.panel.measure(gtk::Orientation::Horizontal, -1).1;
        let height = self.panel.measure(gtk::Orientation::Vertical, -1).1;
        (f64::from(width), f64::from(height))
    }

    fn home(&self) -> (f64, f64) {
        self.home.get().unwrap_or_else(|| {
            let (screen, panel) = (self.size.get(), self.panel_size());
            self.kept(((screen.0 - panel.0) / 2.0, screen.1 - panel.1 - REST))
        })
    }

    /// The place nearest to this one that is all on the screen.
    fn kept(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (screen, panel) = (self.size.get(), self.panel_size());
        let most = |room: f64| (room - EDGE).max(EDGE);
        (x.clamp(EDGE, most(screen.0 - panel.0)).round(), y.clamp(EDGE, most(screen.1 - panel.1)).round())
    }

    fn places(&self) -> Places {
        let (screen, panel) = (self.size.get(), self.panel_size());
        let home = self.home();
        let away = self.away.get();
        let right = self.side.get() == Side::Right;
        let x = home.0 + (self.gone() - home.0) * away;
        let tab_shown = ((away - TAB_FROM) / (1.0 - TAB_FROM)).clamp(0.0, 1.0);
        let tab_x = if right { screen.0 - TAB.0 * tab_shown } else { TAB.0 * (tab_shown - 1.0) };
        let tab_y = (home.1 + (panel.1 - TAB.1) / 2.0).clamp(0.0, (screen.1 - TAB.1).max(0.0));
        let whole = |x: f64, y: f64, size: (f64, f64)| {
            gdk::Rectangle::new(x.round() as i32, y.round() as i32, size.0 as i32, size.1 as i32)
        };
        Places { panel: whole(x, home.1, panel), tab: whole(tab_x, tab_y, TAB), tab_shown }
    }

    /// Put the keyboard and the tab where they are now.
    fn place(&self) {
        if self.claim() {
            self.stage.queue_allocate();
        }
    }

    /// Take the clicks where the keyboard and the tab are now; those beside
    /// them go to what is under the window. Says whether anything has moved
    /// (a spring is at rest long before it says so).
    fn claim(&self) -> bool {
        let places = self.places();
        if self.placed.replace(Some(places.clone())).as_ref() == Some(&places) {
            return false;
        }
        self.tab.set_opacity(places.tab_shown);
        if let Some(surface) = self.window.surface() {
            let rect = |r: &gdk::Rectangle| cairo::RectangleInt::new(r.x(), r.y(), r.width(), r.height());
            let mut taken = vec![rect(&places.panel)];
            if places.tab_shown > 0.0 {
                taken.push(rect(&places.tab));
            }
            surface.set_input_region(Some(&cairo::Region::create_rectangles(&taken)));
        }
        true
    }

    /// Go from where it is to another place, or away, or back, the way a
    /// spring would take it there. `speed` is the one it has already, in
    /// screen units a second to the right, from the hand that let it go.
    fn glide(self: &Rc<Self>, home: (f64, f64), away: f64, speed: f64) {
        if let Some(old) = self.motion.take() {
            old.pause();
        }
        let from = (self.home(), self.away.get());
        let target = adw::CallbackAnimationTarget::new(clone!(
            #[weak(rename_to = ui)]
            self,
            move |done| {
                let between = |a: f64, b: f64| a + (b - a) * done;
                ui.home.set(Some((between(from.0 .0, home.0), between(from.0 .1, home.1))));
                ui.away.set(between(from.1, away));
                ui.place();
            }
        ));
        let spring = adw::SpringAnimation::new(&self.stage, 0.0, 1.0, adw::SpringParams::new(0.86, 1.0, 240.0), target);
        spring.set_epsilon(0.001);
        // (the spring counts in parts of the way it has to go, not in screen units)
        let way = if away != from.1 {
            (self.gone() - from.0 .0) * (away - from.1)
        } else {
            home.0 - from.0 .0
        };
        if way.abs() > 1.0 {
            spring.set_initial_velocity((speed / way).clamp(0.0, 12.0));
        }
        spring.connect_done(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                ui.motion.replace(None);
            }
        ));
        spring.play();
        self.motion.replace(Some(spring));
    }

    /// Where the keyboard's left edge is when it is away.
    fn gone(&self) -> f64 {
        match self.side.get() {
            Side::Right => self.size.get().0 + SHADOW,
            Side::Left => -self.panel_size().0 - SHADOW,
        }
    }

    /// Come to lie above what was put on the screen since: windows of this
    /// kind are stacked in the order they came, and the overview, which comes
    /// later, is typed into with this keyboard. The window is taken off the
    /// screen and put back.
    fn raise(&self) {
        self.window.set_visible(false);
        self.window.present();
        // the clicks beside the keyboard are not the new window's either
        self.placed.replace(None);
        self.claim();
    }

    /// Slide out over an edge of the screen and leave the tab there.
    fn put_away(self: &Rc<Self>, side: Side, speed: f64) {
        self.held.set(self.held.get().wrapping_add(1));
        self.side.set(side);
        self.stashed.set(true);
        let (class, other, arrow) = match side {
            Side::Right => ("on-right", "on-left", "pull-left"),
            Side::Left => ("on-left", "on-right", "pull-right"),
        };
        self.tab.remove_css_class(other);
        self.tab.add_css_class(class);
        self.arrow.set_paintable(Some(&paintable(arrow)));
        self.glide(self.kept(self.home()), 1.0, speed);
    }

    fn come_back(self: &Rc<Self>) {
        self.stashed.set(false);
        self.read_layout();
        self.glide(self.kept(self.home()), 0.0, 0.0);
    }

    /// The pointer's button went down at this point of the screen: does it
    /// take hold of the keyboard?
    fn grab(&self, x: f64, y: f64) -> bool {
        if self.stashed.get() || !self.places().panel.contains_point(x as i32, y as i32) {
            return false;
        }
        if let Some(motion) = self.motion.take() {
            motion.skip();
        }
        self.drag.set(Some(self.home()));
        self.trail.borrow_mut().clear();
        true
    }

    fn drag_to(&self, dx: f64, dy: f64) {
        let Some(start) = self.drag.get() else { return };
        // sideways it may leave the screen, up and down it may not
        let x = start.0 + dx;
        self.home.set(Some((x, self.kept((x, start.1 + dy)).1)));
        let now = Instant::now();
        let mut trail = self.trail.borrow_mut();
        trail.push_back((now, x));
        while trail.front().is_some_and(|(then, _)| now.duration_since(*then) > TRAIL) {
            trail.pop_front();
        }
        drop(trail);
        self.place();
    }

    /// Let go: over an edge (or thrown there) it goes away, anywhere else it
    /// stays, all on the screen.
    fn let_go(self: &Rc<Self>) {
        if self.drag.take().is_none() {
            return;
        }
        let (screen, panel) = (self.size.get(), self.panel_size());
        let home = self.home();
        // (a keyboard that was held still before it was let go has no speed)
        let speed = {
            let trail = self.trail.borrow();
            match (trail.front(), trail.back()) {
                (Some((then, from)), Some((last, to)))
                    if last.elapsed() < TRAIL && last.duration_since(*then) > Duration::from_millis(10) =>
                {
                    (to - from) / last.duration_since(*then).as_secs_f64()
                }
                _ => 0.0,
            }
        };
        let thrown = home.0 + speed * THROW;
        if thrown + panel.0 - screen.0 > panel.0 * OVER {
            self.put_away(Side::Right, speed);
        } else if -thrown > panel.0 * OVER {
            self.put_away(Side::Left, speed);
        } else {
            self.glide(self.kept(home), 0.0, 0.0);
        }
    }

    /// Two fingers moved sideways by this much.
    fn swiped(self: &Rc<Self>, dx: f64) {
        let now = Instant::now();
        let (mut far, last, mut done) = self.swipe.get();
        if last.is_none_or(|last| now.duration_since(last) > SWIPE_PAUSE) {
            (far, done) = (0.0, false);
        }
        far += dx;
        if !done && far.abs() > SWIPE && self.drag.get().is_none() {
            done = true;
            let to = if far > 0.0 { Side::Right } else { Side::Left };
            if !self.stashed.get() {
                self.put_away(to, 0.0);
            } else if to != self.side.get() {
                self.come_back();
            }
        }
        self.swipe.set((far, Some(now), done));
    }
}

fn main() -> glib::ExitCode {
    gio::resources_register_include!("icons.gresource").expect("the icons are part of the program");
    // The keyboard moves, so it is drawn the way the session draws GTK apps
    // (on the graphics chip), not in software as torOS's other small programs
    // are.
    // TOROS_KEYBOARD_RENDERER=cairo tries that.
    if let Ok(renderer) = env::var("TOROS_KEYBOARD_RENDERER") {
        env::set_var("GSK_RENDERER", renderer);
    }

    let app = adw::Application::builder().application_id(APP_ID).build();
    let open: Rc<RefCell<Option<Rc<Ui>>>> = Rc::default();
    // What the overview asks for when it opens ("gapplication action
    // org.toros.Keyboard raise" from a terminal)
    let raise = gio::SimpleAction::new("raise", None);
    raise.connect_activate(clone!(
        #[strong]
        open,
        move |_, _| {
            if let Some(ui) = open.borrow().as_ref() {
                ui.raise();
            }
        }
    ));
    app.add_action(&raise);
    // Runs in the keyboard that is already there when the command is run again
    app.connect_activate(move |app| {
        let ui = open.borrow().clone();
        match ui {
            Some(ui) if ui.stashed.get() => ui.come_back(),
            Some(ui) => ui.app.quit(),
            None => {
                let layout = layout_in_use();
                let Some(typist) = Typist::connect(&layout) else {
                    eprintln!("toros-keyboard: the compositor does not let a program be a keyboard");
                    return;
                };
                let provider = gtk::CssProvider::new();
                provider.load_from_string(include_str!("style.css"));
                if let Some(display) = gdk::Display::default() {
                    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
                }
                let ui = Ui::build(app, typist, layout);
                ui.window.present();
                // for timing the start (see /usr/lib/toros/selftest): leave as
                // soon as the keyboard is drawn
                if env::var_os("TOROS_KEYBOARD_BENCH").is_some() {
                    if let Some(clock) = ui.window.frame_clock() {
                        let app = app.clone();
                        clock.connect_after_paint(move |_| app.quit());
                    }
                }
                open.replace(Some(ui));
            }
        }
    });
    app.run()
}
