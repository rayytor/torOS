//! toros-picker: the emoji picker and clipboard history of torOS.
//!
//!   toros-picker              open on the emoji page (Super+.)
//!   toros-picker clipboard    open on the clipboard page (Super+V)
//!
//! Run again while it is open, it closes (or changes page). It starts when
//! asked and ends when it has pasted, so it uses no memory in between; the
//! clipboard history is kept by toros-clipd.
//!
//! What is chosen is put on the clipboard (wl-copy) and pasted into the window
//! that was in use with Shift+Insert, or Ctrl+V for a picture (wtype). An
//! emoji goes the same way, because it cannot be typed: Chromium drops a key
//! press whose character is beyond U+FFFF, which is nearly every emoji. It is
//! kept out of the history, and afterwards the clipboard gets back what it
//! held before.

mod emoji;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;
use std::{env, iter};

use emoji::Emoji;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};
use toros_picker::store::{self, Item, Kind};

const APP_ID: &str = "org.toros.Picker";
const WIDTH: i32 = 320;
const HEIGHT: i32 = 352;
const COLUMNS: u32 = 8;
const ICON_SIZE: i32 = 16;
const RECENT_MAX: usize = 32;
/// Time for the window that was in use to get the keyboard back before the
/// paste is sent to it.
const REFOCUS: Duration = Duration::from_millis(70);
/// Time for that window to fetch a pasted emoji before the clipboard gets its
/// earlier content back.
const FETCH: Duration = Duration::from_millis(350);
const PALETTE: &str = "/usr/share/toros/palette";

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Emoji,
    Clipboard,
}

/// What to send to the window that was in use once the picker is out of the way.
enum Paste {
    Emoji(String),
    Item(Item),
}

struct Ui {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    entry: gtk::Entry,
    page_buttons: [gtk::ToggleButton; 2],
    stack: gtk::Stack,
    status: gtk::Label,
    page: Cell<Option<Page>>,
    was_active: Cell<bool>,
    closing: Cell<bool>,

    emojis: Vec<Emoji>,
    /// The emoji in the grid, as indexes into `emojis`.
    shown: RefCell<Vec<usize>>,
    strings: gtk::StringList,
    selection: gtk::SingleSelection,
    grid: gtk::GridView,
    /// "Recently used", then one button per group of the table.
    group_buttons: Vec<gtk::ToggleButton>,
    group: Cell<usize>,
    hover: Cell<Option<u32>>,
    recent: RefCell<Vec<String>>,
    tone: Cell<usize>,
    tone_button: gtk::Button,

    list: gtk::ListBox,
    list_scroll: gtk::ScrolledWindow,
    clear_button: gtk::Button,
    /// The clipboard history, one item per row of the list, and each item's
    /// text in lower case for the search.
    items: RefCell<Vec<Item>>,
    texts: RefCell<Vec<String>>,
    list_filled: Cell<bool>,
}

fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"))
        .join("toros/picker")
}

fn save_state(name: &str, content: &str) {
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir).and_then(|()| fs::write(dir.join(name), content));
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
        ("surface", "popover", "@theme_base_color"),
        ("field", "headerbar", "alpha(@theme_fg_color, 0.07)"),
        ("fg", "fg", "@theme_fg_color"),
        ("dim", "dim", "alpha(@theme_fg_color, 0.55)"),
        ("hover", "select", "alpha(@theme_fg_color, 0.09)"),
        ("border", "border", "@borders"),
        ("accent", "accent", "@theme_selected_bg_color"),
        ("accent_text", "accent_text", "@theme_selected_bg_color"),
    ]
    .iter()
    .map(|(name, key, fallback)| {
        format!("@define-color pk_{name} {};\n", from_palette(key).unwrap_or(fallback.to_string()))
    })
    .collect()
}

