//! Recording, the way the Windows Snipping Tool does it: the chosen area can
//! still be moved and resized; "Start" counts down from three and then
//! wf-recorder films the area into an MP4 file, until it is stopped (kept) or
//! thrown away.
//!
//! All of it is one window over the monitor. Before the film starts it takes
//! the pointer and the keys and darkens what is outside the area. While
//! filming it is see-through and lets the pointer through: a red line around
//! the area (just outside it, so it is not in the film) and a small strip with
//! the time and the buttons. With the strip's highlighter on, the pointer
//! draws on the screen instead, and what is drawn is in the film.
//!
//! wf-recorder gets a picture from the compositor only when something in the
//! area has changed. A film of a screen that stands still would have no
//! pictures at all, and every film would begin at the first change and end at
//! the last. So the window changes two pixels in the area's corner a few times
//! a second, by an amount nobody can see (the "pulse").

use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib::clone;
use gtk::prelude::*;
use gtk::{cairo, gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::canvas::{Stroke, Tool};
use crate::capture::{self, Rect};
use crate::overlay::{rounded, RED};
use crate::ui;

/// The count before the film starts.
const COUNT: u32 = 3;
/// Time for the last number to go before the film starts, so its first
/// pictures are of the screen as it is.
const SETTLE: Duration = Duration::from_millis(200);
/// How often the pulse beats: the least number of pictures a second.
const PULSE: Duration = Duration::from_millis(200);
/// How long wf-recorder gets to finish the file after it was told to stop.
const FINISH: Duration = Duration::from_secs(5);
/// wf-recorder ending by itself within this time never got going.
const EARLY: Duration = Duration::from_secs(3);
const SIGINT: i32 = 2;
/// How near the pointer must be to the area's edge to take hold of it, and
/// the smallest area that can be filmed.
const REACH: f64 = 9.0;
const SMALLEST: f64 = 32.0;

/// How a recording ended.
pub enum End {
    Saved(PathBuf),
    /// closed before it began, or thrown away
    Cancelled,
    Failed,
}

type Done = Box<dyn FnOnce(End)>;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// the area is shown and can be changed; waiting for "Start"
    Ready,
    /// counting down, with this number on the screen
    Counting(u32),
    Filming,
    /// told to stop; wf-recorder is finishing the file
    Ending,
}

/// What a drag does to the area before the film starts.
#[derive(Clone, Copy)]
enum Hold {
    /// moves these edges: left, right, top, bottom
    Edges(bool, bool, bool, bool),
    Move,
    /// draws a new area
    New,
}

pub struct Recorder {
    window: gtk::ApplicationWindow,
    canvas: gtk::DrawingArea,
    strip: gtk::Box,
    /// the strip's two faces: before the film and during it
    ready: gtk::Box,
    running: gtk::Box,
    sound: gtk::ToggleButton,
    time: gtk::Label,
    draw: gtk::ToggleButton,
    pulse: gtk::Box,
    /// the monitor, in the units of all monitors together
    screen: gdk::Rectangle,
    /// the filmed area, in the monitor's screen units
    area: Cell<Rect>,
    phase: Cell<Phase>,
    /// the drag that is changing the area, and the area as it was before
    hold: Cell<Option<(Hold, Rect)>>,
    strokes: RefCell<Vec<Stroke>>,
    file: PathBuf,
    /// record the sound too (given up if wf-recorder does not start with it)
    with_sound: Cell<bool>,
    recorder: RefCell<Option<gio::Subprocess>>,
    started: Cell<Option<Instant>>,
    /// the film is not wanted
    discard: Cell<bool>,
    done: RefCell<Option<Done>>,
}

