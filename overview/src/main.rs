//! toros-overview: what the Super key opens, after GNOME's overview.
//!
//!   toros-overview    the open windows as small pictures, under them the
//!                     applications, and a search field that is typed into
//!                     at once. Run again while it is open, it closes.
//!
//! Typing narrows both lists down; Enter opens the first, the arrow keys go
//! to another. A click on a window's picture brings that window to the front,
//! its close button (or the middle button) closes it; Esc empties the search,
//! then closes the overview, and so does a click beside the tiles.
//!
//! It lies where the windows are and leaves the dock and the status panel
//! alone. The program starts when asked and ends when something was chosen,
//! so it uses no memory in between.

mod apps;
mod windows;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{env, fs};

use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use apps::App;
use toros_overview::search::Words;
use toros_overview::thumb;
use windows::{Change, Window, Windows};

const APP_ID: &str = "org.toros.Overview";
const PALETTE: &str = "/usr/share/toros/palette";
/// The size of an application's icon, and of the one on a window's picture.
const APP_ICON: i32 = 64;
const WINDOW_ICON: i32 = 32;
/// The room a window's picture gets.
const SHOT: (i32, i32) = (232, 145);
const APPS_PER_ROW: u32 = 7;
const WINDOWS_PER_ROW: u32 = 4;
/// How often the compositor is asked what happened to the windows.
const WATCH: Duration = Duration::from_millis(100);
/// After a window was closed from here another one comes to the front by
/// itself: for this long that is not taken as the user having gone elsewhere.
const SETTLE: Duration = Duration::from_millis(600);
/// An application that does not say it has started is not waited for longer.
const LAUNCH_WAIT: Duration = Duration::from_secs(10);
/// grim is given this long for a window's picture (it waits for ever for a
/// window that is not drawn, on a screen that is switched off for one).
const GRIM_SECONDS: &str = "2";

/// A tile, as an index into `Ui::wins` or `Ui::apps`.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Window(usize),
    App(usize),
}

struct WinTile {
    window: Window,
    words: Words,
    cell: gtk::FlowBoxChild,
    button: gtk::Button,
}

struct AppTile {
    app: App,
    cell: gtk::FlowBoxChild,
    button: gtk::Button,
}

struct Ui {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    entry: gtk::Entry,
    scroll: gtk::ScrolledWindow,
    content: gtk::Box,
    win_box: gtk::FlowBox,
    app_box: gtk::FlowBox,
    nothing: gtk::Label,
    wins: RefCell<Vec<WinTile>>,
    apps: RefCell<Vec<AppTile>>,
    /// the tiles that fit what is typed, in the order they are shown
    shown: RefCell<Vec<Target>>,
    /// the one Enter opens, as an index into `shown`
    selected: Cell<Option<usize>>,
    toplevels: RefCell<Option<Windows>>,
    settle: Cell<Option<Instant>>,
    was_active: Cell<bool>,
    filled: Cell<bool>,
    closing: Cell<bool>,
    /// what GSK_RENDERER was before the overview chose its own
    renderer: Option<String>,
}

/// The colours the style sheet uses. On torOS they come from the palette that
/// the rest of the desktop is coloured from, light or dark as toros-theme last
/// set it; anywhere else from the GTK theme.
fn colours() -> String {
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
    [
        ("window", "bg", "@theme_bg_color"),
        ("surface", "popover", "@theme_base_color"),
        ("field", "popover", "@theme_base_color"),
        ("fg", "fg", "@theme_fg_color"),
        ("dim", "dim", "alpha(@theme_fg_color, 0.55)"),
        ("border", "border", "@borders"),
        ("accent", "accent", "@theme_selected_bg_color"),
    ]
    .iter()
    .map(|(name, key, fallback)| {
        format!("@define-color ov_{name} {};\n", from_palette(key).unwrap_or(fallback.to_string()))
    })
    .collect()
}