/// A button with one of the picker's own icons. They are symbolic icons: drawn
/// in the button's text colour, whatever state it is in. It never takes the
/// keyboard from the search field.
fn icon_toggle(icon: &str, tooltip: &str) -> gtk::ToggleButton {
    let file = gio::File::for_uri(&format!("resource:///org/toros/Picker/icons/{icon}-symbolic.svg"));
    let scale = gdk::Display::default()
        .and_then(|d| d.monitors().iter::<gdk::Monitor>().flatten().map(|m| m.scale_factor()).max())
        .unwrap_or(1);
    let image = gtk::Image::from_paintable(Some(&gtk::IconPaintable::for_file(&file, ICON_SIZE, scale)));
    image.set_pixel_size(ICON_SIZE);
    gtk::ToggleButton::builder().child(&image).tooltip_text(tooltip).can_focus(false).build()
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}

/// How long ago an item was copied.
fn age(time_ms: u64) -> String {
    match store::now_ms().saturating_sub(time_ms) / 60_000 {
        0 => "now".to_string(),
        m @ 1..=59 => format!("{m} min"),
        m @ 60..=1439 => format!("{} h", m / 60),
        m => format!("{} d", m / 1440),
    }
}

/// Run a helper to its end; true if it worked.
fn run(program: &str, args: &[&str], stdin: Stdio) -> bool {
    Command::new(program).args(args).stdin(stdin).status().is_ok_and(|s| s.success())
}

/// Put an item of the history on the clipboard, or on the primary selection.
fn copy_item(item: &Item, primary: bool) -> bool {
    let Ok(file) = File::open(&item.path) else { return false };
    let mut args = vec!["--type", item.kind.mime()];
    if primary {
        args.push("--primary");
    }
    run("wl-copy", &args, Stdio::from(file))
}

impl Paste {
    /// Put what was chosen on the clipboard. Text also goes on the primary
    /// selection, which is what terminals paste with Shift+Insert.
    fn copy(&self) {
        match self {
            // marked as sensitive, which toros-clipd leaves out of the history
            Paste::Emoji(text) => {
                run("wl-copy", &["--sensitive", "--", text], Stdio::null());
                run("wl-copy", &["--sensitive", "--primary", "--", text], Stdio::null());
            }
            Paste::Item(item) => {
                copy_item(item, false);
                if item.kind == Kind::Text {
                    copy_item(item, true);
                }
            }
        }
    }

    /// Send the keys that paste it into the window that has the keyboard
    /// now; true if they were sent.
    fn send(&self) -> bool {
        let keys: &[&str] = match self {
            Paste::Item(item) if item.kind != Kind::Text => &["-M", "ctrl", "-k", "v", "-m", "ctrl"],
            _ => &["-M", "shift", "-k", "Insert", "-m", "shift"],
        };
        run("wtype", keys, Stdio::null())
    }
}

