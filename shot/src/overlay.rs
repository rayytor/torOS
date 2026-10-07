//! The frozen screen: the picture grim took, shown over everything on each
//! monitor and a little darker, with a small bar at the top. An area is
//! dragged out of it, for a screenshot or to be recorded.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::capture::{PixelRect, Rect, Shot};
use crate::ui;

/// An area smaller than this (a click, a slip of the finger) is not taken.
const SMALLEST: f64 = 8.0;
pub const RED: (f64, f64, f64) = (0.88, 0.11, 0.14);

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Photo,
    Video,
}

/// The shape of a screenshot: a rectangle, or whatever is drawn by hand.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Rect,
    Free,
}

/// What was chosen on the frozen screen.
pub enum Choice {
    Cancelled,
    /// a part of the picture, in its pixels
    Photo(PixelRect),
    /// a shape drawn by hand: its outline in the picture's pixels
    Shape(Vec<(f64, f64)>),
    /// a part of one monitor, in that monitor's own screen units, to be recorded
    Video { monitor: gdk::Monitor, area: Rect },
}

type Done = Box<dyn FnOnce(&Shot, Choice)>;

/// The window on one monitor.
struct Pane {
    window: gtk::ApplicationWindow,
    area: gtk::DrawingArea,
    top: gtk::Box,
    modes: [gtk::ToggleButton; 2],
    shapes: [gtk::ToggleButton; 2],
    hint: gtk::Label,
    monitor: gdk::Monitor,
    /// where this monitor lies in the picture, in screen units
    origin: (f64, f64),
    size: (f64, f64),
    selection: Cell<Option<Rect>>,
    /// the line drawn by hand, while it is drawn
    outline: RefCell<Vec<(f64, f64)>>,
}

pub struct Overlay {
    shot: Shot,
    /// the picture's pixels per screen unit
    scale: f64,
    mode: Cell<Mode>,
    shape: Cell<Shape>,
    panes: RefCell<Vec<Rc<Pane>>>,
    done: RefCell<Option<Done>>,
}

impl Overlay {
    /// Show the frozen screen. `done` is called once, when an area was chosen
    /// or the overlay was closed; by then its windows are gone.
    pub fn open(
        app: &gtk::Application,
        shot: Shot,
        mode: Mode,
        done: impl FnOnce(&Shot, Choice) + 'static,
    ) -> Rc<Overlay> {
        let monitors: Vec<gdk::Monitor> = gdk::Display::default()
            .map(|d| d.monitors().iter::<gdk::Monitor>().flatten().collect())
            .unwrap_or_default();
        // grim's picture begins at the top left corner of all monitors together
        let boxes: Vec<gdk::Rectangle> = monitors.iter().map(|m| m.geometry()).collect();
        let left = boxes.iter().map(|b| b.x()).min().unwrap_or(0);
        let top = boxes.iter().map(|b| b.y()).min().unwrap_or(0);
        let right = boxes.iter().map(|b| b.x() + b.width()).max().unwrap_or(1);
        let scale = f64::from(shot.width) / f64::from((right - left).max(1));

        let overlay = Rc::new(Overlay {
            shot,
            scale,
            mode: Cell::new(mode),
            // the shape used the last time, as on Windows
            shape: Cell::new(if ui::remembered("shape").as_deref() == Some("free") { Shape::Free } else { Shape::Rect }),
            panes: RefCell::default(),
            done: RefCell::new(Some(Box::new(done))),
        });
        for (monitor, place) in monitors.into_iter().zip(boxes) {
            let origin = (f64::from(place.x() - left), f64::from(place.y() - top));
            let size = (f64::from(place.width()), f64::from(place.height()));
            let pane = overlay.pane(app, monitor, origin, size);
            overlay.panes.borrow_mut().push(pane);
        }
        overlay.show_mode();
        for pane in overlay.panes.borrow().iter() {
            pane.window.present();
        }
        if overlay.panes.borrow().is_empty() {
            overlay.finish(Choice::Cancelled);
        }
        overlay
    }