/// One of the program's own icons. They are symbolic icons: drawn in the text
/// colour of what they are in.
fn own_icon(name: &str) -> gtk::Image {
    let file = gio::File::for_uri(&format!("resource:///org/toros/Overview/icons/{name}-symbolic.svg"));
    let scale = gdk::Display::default()
        .and_then(|d| d.monitors().iter::<gdk::Monitor>().flatten().map(|m| m.scale_factor()).max())
        .unwrap_or(1);
    let image = gtk::Image::from_paintable(Some(&gtk::IconPaintable::for_file(&file, 16, scale)));
    image.set_pixel_size(16);
    image
}

fn app_icon(app: Option<&App>, size: i32) -> gtk::Image {
    let image = match app.and_then(|app| app.info.icon()) {
        Some(icon) => gtk::Image::from_gicon(&icon),
        None => gtk::Image::from_icon_name("application-x-executable"),
    };
    image.set_pixel_size(size);
    image
}

fn tiles(per_row: u32, class: &str) -> gtk::FlowBox {
    gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(1)
        .max_children_per_line(per_row)
        .column_spacing(4)
        .row_spacing(4)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Start)
        .can_focus(false)
        .css_classes([class])
        .build()
}

fn cell(child: &impl IsA<gtk::Widget>) -> gtk::FlowBoxChild {
    gtk::FlowBoxChild::builder().child(child).can_focus(false).build()
}