impl Ui {
    fn build(app: &gtk::Application) -> Rc<Self> {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&(colours() + include_str!("style.css")));
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }

        // search field and the emoji / clipboard switch
        let entry = gtk::Entry::builder().hexpand(true).css_classes(["search"]).build();
        let page_buttons = [icon_toggle("emoji", "Emoji (Super+.)"), icon_toggle("clipboard", "Clipboard (Super+V)")];
        page_buttons[1].set_group(Some(&page_buttons[0]));
        let pages = gtk::Box::builder().css_classes(["pages"]).spacing(2).build();
        pages.append(&page_buttons[0]);
        pages.append(&page_buttons[1]);
        let head = gtk::Box::builder().css_classes(["head"]).spacing(6).build();
        head.append(&entry);
        head.append(&pages);

        // emoji page: groups and the grid
        let groups = gtk::Box::builder().css_classes(["groups"]).homogeneous(true).build();
        let mut group_buttons: Vec<gtk::ToggleButton> = Vec::new();
        for (icon, tooltip) in iter::once(("recent", "Recently used")).chain(emoji::GROUPS) {
            let button = icon_toggle(icon, tooltip);
            button.set_group(group_buttons.first());
            groups.append(&button);
            group_buttons.push(button);
        }
        let strings = gtk::StringList::new(&[]);
        let selection = gtk::SingleSelection::new(Some(strings.clone()));
        let factory = gtk::SignalListItemFactory::new();
        let grid = gtk::GridView::builder()
            .model(&selection)
            .factory(&factory)
            .min_columns(COLUMNS)
            .max_columns(COLUMNS)
            .can_focus(false)
            .build();
        let emoji_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        emoji_page.append(&groups);
        emoji_page.append(&scrolled(&grid));

        // clipboard page
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Browse)
            .activate_on_single_click(true)
            .can_focus(false)
            .build();
        list.set_placeholder(Some(
            &gtk::Label::builder().label("Nothing here").css_classes(["empty"]).margin_top(110).build(),
        ));
        let list_scroll = scrolled(&list);

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .transition_duration(120)
            .vexpand(true)
            .build();
        stack.add_named(&emoji_page, Some("emoji"));
        stack.add_named(&list_scroll, Some("clipboard"));

        // the line at the bottom
        let status = gtk::Label::builder()
            .css_classes(["status"])
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        let tone_button =
            gtk::Button::builder().css_classes(["glyph"]).tooltip_text("Skin tone").can_focus(false).build();
        let clear_button = gtk::Button::builder().label("Clear all").css_classes(["text"]).can_focus(false).build();
        let foot = gtk::Box::builder().css_classes(["foot"]).spacing(4).build();
        foot.append(&status);
        foot.append(&tone_button);
        foot.append(&clear_button);

        let shell = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["shell"])
            .width_request(WIDTH)
            .height_request(HEIGHT)
            .build();
        shell.append(&head);
        shell.append(&stack);
        shell.append(&foot);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Emoji and clipboard")
            .decorated(false)
            .resizable(false)
            .css_classes(["picker"])
            .child(&shell)
            .build();

        let state = state_dir();
        let recent = fs::read_to_string(state.join("recent")).unwrap_or_default();
        let tone = fs::read_to_string(state.join("tone")).ok().and_then(|t| t.trim().parse().ok());
        let ui = Rc::new(Ui {
            app: app.clone(),
            window,
            entry,
            page_buttons,
            stack,
            status,
            page: Cell::new(None),
            was_active: Cell::new(false),
            closing: Cell::new(false),
            emojis: emoji::load(),
            shown: RefCell::default(),
            strings,
            selection,
            grid,
            group_buttons,
            group: Cell::new(0),
            hover: Cell::new(None),
            recent: RefCell::new(recent.lines().take(RECENT_MAX).map(String::from).collect()),
            tone: Cell::new(tone.filter(|t| *t < emoji::TONES.len()).unwrap_or(0)),
            tone_button,
            list,
            list_scroll,
            clear_button,
            items: RefCell::default(),
            texts: RefCell::default(),
            list_filled: Cell::new(false),
        });
        ui.connect(&factory);
        ui.show_tone();
        // nothing used yet: begin with the smileys
        ui.group.set(if ui.recent.borrow().is_empty() { 1 } else { 0 });
        ui.group_buttons[ui.group.get()].set_active(true);
        ui
    }

    fn connect(self: &Rc<Self>, factory: &gtk::SignalListItemFactory) {
        // One cell of the grid. Moving the pointer over it only names the
        // emoji at the bottom; the selection (what Enter takes) stays with the
        // keyboard. A pointer that just rests there does not count: the
        // picker opens under it.
        factory.connect_setup(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_, object| {
                let Some(cell) = object.downcast_ref::<gtk::ListItem>() else { return };
                let label = gtk::Label::builder().css_classes(["emoji"]).build();
                cell.set_child(Some(&label));

                let click = gtk::GestureClick::new();
                click.connect_pressed(|click, _, _, _| {
                    click.set_state(gtk::EventSequenceState::Claimed);
                });
                click.connect_released(clone!(
                    #[weak]
                    ui,
                    #[weak]
                    cell,
                    #[weak]
                    label,
                    move |_, _, x, y| {
                        if label.contains(x, y) {
                            ui.pick_emoji(cell.position());
                        }
                    }
                ));
                label.add_controller(click);

                let motion = gtk::EventControllerMotion::new();
                motion.connect_motion(clone!(
                    #[weak]
                    ui,
                    #[weak]
                    cell,
                    move |_, _, _| {
                        if ui.hover.replace(Some(cell.position())) != Some(cell.position()) {
                            ui.show_status();
                        }
                    }
                ));
                motion.connect_leave(clone!(
                    #[weak]
                    ui,
                    move |_| {
                        ui.hover.set(None);
                        ui.show_status();
                    }
                ));
                label.add_controller(motion);
            }
        ));
        factory.connect_bind(|_, object| {
            let Some(cell) = object.downcast_ref::<gtk::ListItem>() else { return };
            if let (Some(label), Some(text)) =
                (cell.child().and_downcast::<gtk::Label>(), cell.item().and_downcast::<gtk::StringObject>())
            {
                label.set_label(&text.string());
            }
        });
        self.selection.connect_selected_notify(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| ui.show_status()
        ));

        for (i, button) in self.group_buttons.iter().enumerate() {
            button.connect_toggled(clone!(
                #[weak(rename_to = ui)]
                self,
                move |button| {
                    let searching = !ui.entry.text().is_empty();
                    if button.is_active() && (ui.group.get() != i || searching) {
                        ui.group.set(i);
                        if searching {
                            ui.entry.set_text(""); // fills the grid as well
                        } else {
                            ui.fill_grid();
                        }
                    }
                }
            ));
        }
        for (button, page) in self.page_buttons.iter().zip([Page::Emoji, Page::Clipboard]) {
            button.connect_toggled(clone!(
                #[weak(rename_to = ui)]
                self,
                move |button| {
                    if button.is_active() {
                        ui.show_page(page);
                    }
                }
            ));
        }
        self.tone_button.connect_clicked(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                ui.tone.set((ui.tone.get() + 1) % emoji::TONES.len());
                save_state("tone", &ui.tone.get().to_string());
                ui.show_tone();
                ui.fill_grid();
            }
        ));

        self.entry.connect_changed(clone!(
            #[weak(rename_to = ui)]
            self,
            move |entry| match ui.page.get() {
                Some(Page::Emoji) => {
                    // no group is lit while the grid shows what was searched for
                    ui.group_buttons[ui.group.get()].set_active(entry.text().is_empty());
                    ui.fill_grid();
                }
                Some(Page::Clipboard) => {
                    ui.list.invalidate_filter();
                    ui.select_row(ui.visible_rows().first());
                    ui.show_status();
                }
                None => {}
            }
        ));

        self.list.set_filter_func(clone!(
            #[weak(rename_to = ui)]
            self,
            #[upgrade_or]
            true,
            move |row| {
                let query = ui.entry.text().to_lowercase();
                query.is_empty() || ui.texts.borrow().get(row.index() as usize).is_some_and(|t| t.contains(&query))
            }
        ));
        self.list.connect_row_activated(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_, row| ui.paste_item(row.index() as usize)
        ));
        self.clear_button.connect_clicked(clone!(
            #[weak(rename_to = ui)]
            self,
            move |_| {
                store::clear();
                ui.fill_list();
            }
        ));

        // The search field keeps the keyboard; these keys are taken before it
        // sees them.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(clone!(
            #[weak(rename_to = ui)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| ui.key(key)
        ));
        self.window.add_controller(keys);

        // A click on another window closes the picker
        self.window.connect_is_active_notify(clone!(
            #[weak(rename_to = ui)]
            self,
            move |window| {
                if window.is_active() {
                    ui.was_active.set(true);
                } else if ui.was_active.get() && !ui.closing.get() {
                    ui.close();
                }
            }
        ));
    }

    fn key(self: &Rc<Self>, key: gdk::Key) -> glib::Propagation {
        let emoji = self.page.get() == Some(Page::Emoji);
        match key {
            gdk::Key::Escape => self.close(),
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if emoji {
                    self.pick_emoji(self.selection.selected());
                } else if let Some(row) = self.list.selected_row() {
                    self.paste_item(row.index() as usize);
                }
            }
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                self.show_page(if emoji { Page::Clipboard } else { Page::Emoji });
            }
            gdk::Key::Up => self.step(0, -1),
            gdk::Key::Down => self.step(0, 1),
            gdk::Key::Left if emoji => self.step(-1, 0),
            gdk::Key::Right if emoji => self.step(1, 0),
            gdk::Key::Page_Up | gdk::Key::Page_Down if emoji => {
                let n = self.group_buttons.len();
                let next = if key == gdk::Key::Page_Down { self.group.get() + 1 } else { self.group.get() + n - 1 };
                self.group_buttons[next % n].set_active(true);
            }
            gdk::Key::Delete if !emoji && self.entry.text().is_empty() => {
                if let Some(row) = self.list.selected_row() {
                    self.remove_item(&row);
                }
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    /// Move the selection with the arrow keys.
    fn step(&self, dx: i32, dy: i32) {
        if self.page.get() == Some(Page::Clipboard) {
            let rows = self.visible_rows();
            let at = self.list.selected_row().and_then(|s| rows.iter().position(|r| *r == s));
            let next = match at {
                Some(at) => (at as i32 + dy).clamp(0, rows.len() as i32 - 1) as usize,
                None => 0,
            };
            self.select_row(rows.get(next));
            return;
        }
        let count = self.strings.n_items() as i32;
        if count == 0 {
            return;
        }
        // the bottom line names the selected emoji again, wherever the pointer rests
        self.hover.set(None);
        let at = (self.selection.selected() as i32).clamp(0, count - 1);
        let mut next = at + dx + dy * COLUMNS as i32;
        if next >= count && dy > 0 && at / (COLUMNS as i32) < (count - 1) / COLUMNS as i32 {
            next = count - 1; // down into a last row that is shorter
        }
        if (0..count).contains(&next) {
            self.selection.set_selected(next as u32);
            self.grid.scroll_to(next as u32, gtk::ListScrollFlags::NONE, None);
        }
    }

    fn show_page(self: &Rc<Self>, page: Page) {
        if self.page.get() == Some(page) {
            return;
        }
        self.page.set(Some(page));
        let emoji = page == Page::Emoji;
        self.page_buttons[if emoji { 0 } else { 1 }].set_active(true);
        self.stack.set_visible_child_name(if emoji { "emoji" } else { "clipboard" });
        self.tone_button.set_visible(emoji);
        self.clear_button.set_visible(!emoji);
        self.entry.set_placeholder_text(Some(if emoji { "Search emoji" } else { "Search clipboard" }));
        if !emoji && !self.list_filled.replace(true) {
            self.fill_list();
        }
        if self.entry.text().is_empty() {
            // otherwise done when the search field changes
            if emoji {
                self.fill_grid();
            } else {
                self.show_status();
            }
        } else {
            self.entry.set_text("");
        }
        self.entry.grab_focus();
    }

    fn show_tone(&self) {
        self.tone_button.set_label(&format!("✋{}", emoji::TONES[self.tone.get()]));
    }

    /// Fill the grid with what was searched for, or with the chosen group.
    fn fill_grid(&self) {
        let query = self.entry.text();
        let shown: Vec<usize> = if !query.trim().is_empty() {
            emoji::search(&self.emojis, &query)
        } else if self.group.get() == 0 {
            let recent = self.recent.borrow();
            recent.iter().filter_map(|r| self.emojis.iter().position(|e| e.text == r)).collect()
        } else {
            let group = self.group.get() - 1;
            (0..self.emojis.len()).filter(|&i| self.emojis[i].group == group).collect()
        };
        let tone = self.tone.get();
        let texts: Vec<_> = shown.iter().map(|&i| self.emojis[i].with_tone(tone)).collect();
        let texts: Vec<&str> = texts.iter().map(|t| t.as_ref()).collect();
        self.hover.set(None);
        *self.shown.borrow_mut() = shown;
        self.strings.splice(0, self.strings.n_items(), &texts);
        if !texts.is_empty() {
            self.selection.set_selected(0);
            self.grid.scroll_to(0, gtk::ListScrollFlags::NONE, None);
        }
        self.show_status();
    }

    /// The bottom line: the name of the emoji under the pointer or else the
    /// selected one; on the clipboard page what the keys do.
    fn show_status(&self) {
        let text = if self.page.get() == Some(Page::Clipboard) {
            if self.visible_rows().is_empty() { "" } else { "Enter pastes, Delete removes" }.to_string()
        } else {
            let at = self.hover.get().unwrap_or(self.selection.selected());
            match self.shown.borrow().get(at as usize) {
                Some(&i) => {
                    let name = self.emojis[i].name;
                    let mut chars = name.chars();
                    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
                }
                None if !self.entry.text().is_empty() => "No emoji found".to_string(),
                None => "No emoji used yet".to_string(),
            }
        };
        self.status.set_label(&text);
    }

    fn pick_emoji(self: &Rc<Self>, at: u32) {
        let Some(&i) = self.shown.borrow().get(at as usize) else { return };
        let emoji = &self.emojis[i];
        let mut recent = self.recent.borrow_mut();
        recent.retain(|r| r != emoji.text);
        recent.insert(0, emoji.text.to_string());
        recent.truncate(RECENT_MAX);
        save_state("recent", &recent.join("\n"));
        self.finish(Paste::Emoji(emoji.with_tone(self.tone.get()).into_owned()));
    }

    /// Read the clipboard history into the list.
    fn fill_list(self: &Rc<Self>) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let items = store::list();
        let mut texts = Vec::with_capacity(items.len());
        for item in &items {
            let (row, text) = self.row(item);
            self.list.append(&row);
            texts.push(text);
        }
        *self.items.borrow_mut() = items;
        *self.texts.borrow_mut() = texts;
        self.select_row(self.visible_rows().first());
        self.show_status();
    }

    /// The row of one item and the text to search it by.
    fn row(self: &Rc<Self>, item: &Item) -> (gtk::ListBoxRow, String) {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let find = if item.kind == Kind::Text {
            // the beginning is enough to show and to search
            let mut text = Vec::new();
            let _ = File::open(&item.path).map(|f| f.take(32 * 1024).read_to_end(&mut text));
            let text = String::from_utf8_lossy(&text);
            let preview: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
            let label = gtk::Label::builder()
                .label(preview)
                .xalign(0.0)
                .hexpand(true)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                .lines(2)
                .ellipsize(pango::EllipsizeMode::End)
                .max_width_chars(20)
                .build();
            line.append(&label);
            text.to_lowercase()
        } else {
            match Pixbuf::from_file_at_scale(&item.path, 230, 64, true) {
                Ok(pixbuf) => {
                    let picture = gtk::Picture::for_paintable(&gdk::Texture::for_pixbuf(&pixbuf));
                    picture.set_can_shrink(false);
                    picture.set_halign(gtk::Align::Start);
                    picture.set_hexpand(true);
                    line.append(&picture);
                }
                Err(_) => line.append(&gtk::Label::builder().label("Picture").xalign(0.0).hexpand(true).build()),
            }
            "picture image".to_string()
        };
        line.append(&gtk::Label::builder().label(age(item.time_ms)).css_classes(["age"]).build());
        let remove = gtk::Button::builder()
            .label("×")
            .tooltip_text("Remove")
            .css_classes(["remove"])
            .valign(gtk::Align::Center)
            .can_focus(false)
            .build();
        line.append(&remove);
        let row = gtk::ListBoxRow::builder().child(&line).can_focus(false).build();
        remove.connect_clicked(clone!(
            #[weak(rename_to = ui)]
            self,
            #[weak]
            row,
            move |_| ui.remove_item(&row)
        ));
        (row, find)
    }

    fn visible_rows(&self) -> Vec<gtk::ListBoxRow> {
        (0..).map_while(|i| self.list.row_at_index(i)).filter(|r| r.is_child_visible()).collect()
    }

    /// Select a row of the clipboard list and scroll it into view.
    fn select_row(&self, row: Option<&gtk::ListBoxRow>) {
        self.list.select_row(row);
        let adjustment = self.list_scroll.vadjustment();
        if let Some(bounds) = row.and_then(|r| r.compute_bounds(&self.list)) {
            let (top, bottom) = (f64::from(bounds.y()), f64::from(bounds.y() + bounds.height()));
            if top < adjustment.value() {
                adjustment.set_value(top - 2.0);
            } else if bottom > adjustment.value() + adjustment.page_size() {
                adjustment.set_value(bottom - adjustment.page_size() + 6.0);
            }
        }
    }

    fn remove_item(&self, row: &gtk::ListBoxRow) {
        let at = row.index() as usize;
        let rows = self.visible_rows();
        let next = rows.iter().position(|r| r == row).and_then(|p| rows.get(p + 1).or(p.checked_sub(1).map(|p| &rows[p])));
        store::remove(&self.items.borrow_mut().remove(at));
        self.texts.borrow_mut().remove(at);
        self.list.remove(row);
        self.select_row(next);
        self.show_status();
    }

    fn paste_item(self: &Rc<Self>, at: usize) {
        let Some(item) = self.items.borrow().get(at).cloned() else { return };
        self.finish(Paste::Item(item));
    }

    /// Get out of the way, then put what was chosen into the window that was
    /// in use before the picker opened.
    fn finish(self: &Rc<Self>, paste: Paste) {
        if self.closing.replace(true) {
            return;
        }
        // An emoji only passes through the clipboard: note what is there now
        let before = if matches!(paste, Paste::Emoji(_)) { store::current() } else { None };
        paste.copy();
        self.window.set_visible(false);
        let app = self.app.clone();
        glib::timeout_add_local_once(REFOCUS, move || {
            // (if the keys could not be sent, the emoji stays to be pasted by hand)
            let sent = paste.send();
            match before {
                Some(before) if sent => {
                    glib::timeout_add_local_once(FETCH, move || {
                        copy_item(&before, false);
                        app.quit();
                    });
                }
                _ => app.quit(),
            }
        });
    }

    fn close(&self) {
        self.closing.set(true);
        self.app.quit();
    }
}