    fn pane(self: &Rc<Self>, app: &gtk::Application, monitor: gdk::Monitor, origin: (f64, f64), size: (f64, f64)) -> Rc<Pane> {
        // the bar, as on Windows: screenshot or recording; a rectangle, a
        // shape drawn by hand or the whole screen; close
        let modes = [
            ui::word_toggle("photo", "Screenshot", "Take a screenshot (Print)"),
            ui::word_toggle("video", "Record", "Record the screen (Super+Shift+R)"),
        ];
        modes[1].set_group(Some(&modes[0]));
        let switch = gtk::Box::builder().css_classes(["modes"]).spacing(2).build();
        switch.append(&modes[0]);
        switch.append(&modes[1]);
        let shapes = [ui::icon_toggle("rect", "Rectangle"), ui::icon_toggle("free", "Freeform")];
        shapes[1].set_group(Some(&shapes[0]));
        let whole = ui::icon_button("screen", "The whole screen (Enter)");
        let close = ui::icon_button("close", "Close (Esc)");
        let bar = gtk::Box::builder().css_classes(["bar"]).spacing(2).halign(gtk::Align::Center).build();
        bar.append(&switch);
        bar.append(&ui::rule());
        bar.append(&shapes[0]);
        bar.append(&shapes[1]);
        bar.append(&whole);
        bar.append(&ui::rule());
        bar.append(&close);
        let hint = gtk::Label::builder().css_classes(["hint"]).halign(gtk::Align::Center).build();
        let top = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Start)
            .margin_top(20)
            .build();
        top.append(&bar);
        top.append(&hint);