/// Take a window's picture, cut it down to the window and make it small. Runs
/// on a thread of its own.
fn shoot(capture: &str, width: usize, height: usize) -> Option<thumb::Rgb> {
    let grim = Command::new("timeout")
        .args([GRIM_SECONDS, "grim", "-T", capture, "-t", "ppm", "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let picture = thumb::parse_ppm(&grim.stdout)?;
    let part = thumb::window_box(&picture);
    let (width, height) = thumb::fit(part.2, part.3, width, height);
    Some(thumb::shrink(&picture, part, width, height))
}

impl Ui {
    /// The empty overview: the sheet and its search field. The tiles come
    /// with `fill`, once this much is on the screen and takes the keys.
    fn build(app: &gtk::Application, renderer: Option<String>) -> Rc<Self> {
        if let Some(display) = gdk::Display::default() {
            let style = gtk::CssProvider::new();
            style.load_from_string(&(colours() + include_str!("style.css")));
            gtk::style_context_add_provider_for_display(&display, &style, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }

        let entry = gtk::Entry::builder().placeholder_text("Type to search").css_classes(["search"]).build();
        let field = gtk::Box::builder().css_classes(["field"]).halign(gtk::Align::Center).build();
        field.append(&own_icon("search"));
        field.append(&entry);

        let win_box = tiles(WINDOWS_PER_ROW, "windows");
        let app_box = tiles(APPS_PER_ROW, "apps");
        let nothing = gtk::Label::builder().label("No results").css_classes(["nothing"]).visible(false).build();
        let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        content.append(&win_box);
        content.append(&app_box);
        content.append(&nothing);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .can_focus(false)
            .child(&content)
            .build();

        let sheet = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["sheet"]).build();
        sheet.append(&field);
        sheet.append(&scroll);

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Overview")
            .decorated(false)
            .css_classes(["overview"])
            .child(&sheet)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some("toros-overview"));
        window.set_layer(Layer::Overlay);
        // where the windows are: beside the dock and the status panel, which
        // keep their places (an exclusive zone of nothing, not of -1)
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(0);
        // It takes the keyboard when it opens, and gives it up when another
        // window is brought to the front from the dock (which is when it
        // closes; "exclusive" would keep the keys and never hear of that).
        window.set_keyboard_mode(KeyboardMode::OnDemand);

        let ui = Rc::new(Ui {
            app: app.clone(),
            window,
            entry,
            scroll,
            content,
            win_box,
            app_box,
            nothing,
            wins: RefCell::default(),
            apps: RefCell::default(),
            shown: RefCell::default(),
            selected: Cell::new(None),
            toplevels: RefCell::new(None),
            settle: Cell::new(None),
            was_active: Cell::new(false),
            filled: Cell::new(false),
            closing: Cell::new(false),
            renderer,
        });

        ui.entry.connect_changed(clone!(
            #[weak]
            ui,
            move |_| ui.show()
        ));
        // all keys go to the search field, except the ones that choose
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(clone!(
            #[weak]
            ui,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| ui.key(key)
        ));
        ui.window.add_controller(keys);
        // a click beside the tiles and the field closes
        let beside = gtk::GestureClick::new();
        beside.connect_released(clone!(
            #[weak]
            ui,
            move |_, _, x, y| {
                let mut at = ui.window.pick(x, y, gtk::PickFlags::DEFAULT);
                while let Some(widget) = at {
                    if widget.is::<gtk::Button>() || widget.has_css_class("field") {
                        return;
                    }
                    at = widget.parent();
                }
                ui.close();
            }
        ));
        ui.window.add_controller(beside);
        // the keyboard went to something else: the user is no longer here
        ui.window.connect_is_active_notify(clone!(
            #[weak]
            ui,
            move |window| {
                // (not when it has hidden itself to start an application:
                // it then ends when that has come up, see `open`)
                if window.is_active() {
                    ui.was_active.set(true);
                } else if ui.was_active.get() && !ui.settling() && !ui.closing.get() {
                    ui.close();
                }
            }
        ));
        ui
    }

    /// The keyboard on the screen (toros-keyboard), if it is there, was there
    /// first and so lies under the overview. Ask it to come above, or the
    /// search could not be typed into with it.
    fn raise_keyboard(&self) {
        let Some(bus) = self.app.dbus_connection() else { return };
        let action = ("raise", Vec::<glib::Variant>::new(), HashMap::<String, glib::Variant>::new()).to_variant();
        bus.call(
            Some("org.toros.Keyboard"),
            "/org/toros/Keyboard",
            "org.freedesktop.Application",
            "ActivateAction",
            Some(&action),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            -1,
            gio::Cancellable::NONE,
            |_| {},
        );
    }

    /// Put in the open windows and the applications.
    fn fill(self: &Rc<Self>) {
        if self.filled.replace(true) {
            return;
        }
        self.raise_keyboard();
        let apps = apps::load();
        let toplevels = Windows::connect();
        let wins: Vec<WinTile> = toplevels
            .as_ref()
            .map(|t| t.list())
            .unwrap_or_default()
            .into_iter()
            .map(|window| self.window_tile(window, &apps))
            .collect();
        self.wins.replace(wins);
        let apps: Vec<AppTile> = apps.into_iter().enumerate().map(|(at, app)| self.app_tile(at, app)).collect();
        self.apps.replace(apps);
        self.show();

        if toplevels.is_some() {
            self.toplevels.replace(toplevels);
            glib::timeout_add_local(
                WATCH,
                clone!(
                    #[weak(rename_to = ui)]
                    self,
                    #[upgrade_or]
                    glib::ControlFlow::Break,
                    move || {
                        ui.watch();
                        glib::ControlFlow::Continue
                    }
                ),
            );
        }
    }

    fn app_tile(self: &Rc<Self>, at: usize, app: App) -> AppTile {
        let name = gtk::Label::builder()
            .label(&app.name)
            .justify(gtk::Justification::Center)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .lines(2)
            .ellipsize(pango::EllipsizeMode::End)
            .width_chars(13)
            .max_width_chars(13)
            .valign(gtk::Align::Start)
            .build();
        let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Start).build();
        content.append(&app_icon(Some(&app), APP_ICON));
        content.append(&name);
        let button = gtk::Button::builder().child(&content).css_classes(["tile", "app"]).can_focus(false).build();
        button.connect_clicked(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.open(Target::App(at))
        ));
        AppTile { cell: cell(&button), button, app }
    }

    fn window_tile(self: &Rc<Self>, window: Window, apps: &[App]) -> WinTile {
        let app = apps::of_window(apps, &window.app_id);
        let key = window.key;

        // until its picture is there (and for a window grim gets none of):
        // the place of one, with the application's icon in it
        let shot = gtk::Box::builder()
            .css_classes(["shot"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .overflow(gtk::Overflow::Hidden)
            .build();
        let waiting = app_icon(app, APP_ICON);
        waiting.set_size_request(SHOT.0 * 3 / 4, SHOT.1 * 3 / 4);
        shot.append(&waiting);
        let stage = gtk::Overlay::builder().child(&shot).build();
        stage.set_size_request(SHOT.0, SHOT.1 + WINDOW_ICON / 2);
        shot.set_margin_bottom(WINDOW_ICON / 2);
        let badge = app_icon(app, WINDOW_ICON);
        badge.set_halign(gtk::Align::Center);
        badge.set_valign(gtk::Align::End);
        badge.set_visible(false);
        stage.add_overlay(&badge);

        let title = if window.title.is_empty() { app.map(|a| a.name.as_str()).unwrap_or("Window") } else { &window.title };
        let caption = gtk::Label::builder()
            .label(title)
            .ellipsize(pango::EllipsizeMode::End)
            .max_width_chars(26)
            .css_classes(["caption"])
            .build();
        let content = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        content.append(&stage);
        content.append(&caption);
        let button = gtk::Button::builder().child(&content).css_classes(["tile", "shown"]).can_focus(false).build();
        button.connect_clicked(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.open_window(key)
        ));
        let middle = gtk::GestureClick::builder().button(gdk::BUTTON_MIDDLE).build();
        middle.connect_released(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_, _, _, _| ui.close_window(key)
        ));
        button.add_controller(middle);

        let close = gtk::Button::builder()
            .child(&own_icon("close"))
            .tooltip_text("Close")
            .css_classes(["close"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .can_focus(false)
            .build();
        close.connect_clicked(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.close_window(key)
        ));
        let tile = gtk::Overlay::builder().child(&button).css_classes(["window"]).build();
        tile.add_overlay(&close);
        if window.minimized {
            tile.add_css_class("minimized");
        }

        if let Some(capture) = window.capture.clone() {
            let scale = self.window.scale_factor().max(1);
            let (width, height) = ((SHOT.0 * scale) as usize, (SHOT.1 * scale) as usize);
            glib::spawn_future_local(clone!(
                #[weak]
                shot,
                #[weak]
                waiting,
                #[weak]
                badge,
                async move {
                    let small = gio::spawn_blocking(move || shoot(&capture, width, height)).await;
                    let Ok(Some(small)) = small else { return };
                    let texture = gdk::MemoryTexture::new(
                        small.width as i32,
                        small.height as i32,
                        gdk::MemoryFormat::R8g8b8,
                        &glib::Bytes::from_owned(small.data),
                        small.width * 3,
                    );
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_can_shrink(false);
                    picture.set_size_request(small.width as i32 / scale, small.height as i32 / scale);
                    shot.remove(&waiting);
                    shot.append(&picture);
                    badge.set_visible(true);
                }
            ));
        }

        let words = Words::new(&window.title, &[app.map(|a| a.name.as_str()).unwrap_or_default(), &window.app_id]);
        WinTile { window, words, cell: cell(&tile), button }
    }

    fn button(&self, target: Target) -> Option<gtk::Button> {
        match target {
            Target::Window(at) => self.wins.borrow().get(at).map(|tile| tile.button.clone()),
            Target::App(at) => self.apps.borrow().get(at).map(|tile| tile.button.clone()),
        }
    }

    /// Show the tiles that fit what is typed: the windows as they are, the
    /// applications best fit first (and the ones started more often before
    /// the others); with nothing typed, all of them by name.
    fn show(&self) {
        if !self.filled.get() {
            return;
        }
        let typed = self.entry.text();
        let typed = typed.trim();
        self.select(None);
        self.win_box.remove_all();
        self.app_box.remove_all();

        let mut shown = Vec::new();
        for (at, tile) in self.wins.borrow().iter().enumerate() {
            if tile.words.score(typed) > 0 {
                self.win_box.append(&tile.cell);
                shown.push(Target::Window(at));
            }
        }
        let windows = shown.len();
        let apps = self.apps.borrow();
        let mut fit: Vec<(u32, usize)> =
            apps.iter().enumerate().map(|(at, tile)| (tile.app.words.score(typed), at)).filter(|(score, _)| *score > 0).collect();
        // (a stable sort: what fits equally well stays in order of name, and
        // with nothing typed every application keeps its place)
        if !typed.is_empty() {
            fit.sort_by_key(|(score, at)| (std::cmp::Reverse(*score), std::cmp::Reverse(apps[*at].app.used)));
        }
        for (_, at) in &fit {
            self.app_box.append(&apps[*at].cell);
            shown.push(Target::App(*at));
        }

        // a row of windows that is not full stands in the middle
        self.win_box.set_max_children_per_line((windows as u32).clamp(1, WINDOWS_PER_ROW));
        self.win_box.set_visible(windows > 0);
        self.app_box.set_visible(shown.len() > windows);
        self.nothing.set_visible(shown.is_empty());
        let first = (!typed.is_empty() && !shown.is_empty()).then_some(0);
        self.shown.replace(shown);
        self.scroll.vadjustment().set_value(0.0);
        self.select(first);
    }

    fn select(&self, to: Option<usize>) {
        let target = |at: Option<usize>| at.and_then(|at| self.shown.borrow().get(at).copied());
        if let Some(button) = target(self.selected.get()).and_then(|t| self.button(t)) {
            button.remove_css_class("selected");
        }
        self.selected.set(to);
        let Some(button) = target(to).and_then(|t| self.button(t)) else { return };
        button.add_css_class("selected");
        // scroll it into view
        let view = self.scroll.vadjustment();
        if let Some(bounds) = button.compute_bounds(&self.content) {
            let (top, bottom) = (f64::from(bounds.y()), f64::from(bounds.y() + bounds.height()));
            if top < view.value() {
                view.set_value(top - 10.0);
            } else if bottom > view.value() + view.page_size() {
                view.set_value(bottom - view.page_size() + 10.0);
            }
        }
    }

    /// The middle of every shown tile, for going up and down between rows
    /// that do not have the same number of tiles.
    fn middles(&self) -> Vec<(f32, f32)> {
        self.shown
            .borrow()
            .iter()
            .map(|target| {
                self.button(*target)
                    .and_then(|button| button.compute_bounds(&self.content))
                    .map(|b| (b.x() + b.width() / 2.0, b.y() + b.height() / 2.0))
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The tile one row down (or up) from this one, the nearest one sideways.
    fn step_rows(&self, from: usize, down: bool) -> Option<usize> {
        let middles = self.middles();
        let (x, y) = *middles.get(from)?;
        let beyond = |other: f32| if down { other > y + 1.0 } else { other < y - 1.0 };
        let row = middles
            .iter()
            .map(|(_, other)| *other)
            .filter(|other| beyond(*other))
            .min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs()))?;
        middles
            .iter()
            .enumerate()
            .filter(|(_, (_, other))| (other - row).abs() < 1.0)
            .min_by(|(_, a), (_, b)| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
            .map(|(at, _)| at)
    }

    fn key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        let count = self.shown.borrow().len();
        let selected = self.selected.get();
        match key {
            gdk::Key::Escape if self.entry.text().is_empty() => self.close(),
            gdk::Key::Escape => self.entry.set_text(""),
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => {
                if let Some(target) = selected.and_then(|at| self.shown.borrow().get(at).copied()) {
                    self.open(target);
                }
            }
            _ if count == 0 => return glib::Propagation::Proceed,
            gdk::Key::Down | gdk::Key::Tab => match selected {
                None => self.select(Some(0)),
                Some(at) if key == gdk::Key::Tab => self.select(Some((at + 1) % count)),
                Some(at) => self.select(self.step_rows(at, true).or(selected)),
            },
            gdk::Key::ISO_Left_Tab => self.select(Some(selected.map_or(count - 1, |at| (at + count - 1) % count))),
            // (from the top row, up goes back to nothing chosen when nothing is typed)
            gdk::Key::Up => match selected {
                Some(at) => {
                    let up = self.step_rows(at, false);
                    self.select(up.or(selected.filter(|_| !self.entry.text().is_empty())));
                }
                None => return glib::Propagation::Proceed,
            },
            // left and right belong to the text while no tile is chosen
            gdk::Key::Right => match selected {
                Some(at) => self.select(Some((at + 1).min(count - 1))),
                None => return glib::Propagation::Proceed,
            },
            gdk::Key::Left => match selected {
                Some(at) => self.select(Some(at.saturating_sub(1))),
                None => return glib::Propagation::Proceed,
            },
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn open(self: &Rc<Self>, target: Target) {
        if self.closing.get() {
            return;
        }
        match target {
            Target::Window(at) => {
                let key = self.wins.borrow().get(at).map(|tile| tile.window.key);
                if let Some(key) = key {
                    self.open_window(key);
                }
            }
            Target::App(at) => {
                // out of the way at once; the program ends when the
                // application is on its way (see apps::launch)
                self.closing.set(true);
                self.window.set_visible(false);
                let app = self.app.clone();
                if let Some(tile) = self.apps.borrow().get(at) {
                    apps::launch(&tile.app, self.renderer.as_deref(), move || app.quit());
                }
                let app = self.app.clone();
                glib::timeout_add_local_once(LAUNCH_WAIT, move || app.quit());
            }
        }
    }

    fn open_window(&self, key: u32) {
        if self.closing.get() {
            return;
        }
        // out of the way first, so that the window gets the keyboard
        self.closing.set(true);
        self.window.set_visible(false);
        if let Some(toplevels) = self.toplevels.borrow_mut().as_mut() {
            toplevels.activate(key);
        }
        self.close();
    }

    /// Has a window just been closed from here?
    fn settling(&self) -> bool {
        self.settle.get().is_some_and(|until| Instant::now() < until)
    }

    fn close_window(&self, key: u32) {
        self.settle.set(Some(Instant::now() + SETTLE));
        if let Some(toplevels) = self.toplevels.borrow_mut().as_mut() {
            toplevels.close(key);
        }
    }

    /// See what happened to the windows while the overview is open.
    fn watch(&self) {
        let changes = self.toplevels.borrow_mut().as_mut().map(|t| t.changes()).unwrap_or_default();
        for change in changes {
            match change {
                Change::Closed(key) => {
                    let gone = self.wins.borrow().iter().position(|tile| tile.window.key == key);
                    if let Some(at) = gone {
                        self.select(None);
                        self.win_box.remove_all();
                        self.wins.borrow_mut().remove(at);
                        self.show();
                    }
                }
                // the user went elsewhere (the dock, another key): leave
                Change::Moved if !self.settling() => self.close(),
                Change::Moved => {}
            }
        }
    }

    fn close(&self) {
        self.closing.set(true);
        self.app.quit();
    }
}

fn main() -> glib::ExitCode {
    gio::resources_register_include!("icons.gresource").expect("the icons are part of the program");
    // The session draws GTK apps with OpenGL. Drawing in software starts
    // faster and needs far less memory, and nothing here moves. The
    // applications started from here must not get this setting: it is put
    // back before one is (apps::launch).
    let renderer = env::var("GSK_RENDERER").ok();
    env::set_var("GSK_RENDERER", env::var("TOROS_OVERVIEW_RENDERER").unwrap_or("cairo".to_string()));

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let open: RefCell<Option<Rc<Ui>>> = RefCell::default();
    // Runs in the overview that is already open when the command is run again
    app.connect_command_line(move |app, _| {
        let ui = open.borrow().clone();
        match ui {
            // (it is out of sight and waits for an application to start)
            Some(ui) if ui.closing.get() => {}
            Some(ui) => ui.close(),
            None => {
                let ui = Ui::build(app, renderer.clone());
                ui.window.present();
                ui.entry.grab_focus();
                // The tiles come after the first picture: the sheet is there
                // and takes what is typed while the icons are still read.
                // (for timing the start, see /usr/lib/toros/selftest: leave
                // as soon as the picture with the tiles is drawn)
                let bench = env::var_os("TOROS_OVERVIEW_BENCH").is_some();
                match ui.window.frame_clock() {
                    Some(clock) => {
                        clock.connect_after_paint(clone!(
                            #[weak]
                            ui,
                            move |_| {
                                if ui.filled.get() {
                                    if bench {
                                        ui.close();
                                    }
                                    return;
                                }
                                glib::idle_add_local_once(clone!(
                                    #[weak]
                                    ui,
                                    move || ui.fill()
                                ));
                            }
                        ));
                    }
                    None => ui.fill(),
                }
                open.replace(Some(ui));
            }
        }
        glib::ExitCode::SUCCESS
    });
    app.run()
}
