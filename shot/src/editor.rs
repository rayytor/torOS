//! The editor: a screenshot in a window, to be drawn on with the highlighter
//! or the pen (and an eraser for what was drawn). There is no "save": whenever the window is left or closed, the
//! picture as it looks then replaces the file and goes on the clipboard.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::cairo::{self, ImageSurface};
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::canvas::{self, Stroke, Tool, INKS};
use crate::capture;
use crate::ui;

/// Room the window leaves on the screen for the panel, the dock, its own title
/// bar and its tools, when the picture is as large as the screen.
const MARGIN: (f64, f64) = (48.0, 190.0);

/// What the pointer does on the picture.
#[derive(Clone, Copy, PartialEq)]
enum Hand {
    Draw(Tool),
    Erase,
}

pub struct Editor {
    window: gtk::ApplicationWindow,
    canvas: gtk::DrawingArea,
    tools: [gtk::ToggleButton; 3],
    inks: Vec<gtk::ToggleButton>,
    undo: gtk::Button,
    redo: gtk::Button,
    picture: ImageSurface,
    /// the file the picture is in, if it could be saved
    file: Option<PathBuf>,
    /// screen units per pixel of the picture: 1, or less for a picture that
    /// would not fit on the screen
    fit: f64,
    hand: Cell<Hand>,
    ink: Cell<usize>,
    strokes: RefCell<Vec<Stroke>>,
    /// the strokes as they were before each change, and after each one that
    /// was taken back
    before: RefCell<Vec<Vec<Stroke>>>,
    after: RefCell<Vec<Vec<Stroke>>>,
    /// drawn on since it was last saved and copied
    changed: Cell<bool>,
}

impl Editor {
    pub fn open(app: &gtk::Application, picture: ImageSurface, file: Option<PathBuf>) -> Rc<Editor> {
        let (width, height) = (f64::from(picture.width()), f64::from(picture.height()));
        let screen = gdk::Display::default()
            .and_then(|d| d.monitors().iter::<gdk::Monitor>().flatten().next())
            .map(|m| m.geometry())
            .map_or((1280.0, 720.0), |g| (f64::from(g.width()), f64::from(g.height())));
        let fit = ((screen.0 - MARGIN.0) / width).min((screen.1 - MARGIN.1) / height).min(1.0);

        let tools = [
            ui::icon_toggle("marker", "Highlighter (H)"),
            ui::icon_toggle("pen", "Pen (P)"),
            ui::icon_toggle("eraser", "Eraser (E)"),
        ];
        tools[1].set_group(Some(&tools[0]));
        tools[2].set_group(Some(&tools[0]));
        tools[0].set_active(true);
        let inks: Vec<gtk::ToggleButton> = INKS
            .iter()
            .enumerate()
            .map(|(i, (name, _, _))| {
                let drop = gtk::Box::builder()
                    .css_classes(["ink", &format!("ink{i}")])
                    .halign(gtk::Align::Center)
                    .valign(gtk::Align::Center)
                    .build();
                gtk::ToggleButton::builder()
                    .child(&drop)
                    .tooltip_text(format!("{name} ({})", i + 1))
                    .css_classes(["swatch"])
                    .can_focus(false)
                    .build()
            })
            .collect();
        for ink in &inks[1..] {
            ink.set_group(Some(&inks[0]));
        }
        inks[0].set_active(true);
        let undo = ui::icon_button("undo", "Undo (Ctrl+Z)");
        let redo = ui::icon_button("redo", "Redo (Ctrl+Shift+Z)");
        let done = gtk::Button::builder()
            .label("Done")
            .tooltip_text("Copy and close (Enter)")
            .css_classes(["done"])
            .can_focus(false)
            .build();

        let bar = gtk::Box::builder().css_classes(["tools"]).spacing(2).build();
        for tool in &tools {
            bar.append(tool);
        }
        bar.append(&ui::rule());
        for ink in &inks {
            bar.append(ink);
        }
        bar.append(&ui::rule());
        bar.append(&undo);
        bar.append(&redo);
        bar.append(&gtk::Box::builder().hexpand(true).width_request(24).build());
        bar.append(&done);

        let canvas = gtk::DrawingArea::builder()
            .content_width((width * fit).round() as i32)
            .content_height((height * fit).round() as i32)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .css_classes(["sheet"])
            .build();
        canvas.set_cursor_from_name(Some("crosshair"));
        let stage = gtk::Box::builder().css_classes(["stage"]).halign(gtk::Align::Fill).build();
        canvas.set_hexpand(true);
        stage.append(&canvas);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&bar);
        content.append(&stage);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Screenshot")
            .resizable(false)
            .css_classes(["shot-editor"])
            .child(&content)
            .build();