impl Recorder {
    /// Show `area` of `monitor` ready to be filmed. `done` is called once,
    /// when the film is written, or when there will be none.
    pub fn open(
        app: &gtk::Application,
        monitor: &gdk::Monitor,
        area: Rect,
        done: impl FnOnce(End) + 'static,
    ) -> Option<Rc<Recorder>> {
        let file = capture::new_file(true)?;

        // the strip before the film: Start, sound or none, close
        let start = gtk::Button::builder()
            .label("Start")
            .tooltip_text("Start recording (Enter)")
            .css_classes(["start"])
            .can_focus(false)
            .build();
        let sound = gtk::ToggleButton::builder()
            .active(ui::remembered("sound").as_deref() != Some("off"))
            .css_classes(["quiet"])
            .can_focus(false)
            .build();
        let close = ui::icon_button("close", "Close (Esc)");
        let ready = gtk::Box::builder().spacing(4).build();
        ready.append(&start);
        ready.append(&sound);
        ready.append(&close);
        // and during it: the time, the highlighter, stop, throw away
        let time = gtk::Label::builder().label("0:00").css_classes(["time"]).xalign(0.0).build();
        let draw = ui::icon_toggle("marker", "Draw on the screen");
        let stop = ui::icon_button("stop", "Stop and save (Super+Shift+R)");
        stop.add_css_class("stop");
        let trash = ui::icon_button("trash", "Throw the recording away");
        let running = gtk::Box::builder().spacing(4).visible(false).build();
        running.append(&gtk::Box::builder().css_classes(["dot"]).valign(gtk::Align::Center).build());
        running.append(&time);
        running.append(&draw);
        running.append(&stop);
        running.append(&trash);
        let strip = gtk::Box::builder()
            .css_classes(["strip"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        strip.append(&ready);
        strip.append(&running);

        let pulse = gtk::Box::builder()
            .css_classes(["pulse"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .can_target(false)
            .build();
        let canvas = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        let layers = gtk::Overlay::builder().child(&canvas).build();
        layers.add_overlay(&pulse);
        layers.add_overlay(&strip);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Recording")
            .decorated(false)
            .css_classes(["shot-screen"])
            .child(&layers)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some("toros-shot"));
        window.set_layer(Layer::Overlay);
        window.set_monitor(Some(monitor));
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::Exclusive);

        let recorder = Rc::new(Recorder {
            window,
            canvas,
            strip,
            ready,
            running,
            sound,
            time,
            draw,
            pulse,
            screen: monitor.geometry(),
            area: Cell::new(area),
            phase: Cell::new(Phase::Ready),
            hold: Cell::new(None),
            strokes: RefCell::default(),
            file,
            with_sound: Cell::new(true),
            recorder: RefCell::default(),
            started: Cell::new(None),
            discard: Cell::new(false),
            done: RefCell::new(Some(Box::new(done))),
        });
        recorder.connect(&start, &close, &stop, &trash);
        recorder.show_sound();
        recorder.place_strip();
        recorder.window.present();
        Some(recorder)
    }

    fn connect(self: &Rc<Self>, start: &gtk::Button, close: &gtk::Button, stop: &gtk::Button, trash: &gtk::Button) {
        self.canvas.set_draw_func(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_, cr, width, height| recorder.paint(cr, f64::from(width), f64::from(height))
        ));
        start.connect_clicked(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_| recorder.count(COUNT)
        ));
        self.sound.connect_toggled(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |sound| {
                ui::remember("sound", if sound.is_active() { "on" } else { "off" });
                recorder.show_sound();
            }
        ));
        close.connect_clicked(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_| recorder.stop()
        ));
        stop.connect_clicked(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_| recorder.stop()
        ));
        trash.connect_clicked(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_| {
                recorder.discard.set(true);
                recorder.stop();
            }
        ));

        // The highlighter: while it is on, the pointer draws; switched off,
        // what was drawn goes and the pointer reaches the windows again.
        self.draw.connect_toggled(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |draw| {
                if !draw.is_active() {
                    recorder.strokes.borrow_mut().clear();
                    recorder.canvas.queue_draw();
                }
                recorder.canvas.set_cursor_from_name(draw.is_active().then_some("crosshair"));
                recorder.pass_pointer();
            }
        ));

        // A drag changes the area before the film, and draws during it
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_, x, y| match recorder.phase.get() {
                Phase::Ready => {
                    recorder.hold.set(Some((recorder.hold_at(x, y), recorder.area.get())));
                    recorder.strip.set_visible(false);
                }
                Phase::Filming => {
                    recorder.strokes.borrow_mut().push(Stroke::new(Tool::Marker, 0, (x, y)));
                    recorder.canvas.queue_draw();
                }
                _ => {}
            }
        ));
        drag.connect_drag_update(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |drag, dx, dy| {
                let Some((x, y)) = drag.start_point() else { return };
                match (recorder.phase.get(), recorder.hold.get()) {
                    (Phase::Ready, Some((hold, before))) => {
                        recorder.area.set(recorder.changed(hold, before, (x, y), (dx, dy)));
                    }
                    (Phase::Filming, _) => {
                        if let Some(stroke) = recorder.strokes.borrow_mut().last_mut() {
                            stroke.points.push((x + dx, y + dy));
                        }
                    }
                    _ => return,
                }
                recorder.canvas.queue_draw();
            }
        ));
        drag.connect_drag_end(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_, _, _| {
                let Some((_, before)) = recorder.hold.take() else { return };
                let area = recorder.area.get();
                if area.w < SMALLEST || area.h < SMALLEST {
                    recorder.area.set(before);
                }
                if recorder.phase.get() == Phase::Ready {
                    recorder.strip.set_visible(true);
                    recorder.place_strip();
                }
                recorder.canvas.queue_draw();
            }
        ));
        self.canvas.add_controller(drag);

        // the pointer shows what a drag would do
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(clone!(
            #[weak(rename_to = recorder)]
            self,
            move |_, x, y| {
                if recorder.phase.get() != Phase::Ready || recorder.hold.get().is_some() {
                    return;
                }
                recorder.canvas.set_cursor_from_name(Some(match recorder.hold_at(x, y) {
                    Hold::Edges(true, _, true, _) | Hold::Edges(_, true, _, true) => "nwse-resize",
                    Hold::Edges(true, _, _, true) | Hold::Edges(_, true, true, _) => "nesw-resize",
                    Hold::Edges(true, ..) | Hold::Edges(_, true, ..) => "ew-resize",
                    Hold::Edges(..) => "ns-resize",
                    Hold::Move => "move",
                    Hold::New => "crosshair",
                }));
            }
        ));
        self.canvas.add_controller(motion);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(clone!(
            #[weak(rename_to = recorder)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                match key {
                    gdk::Key::Escape => recorder.stop(),
                    gdk::Key::Return | gdk::Key::KP_Enter if recorder.phase.get() == Phase::Ready => {
                        recorder.count(COUNT);
                    }
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        self.window.add_controller(keys);

        // a window that is closed some other way ends the film too
        self.window.connect_close_request(clone!(
            #[weak(rename_to = recorder)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_| {
                recorder.stop();
                glib::Propagation::Stop
            }
        ));
    }

    fn paint(&self, cr: &cairo::Context, width: f64, height: f64) {
        let area = self.area.get();
        if self.phase.get() == Phase::Filming {
            cr.set_source_rgba(RED.0, RED.1, RED.2, 0.9);
            cr.set_line_width(2.0);
            cr.rectangle(area.x - 2.0, area.y - 2.0, area.w + 4.0, area.h + 4.0);
            let _ = cr.stroke();
            for stroke in self.strokes.borrow().iter() {
                stroke.draw(cr, false);
            }
            return;
        }
        if self.phase.get() == Phase::Ending {
            return;
        }

        // before the film: darker outside the area, a line around it
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.4);
        cr.set_fill_rule(cairo::FillRule::EvenOdd);
        cr.rectangle(0.0, 0.0, width, height);
        cr.rectangle(area.x, area.y, area.w, area.h);
        let _ = cr.fill();
        cr.set_source_rgb(RED.0, RED.1, RED.2);
        cr.set_line_width(2.0);
        cr.rectangle(area.x - 1.0, area.y - 1.0, area.w + 2.0, area.h + 2.0);
        let _ = cr.stroke();
        match self.phase.get() {
            // knobs to take hold of, on the corners and the middles of the sides
            Phase::Ready => {
                for (fx, fy) in [(0.0, 0.0), (0.5, 0.0), (1.0, 0.0), (1.0, 0.5), (1.0, 1.0), (0.5, 1.0), (0.0, 1.0), (0.0, 0.5)] {
                    cr.arc(area.x + area.w * fx, area.y + area.h * fy, 4.5, 0.0, std::f64::consts::TAU);
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    let _ = cr.fill_preserve();
                    cr.set_source_rgb(RED.0, RED.1, RED.2);
                    cr.set_line_width(1.5);
                    let _ = cr.stroke();
                }
            }
            // the count, in the middle of the area
            Phase::Counting(number) => {
                let (x, y) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.65);
                rounded(cr, x - 46.0, y - 46.0, 92.0, 92.0, 46.0);
                let _ = cr.fill();
                let text = number.to_string();
                cr.select_font_face("Inter", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
                cr.set_font_size(52.0);
                if let Ok(extents) = cr.text_extents(&text) {
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                    cr.move_to(x - extents.width() / 2.0 - extents.x_bearing(), y - extents.height() / 2.0 - extents.y_bearing());
                    let _ = cr.show_text(&text);
                }
            }
            _ => {}
        }
    }

    /// What a drag that begins here would take hold of.
    fn hold_at(&self, x: f64, y: f64) -> Hold {
        let area = self.area.get();
        let beside = y >= area.y - REACH && y <= area.y + area.h + REACH;
        let above = x >= area.x - REACH && x <= area.x + area.w + REACH;
        let left = beside && (x - area.x).abs() <= REACH;
        let right = beside && !left && (x - area.x - area.w).abs() <= REACH;
        let top = above && (y - area.y).abs() <= REACH;
        let bottom = above && !top && (y - area.y - area.h).abs() <= REACH;
        if left || right || top || bottom {
            Hold::Edges(left, right, top, bottom)
        } else if beside && above {
            Hold::Move
        } else {
            Hold::New
        }
    }

    /// The area after a drag from `from` by `by`.
    fn changed(&self, hold: Hold, before: Rect, from: (f64, f64), by: (f64, f64)) -> Rect {
        let (width, height) = (f64::from(self.screen.width()), f64::from(self.screen.height()));
        let add = |on: bool, d: f64| if on { d } else { 0.0 };
        match hold {
            Hold::Edges(left, right, top, bottom) => Rect::between(
                (before.x + add(left, by.0), before.y + add(top, by.1)),
                (before.x + before.w + add(right, by.0), before.y + before.h + add(bottom, by.1)),
                width,
                height,
            ),
            Hold::Move => Rect {
                x: (before.x + by.0).clamp(0.0, width - before.w).round(),
                y: (before.y + by.1).clamp(0.0, height - before.h).round(),
                ..before
            },
            Hold::New => Rect::between(from, (from.0 + by.0, from.1 + by.1), width, height),
        }
    }

    /// Put the strip under the area, or over it; only when the area leaves no
    /// room for either is it inside, and then it is in the film. (The strip
    /// must be visible: a hidden widget measures as nothing. And a widget's
    /// measure has its margins in it, which are how the strip is placed.)
    fn place_strip(&self) {
        let area = self.area.get();
        let (width, height) = (f64::from(self.screen.width()), f64::from(self.screen.height()));
        self.strip.set_margin_start(0);
        self.strip.set_margin_top(0);
        let (_, strip_w, _, _) = self.strip.measure(gtk::Orientation::Horizontal, -1);
        let (_, strip_h, _, _) = self.strip.measure(gtk::Orientation::Vertical, -1);
        let (strip_w, strip_h) = (f64::from(strip_w), f64::from(strip_h));
        let x = (area.x + (area.w - strip_w) / 2.0).clamp(8.0, (width - strip_w - 8.0).max(8.0));
        let y = if area.y + area.h + 12.0 + strip_h + 8.0 <= height {
            area.y + area.h + 12.0
        } else if area.y - 12.0 - strip_h >= 8.0 {
            area.y - 12.0 - strip_h
        } else {
            area.y + 16.0
        };
        self.strip.set_margin_start(x.round() as i32);
        self.strip.set_margin_top(y.round() as i32);
        self.pulse.set_margin_start(area.x as i32);
        self.pulse.set_margin_top(area.y as i32);
    }

    fn show_sound(&self) {
        let on = self.sound.is_active();
        self.sound.set_child(Some(&ui::icon(if on { "sound" } else { "muted" })));
        self.sound.set_tooltip_text(Some(if on { "The laptop's sound is recorded" } else { "No sound is recorded" }));
    }

    /// Let the pointer through the window, except on the strip. Before the
    /// film, and while the highlighter is on, all of the window takes it.
    fn pass_pointer(&self) {
        let Some(surface) = self.window.surface() else { return };
        let all = cairo::RectangleInt::new(0, 0, self.window.width().max(1), self.window.height().max(1));
        let strip = cairo::RectangleInt::new(
            self.strip.margin_start(),
            self.strip.margin_top(),
            self.strip.width().max(1),
            self.strip.height().max(1),
        );
        let through = self.phase.get() == Phase::Filming && !self.draw.is_active();
        surface.set_input_region(Some(&cairo::Region::create_rectangle(if through { &strip } else { &all })));
        self.window.queue_draw();
    }

    /// Count down on the screen, then film.
    fn count(self: &Rc<Self>, number: u32) {
        if !matches!(self.phase.get(), Phase::Ready | Phase::Counting(_)) {
            return; // closed meanwhile
        }
        if number > 0 {
            self.phase.set(Phase::Counting(number));
            self.strip.set_visible(false);
            self.canvas.set_cursor_from_name(None);
            self.canvas.queue_draw();
            glib::timeout_add_local_once(
                Duration::from_secs(1),
                clone!(
                    #[strong(rename_to = recorder)]
                    self,
                    move || {
                        if recorder.phase.get() == Phase::Counting(number) {
                            recorder.count(number - 1);
                        }
                    }
                ),
            );
            return;
        }
        // The film begins. The encoder takes even sizes only.
        let area = self.area.get();
        self.area.set(Rect { w: (area.w / 2.0).floor() * 2.0, h: (area.h / 2.0).floor() * 2.0, ..area });
        self.phase.set(Phase::Filming);
        self.with_sound.set(self.sound.is_active());
        self.ready.set_visible(false);
        self.running.set_visible(true);
        self.strip.add_css_class("on");
        self.strip.set_visible(true);
        self.place_strip();
        self.window.set_keyboard_mode(KeyboardMode::None);
        self.canvas.queue_draw();
        glib::timeout_add_local_once(
            SETTLE,
            clone!(
                #[strong(rename_to = recorder)]
                self,
                move || recorder.film()
            ),
        );
    }

    /// Start wf-recorder.
    fn film(self: &Rc<Self>) {
        if self.phase.get() != Phase::Filming {
            return; // stopped before it began
        }
        // (the strip has its size now)
        self.pass_pointer();
        let area = self.area.get();
        let geometry = format!(
            "{},{} {}x{}",
            self.screen.x() + area.x as i32,
            self.screen.y() + area.y as i32,
            area.w as i32,
            area.h as i32
        );
        // H.264 in the pixel format every player and browser takes, with the
        // sound the laptop is playing
        let mut command: Vec<&OsStr> =
            vec!["wf-recorder".as_ref(), "-g".as_ref(), geometry.as_ref(), "-x".as_ref(), "yuv420p".as_ref()];
        if self.with_sound.get() {
            command.push("--audio=@DEFAULT_MONITOR@".as_ref());
        }
        command.extend(["-f".as_ref(), self.file.as_os_str()]);
        let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDOUT_SILENCE);
        // what it says goes to ~/.local/state/toros/shot/recorder.log
        let state = ui::state_dir();
        let _ = fs::create_dir_all(&state);
        launcher.set_stderr_file_path(Some(&state.join("recorder.log")));
        let Ok(process) = launcher.spawn(&command) else { return self.end(End::Failed) };
        let started = Instant::now();
        process.wait_async(
            gio::Cancellable::NONE,
            clone!(
                #[strong(rename_to = recorder)]
                self,
                move |_| {
                    // It ends when it is told to (the film is whole then), or
                    // by itself when something went wrong. If that was at
                    // once, the sound may be what it could not get: once more
                    // without.
                    let stopped = recorder.phase.get() == Phase::Ending;
                    let written = fs::metadata(&recorder.file).is_ok_and(|file| file.len() > 0);
                    if stopped && recorder.discard.get() {
                        let _ = fs::remove_file(&recorder.file);
                        recorder.end(End::Cancelled);
                    } else if stopped && written {
                        recorder.end(End::Saved(recorder.file.clone()));
                    } else if !stopped && started.elapsed() < EARLY && recorder.with_sound.replace(false) {
                        let _ = fs::remove_file(&recorder.file);
                        recorder.film();
                    } else {
                        recorder.end(End::Failed);
                    }
                }
            ),
        );
        self.recorder.replace(Some(process));
        if self.started.replace(Some(started)).is_some() {
            return; // the clock runs already
        }
        glib::timeout_add_local(
            PULSE,
            clone!(
                #[weak(rename_to = recorder)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    let Some(started) = recorder.started.get() else { return glib::ControlFlow::Break };
                    if recorder.pulse.has_css_class("beat") {
                        recorder.pulse.remove_css_class("beat");
                    } else {
                        recorder.pulse.add_css_class("beat");
                    }
                    let seconds = started.elapsed().as_secs();
                    let text = format!("{}:{:02}", seconds / 60, seconds % 60);
                    if recorder.time.label() != text {
                        recorder.time.set_label(&text);
                    }
                    glib::ControlFlow::Continue
                }
            ),
        );
    }

    /// Before the film has begun: close. During it: end it; `done` is called
    /// when wf-recorder has written the file.
    pub fn stop(&self) {
        let process = self.recorder.borrow().clone();
        match (self.phase.get(), process) {
            (Phase::Ending, _) => {}
            (Phase::Filming, Some(process)) => {
                self.phase.set(Phase::Ending);
                self.started.set(None);
                self.window.set_visible(false);
                process.send_signal(SIGINT);
                // (it has been seen to hang instead; the film is then lost)
                glib::timeout_add_local_once(FINISH, move || process.force_exit());
            }
            _ => {
                self.phase.set(Phase::Ending);
                self.end(End::Cancelled);
            }
        }
    }

    fn end(&self, end: End) {
        self.started.set(None);
        self.recorder.replace(None);
        self.window.destroy();
        let done = self.done.borrow_mut().take();
        if let Some(done) = done {
            done(end);
        }
    }
}