fn main() -> glib::ExitCode {
    gio::resources_register_include!("icons.gresource").expect("the icons are part of the program");
    // The session draws GTK apps with OpenGL. For a window this small drawing
    // in software starts faster and needs far less memory.
    env::set_var("GSK_RENDERER", env::var("TOROS_PICKER_RENDERER").unwrap_or("cairo".to_string()));

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let open: RefCell<Option<Rc<Ui>>> = RefCell::default();
    // Runs in the picker that is already open when the command is run again
    app.connect_command_line(move |app, command| {
        let page = match command.arguments().get(1).and_then(|a| a.to_str()) {
            Some("clipboard") => Page::Clipboard,
            _ => Page::Emoji,
        };
        let ui = open.borrow().clone();
        match ui {
            Some(ui) if ui.closing.get() => {}
            Some(ui) if ui.page.get() == Some(page) => ui.close(),
            Some(ui) => ui.show_page(page),
            None => {
                let ui = Ui::build(app);
                ui.show_page(page);
                ui.window.present();
                // for timing the start (see /usr/lib/toros/selftest): leave
                // as soon as the first picture of the window is drawn
                if env::var_os("TOROS_PICKER_BENCH").is_some() {
                    if let Some(clock) = ui.window.frame_clock() {
                        let app = app.clone();
                        clock.connect_after_paint(move |_| app.quit());
                    }
                }
                open.replace(Some(ui));
            }
        }
        glib::ExitCode::SUCCESS
    });
    app.run()
}