        let area = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        area.set_cursor_from_name(Some("crosshair"));
        let layers = gtk::Overlay::builder().child(&area).build();
        layers.add_overlay(&top);

        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Screenshot")
            .decorated(false)
            .css_classes(["shot-screen"])
            .child(&layers)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some("toros-shot"));
        window.set_layer(Layer::Overlay);
        window.set_monitor(Some(&monitor));
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        // over the panel and the dock as well, and all keys come here
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::Exclusive);

        let pane = Rc::new(Pane {
            window,
            area,
            top,
            modes,
            shapes,
            hint,
            monitor,
            origin,
            size,
            selection: Cell::new(None),
            outline: RefCell::default(),
        });

        pane.area.set_draw_func(clone!(
            #[weak(rename_to = overlay)]
            self,
            #[weak]
            pane,
            move |_, cr, width, height| overlay.draw(&pane, cr, f64::from(width), f64::from(height))
        ));

        for (button, mode) in pane.modes.iter().zip([Mode::Photo, Mode::Video]) {
            button.connect_toggled(clone!(
                #[weak(rename_to = overlay)]
                self,
                move |button| {
                    if button.is_active() && overlay.mode.get() != mode {
                        overlay.set_mode(mode);
                    }
                }
            ));
        }
        for (button, shape) in pane.shapes.iter().zip([Shape::Rect, Shape::Free]) {
            button.connect_toggled(clone!(
                #[weak(rename_to = overlay)]
                self,
                move |button| {
                    // (while recording is chosen the rectangle is lit whatever the shape is)
                    if button.is_active() && overlay.mode.get() == Mode::Photo && overlay.shape.get() != shape {
                        overlay.shape.set(shape);
                        ui::remember("shape", if shape == Shape::Free { "free" } else { "rect" });
                        overlay.show_mode();
                    }
                }
            ));
        }
        whole.connect_clicked(clone!(
            #[weak(rename_to = overlay)]
            self,
            #[weak]
            pane,
            move |_| overlay.choose(&pane, None)
        ));
        close.connect_clicked(clone!(
            #[weak(rename_to = overlay)]
            self,
            move |_| overlay.finish(Choice::Cancelled)
        ));

        // dragging out the area; the bar is out of the way meanwhile
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(clone!(
            #[weak]
            pane,
            move |_, x, y| {
                pane.top.set_visible(false);
                pane.outline.replace(vec![(x, y)]);
            }
        ));
        drag.connect_drag_update(clone!(
            #[weak(rename_to = overlay)]
            self,
            #[weak]
            pane,
            move |drag, dx, dy| {
                let Some((x, y)) = drag.start_point() else { return };
                let to = ((x + dx).clamp(0.0, pane.size.0), (y + dy).clamp(0.0, pane.size.1));
                if overlay.by_hand() {
                    pane.outline.borrow_mut().push(to);
                } else {
                    pane.selection.set(Some(Rect::between((x, y), to, pane.size.0, pane.size.1)));
                }
                pane.area.queue_draw();
            }
        ));
        drag.connect_drag_end(clone!(
            #[weak(rename_to = overlay)]
            self,
            #[weak]
            pane,
            move |_, _, _| {
                let outline = pane.outline.take();
                let (wide, high) = if overlay.by_hand() {
                    let across = |of: fn(&(f64, f64)) -> f64| {
                        outline.iter().map(of).fold(f64::MIN, f64::max) - outline.iter().map(of).fold(f64::MAX, f64::min)
                    };
                    (across(|p| p.0), across(|p| p.1))
                } else {
                    pane.selection.get().map_or((0.0, 0.0), |area| (area.w, area.h))
                };
                if wide < SMALLEST || high < SMALLEST {
                    pane.selection.set(None);
                    pane.top.set_visible(true);
                    pane.area.queue_draw();
                } else if overlay.by_hand() {
                    let scale = overlay.scale;
                    let in_picture = outline.iter().map(|p| ((pane.origin.0 + p.0) * scale, (pane.origin.1 + p.1) * scale));
                    overlay.finish(Choice::Shape(in_picture.collect()));
                } else {
                    overlay.choose(&pane, pane.selection.get());
                }
            }
        ));
        pane.area.add_controller(drag);
        // the other mouse button closes
        let other = gtk::GestureClick::builder().button(gdk::BUTTON_SECONDARY).build();
        other.connect_pressed(clone!(
            #[weak(rename_to = overlay)]
            self,
            move |_, _, _, _| overlay.finish(Choice::Cancelled)
        ));
        pane.area.add_controller(other);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(clone!(
            #[weak(rename_to = overlay)]
            self,
            #[weak]
            pane,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                match key {
                    gdk::Key::Escape => overlay.finish(Choice::Cancelled),
                    gdk::Key::Return | gdk::Key::KP_Enter => overlay.choose(&pane, None),
                    gdk::Key::Tab | gdk::Key::ISO_Left_Tab => {
                        overlay.set_mode(if overlay.mode.get() == Mode::Photo { Mode::Video } else { Mode::Photo });
                    }
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        pane.window.add_controller(keys);
        pane
    }

    fn draw(&self, pane: &Pane, cr: &cairo::Context, width: f64, height: f64) {
        // this monitor's part of the picture
        let _ = cr.save();
        cr.scale(1.0 / self.scale, 1.0 / self.scale);
        let _ = cr.set_source_surface(&self.shot.surface, -pane.origin.0 * self.scale, -pane.origin.1 * self.scale);
        if self.scale == 1.0 {
            cr.source().set_filter(cairo::Filter::Nearest);
        }
        let _ = cr.paint();
        let _ = cr.restore();

        // darker everywhere but in the chosen area
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.4);
        let outline = pane.outline.borrow();
        if self.by_hand() && outline.len() > 1 {
            cr.set_fill_rule(cairo::FillRule::EvenOdd);
            cr.rectangle(0.0, 0.0, width, height);
            let trace = || {
                cr.move_to(outline[0].0, outline[0].1);
                for &(x, y) in &outline[1..] {
                    cr.line_to(x, y);
                }
                cr.close_path();
            };
            trace();
            let _ = cr.fill();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(1.5);
            cr.set_line_join(cairo::LineJoin::Round);
            trace();
            let _ = cr.stroke();
            return;
        }
        let Some(area) = pane.selection.get() else {
            let _ = cr.paint();
            return;
        };
        cr.set_fill_rule(cairo::FillRule::EvenOdd);
        cr.rectangle(0.0, 0.0, width, height);
        cr.rectangle(area.x, area.y, area.w, area.h);
        let _ = cr.fill();
        // a line around the area, red when it will be recorded
        match self.mode.get() {
            Mode::Photo => cr.set_source_rgb(1.0, 1.0, 1.0),
            Mode::Video => cr.set_source_rgb(RED.0, RED.1, RED.2),
        }
        cr.set_line_width(1.0);
        cr.rectangle(area.x - 0.5, area.y - 0.5, area.w + 1.0, area.h + 1.0);
        let _ = cr.stroke();

        // its size in pixels, under its bottom right corner (inside it if
        // there is no room below)
        let text = format!("{} × {}", (area.w * self.scale).round(), (area.h * self.scale).round());
        cr.select_font_face("Inter", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(12.0);
        let Ok(extents) = cr.text_extents(&text) else { return };
        let (w, h) = (extents.width() + 14.0, 22.0);
        let x = (area.x + area.w - w).max(0.0);
        let y = if area.y + area.h + 6.0 + h <= height { area.y + area.h + 6.0 } else { area.y + area.h - h - 6.0 };
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.7);
        rounded(cr, x, y, w, h, 7.0);
        let _ = cr.fill();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.move_to(x + 7.0 - extents.x_bearing(), y + 15.5);
        let _ = cr.show_text(&text);
    }

    pub fn mode(&self) -> Mode {
        self.mode.get()
    }

    pub fn set_mode(&self, mode: Mode) {
        self.mode.set(mode);
        self.show_mode();
    }

    /// Is the area drawn by hand? (A recording is always a rectangle.)
    fn by_hand(&self) -> bool {
        self.mode.get() == Mode::Photo && self.shape.get() == Shape::Free
    }

    fn show_mode(&self) {
        let video = self.mode.get() == Mode::Video;
        for pane in self.panes.borrow().iter() {
            pane.modes[usize::from(video)].set_active(true);
            pane.shapes[usize::from(self.by_hand())].set_active(true);
            pane.shapes[1].set_sensitive(!video);
            pane.hint.set_label(match (video, self.by_hand()) {
                (true, _) => "Drag over the area to record",
                (false, false) => "Drag over the area to capture",
                (false, true) => "Draw around what to capture",
            });
            pane.area.queue_draw();
        }
    }

    /// An area of a monitor was chosen, or all of it.
    fn choose(self: &Rc<Self>, pane: &Pane, area: Option<Rect>) {
        let area = area.unwrap_or(Rect { x: 0.0, y: 0.0, w: pane.size.0, h: pane.size.1 });
        self.finish(match self.mode.get() {
            Mode::Photo => {
                let in_picture = Rect { x: pane.origin.0 + area.x, y: pane.origin.1 + area.y, ..area };
                Choice::Photo(self.shot.part(in_picture, self.scale))
            }
            Mode::Video => Choice::Video { monitor: pane.monitor.clone(), area },
        });
    }

    /// Take the windows away and tell what was chosen. Not at once but when
    /// the click or key that chose it has been dealt with: a window that is
    /// closed, and another opened, in the middle of a click leaves GTK
    /// deaf to the next click.
    pub fn finish(self: &Rc<Self>, choice: Choice) {
        let Some(done) = self.done.borrow_mut().take() else { return };
        let overlay = self.clone();
        glib::idle_add_local_once(move || {
            for pane in overlay.panes.borrow_mut().drain(..) {
                pane.window.destroy();
            }
            // the screen is itself again before the picture is packed and saved
            if let Some(display) = gdk::Display::default() {
                display.flush();
            }
            done(&overlay.shot, choice);
        });
    }

    /// The first monitor's window (for timing the start, see main.rs).
    pub fn window(&self) -> Option<gtk::ApplicationWindow> {
        self.panes.borrow().first().map(|pane| pane.window.clone())
    }
}

pub fn rounded(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, PI + FRAC_PI_2);
    cr.close_path();
}