        let editor = Rc::new(Editor {
            window,
            canvas,
            tools,
            inks,
            undo,
            redo,
            picture,
            file,
            fit,
            hand: Cell::new(Hand::Draw(Tool::Marker)),
            ink: Cell::new(0),
            strokes: RefCell::default(),
            before: RefCell::default(),
            after: RefCell::default(),
            changed: Cell::new(false),
        });
        editor.connect(&done);
        editor.show_steps();
        editor.window.present();
        editor
    }

    fn connect(self: &Rc<Self>, done: &gtk::Button) {
        self.canvas.set_draw_func(clone!(
            #[weak(rename_to = editor)]
            self,
            move |_, cr, _, _| {
                cr.scale(editor.fit, editor.fit);
                editor.paint(cr);
            }
        ));

        for (button, hand) in self.tools.iter().zip([Hand::Draw(Tool::Marker), Hand::Draw(Tool::Pen), Hand::Erase]) {
            button.connect_toggled(clone!(
                #[weak(rename_to = editor)]
                self,
                move |button| {
                    if button.is_active() {
                        editor.hand.set(hand);
                    }
                }
            ));
        }
        for (i, button) in self.inks.iter().enumerate() {
            button.connect_toggled(clone!(
                #[weak(rename_to = editor)]
                self,
                move |button| {
                    if button.is_active() {
                        editor.ink.set(i);
                    }
                }
            ));
        }
        self.undo.connect_clicked(clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| editor.step(false)
        ));
        self.redo.connect_clicked(clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| editor.step(true)
        ));
        done.connect_clicked(clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| editor.window.close()
        ));

        // Drawing. With Shift held the stroke is a straight line, level or
        // upright if it is nearly so. The eraser takes away each stroke it
        // touches, whole.
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(clone!(
            #[weak(rename_to = editor)]
            self,
            move |_, x, y| {
                let at = (x / editor.fit, y / editor.fit);
                match editor.hand.get() {
                    Hand::Draw(tool) => {
                        editor.change();
                        editor.strokes.borrow_mut().push(Stroke::new(tool, editor.ink.get(), at));
                        editor.canvas.queue_draw();
                    }
                    Hand::Erase => editor.erase(at),
                }
            }
        ));
        drag.connect_drag_update(clone!(
            #[weak(rename_to = editor)]
            self,
            move |drag, dx, dy| {
                let Some((x, y)) = drag.start_point() else { return };
                let at = ((x + dx) / editor.fit, (y + dy) / editor.fit);
                if editor.hand.get() == Hand::Erase {
                    return editor.erase(at);
                }
                if let Some(stroke) = editor.strokes.borrow_mut().last_mut() {
                    if drag.current_event_state().contains(gdk::ModifierType::SHIFT_MASK) {
                        let from = stroke.points[0];
                        stroke.points.truncate(1);
                        stroke.points.push(canvas::straight(from, at));
                    } else {
                        stroke.points.push(at);
                    }
                }
                editor.canvas.queue_draw();
            }
        ));
        self.canvas.add_controller(drag);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(clone!(
            #[weak(rename_to = editor)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, state| {
                let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
                let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
                match key.to_lower() {
                    gdk::Key::Escape | gdk::Key::Return | gdk::Key::KP_Enter => editor.window.close(),
                    gdk::Key::z if ctrl => editor.step(shift),
                    gdk::Key::y if ctrl => editor.step(true),
                    gdk::Key::c if ctrl => editor.keep(),
                    gdk::Key::h => editor.tools[0].set_active(true),
                    gdk::Key::p => editor.tools[1].set_active(true),
                    gdk::Key::e => editor.tools[2].set_active(true),
                    gdk::Key::_1 => editor.inks[0].set_active(true),
                    gdk::Key::_2 => editor.inks[1].set_active(true),
                    gdk::Key::_3 => editor.inks[2].set_active(true),
                    gdk::Key::_4 => editor.inks[3].set_active(true),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        self.window.add_controller(keys);

        // Leaving the window (to paste the picture somewhere) or closing it
        // keeps what was drawn. The editor lives as long as its window.
        self.window.connect_is_active_notify(clone!(
            #[weak(rename_to = editor)]
            self,
            move |window| {
                if !window.is_active() {
                    editor.keep();
                }
            }
        ));
        self.window.connect_close_request(clone!(
            #[strong(rename_to = editor)]
            self,
            move |_| {
                editor.keep();
                glib::Propagation::Proceed
            }
        ));
    }

    /// The picture with what was drawn on it, in the picture's pixels.
    fn paint(&self, cr: &cairo::Context) {
        let _ = cr.set_source_surface(&self.picture, 0.0, 0.0);
        let _ = cr.paint();
        for stroke in self.strokes.borrow().iter() {
            stroke.draw(cr, true);
        }
    }

    /// The strokes are about to change: note how they are now, for "undo".
    fn change(&self) {
        self.before.borrow_mut().push(self.strokes.borrow().clone());
        self.after.borrow_mut().clear();
        self.changed.set(true);
        self.show_steps();
    }

    /// Take away the topmost stroke under a point of the picture.
    fn erase(&self, at: (f64, f64)) {
        let found = self.strokes.borrow().iter().rposition(|stroke| stroke.covers(at, 3.0 / self.fit));
        if let Some(found) = found {
            self.change();
            self.strokes.borrow_mut().remove(found);
            self.canvas.queue_draw();
        }
    }

    /// Take the last change back, or make again the one taken back last.
    fn step(&self, forward: bool) {
        let (from, to) = if forward { (&self.after, &self.before) } else { (&self.before, &self.after) };
        let Some(strokes) = from.borrow_mut().pop() else { return };
        to.borrow_mut().push(self.strokes.replace(strokes));
        self.changed.set(true);
        self.show_steps();
        self.canvas.queue_draw();
    }

    fn show_steps(&self) {
        self.undo.set_sensitive(!self.before.borrow().is_empty());
        self.redo.set_sensitive(!self.after.borrow().is_empty());
    }

    /// Write the picture as it looks now into its file and put it on the
    /// clipboard, if it was drawn on since the last time.
    fn keep(&self) {
        if !self.changed.replace(false) {
            return;
        }
        let Ok(result) = ImageSurface::create(self.picture.format(), self.picture.width(), self.picture.height()) else {
            return;
        };
        if let Ok(cr) = cairo::Context::new(&result) {
            self.paint(&cr);
        }
        let Some(png) = capture::png(&result) else { return };
        if let Some(file) = &self.file {
            let _ = fs::write(file, &png);
        }
        capture::copy(&png);
    }
}
