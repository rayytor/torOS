//! toros-shot: screenshots and screen recordings for torOS.
//!
//!   toros-shot              freeze the screen and drag out an area (Print,
//!                           Super+Shift+S); Enter takes the whole screen
//!   toros-shot screen       the whole screen, at once (Super+Print, Shift+Print)
//!   toros-shot record       the same frozen screen, set to recording: drag out
//!                           the area to film, then "Start" (Super+Shift+R).
//!                           Run again while it films, it stops.
//!   toros-shot edit FILE    draw on a picture that is already there
//!   --delay                 wait a moment first (for the menu or panel the
//!                           command was started from to close)
//!
//! A screenshot is on the clipboard and in Pictures/Screenshots as soon as it
//! is taken. A click on the notification that says so opens it in the editor,
//! to highlight or draw on; what is drawn replaces the file and the clipboard
//! by itself. A recording goes to Videos/Recordings as MP4 (wf-recorder).
//!
//! The program starts when asked and ends when its last window and
//! notification are gone. While it runs, the commands above go to it.

mod canvas;
mod capture;
mod editor;
mod notes;
mod overlay;
mod recorder;
mod ui;

use std::cell::RefCell;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::Duration;
use std::{env, fs};

use gtk::cairo::ImageSurface;
use gtk::gio::{self, ApplicationHoldGuard};
use gtk::glib::{self, clone};
use gtk::prelude::*;

use capture::Shot;
use editor::Editor;
use notes::Notes;
use overlay::{Choice, Mode, Overlay};
use recorder::{End, Recorder};

const APP_ID: &str = "org.toros.Shot";
/// Time for a menu to close before the picture is taken (`--delay`).
const DELAY: Duration = Duration::from_millis(350);
/// Time for the notification of the screenshot before to go.
const CLEAR: Duration = Duration::from_millis(150);

struct App {
    app: gtk::Application,
    /// grim, started before GTK was, with the first picture
    early: RefCell<Option<Child>>,
    notes: RefCell<Option<Rc<Notes>>>,
    overlay: RefCell<Option<Rc<Overlay>>>,
    recorder: RefCell<Option<Rc<Recorder>>>,
}

/// Does this command begin with a picture of the screen as it is now?
fn starts_with_picture(words: &[String]) -> bool {
    !words.iter().any(|w| w == "--delay") && matches!(words.first().map(String::as_str), None | Some("region" | "screen"))
}

impl App {
    fn command(self: &Rc<Self>, words: &[String]) {
        ui::load_style();
        let delay = words.iter().any(|w| w == "--delay");
        let mut words = words.iter().map(String::as_str).filter(|w| *w != "--delay");
        match words.next() {
            Some("edit") => match words.next() {
                Some(file) => self.edit(Path::new(file)),
                None => eprintln!("toros-shot edit: which file?"),
            },
            Some("screen") => self.grab(delay, |app, shot| app.photo(shot.cut(shot.all()))),
            Some("record") => {
                // (the recorder is taken out first: stopping may end it at once)
                let recorder = self.recorder.borrow().clone();
                match recorder {
                    Some(recorder) => recorder.stop(),
                    None => self.choose(Mode::Video, delay),
                }
            }
            _ => self.choose(Mode::Photo, delay),
        }
    }

    /// Show the frozen screen. If it is shown already: the same key closes it,
    /// the other key changes what it is for.
    fn choose(self: &Rc<Self>, mode: Mode, delay: bool) {
        let open = self.overlay.borrow().clone();
        if let Some(overlay) = open {
            if overlay.mode() == mode {
                overlay.finish(Choice::Cancelled);
            } else {
                overlay.set_mode(mode);
            }
            return;
        }
        self.grab(delay, move |app, shot| {
            let overlay = Overlay::open(
                &app.app,
                shot,
                mode,
                clone!(
                    #[weak]
                    app,
                    move |shot, choice| {
                        app.overlay.replace(None);
                        match choice {
                            Choice::Cancelled => {}
                            Choice::Photo(part) => app.photo(shot.cut(part)),
                            Choice::Shape(outline) => app.photo(shot.cut_shape(&outline)),
                            Choice::Video { monitor, area } => app.record(&monitor, area),
                        }
                    }
                ),
            );
            // for timing the start (see /usr/lib/toros/selftest): leave as
            // soon as the first picture of the frozen screen is drawn
            if env::var_os("TOROS_SHOT_BENCH").is_some() {
                if let Some(clock) = overlay.window().and_then(|w| w.frame_clock()) {
                    let app = app.app.clone();
                    clock.connect_after_paint(move |_| app.quit());
                }
            }
            app.overlay.replace(Some(overlay));
        });
    }

    /// Take a picture of the screen and go on with it.
    fn grab(self: &Rc<Self>, delay: bool, then: impl FnOnce(&Rc<App>, Shot) + 'static) {
        let early = self.early.take();
        // our own notification must not be in the picture
        let cleared = self.notes.borrow().as_ref().is_some_and(|notes| notes.clear());
        let mut wait = Duration::ZERO;
        if early.is_none() {
            if delay {
                wait += DELAY;
            }
            if cleared {
                wait += CLEAR;
            }
        }
        let hold = self.app.hold();
        let go = clone!(
            #[strong(rename_to = app)]
            self,
            move || {
                let _hold = hold;
                match early.or_else(capture::grab).and_then(capture::finish) {
                    Some(shot) => then(&app, shot),
                    None => app.tell("Screenshot failed", "The screen could not be captured.", None, |_| {}),
                }
            }
        );
        if wait.is_zero() {
            go();
        } else {
            glib::timeout_add_local_once(wait, go);
        }
    }

    /// A part of the frozen screen becomes a screenshot: copied, saved, and
    /// one click away from the editor.
    fn photo(self: &Rc<Self>, picture: Option<ImageSurface>) {
        let Some(picture) = picture else { return };
        let Some(png) = capture::png(&picture) else { return };
        let file = capture::new_file(false).filter(|file| fs::write(file, &png).is_ok());
        let copied = capture::copy(&png);
        let title = if copied { "Screenshot copied" } else { "Screenshot taken" };
        let text = match file.as_deref().and_then(Path::parent) {
            Some(folder) => format!("Click to highlight or draw on it.\nSaved in {}.", capture::shown(folder)),
            None => "Click to highlight or draw on it.\nIt could not be saved.".to_string(),
        };
        let app = self.app.clone();
        let saved = file.clone();
        self.tell(title, &text, saved.as_deref(), move |clicked| {
            if clicked {
                Editor::open(&app, picture, file);
            }
        });
    }

    fn record(self: &Rc<Self>, monitor: &gtk::gdk::Monitor, area: capture::Rect) {
        let hold = self.app.hold();
        let recorder = Recorder::open(
            &self.app,
            monitor,
            area,
            clone!(
                #[strong(rename_to = app)]
                self,
                move |end| {
                    let _hold = hold;
                    app.recorder.replace(None);
                    match end {
                        End::Saved(file) => {
                            let folder = file.parent().map(capture::shown).unwrap_or_default();
                            let text = format!("Click to show it in Files.\nSaved in {folder}.");
                            app.tell("Recording saved", &text, None, move |clicked| {
                                if clicked {
                                    show_in_files(&file);
                                }
                            });
                        }
                        End::Cancelled => {}
                        End::Failed => app.tell(
                            "Recording failed",
                            "The screen could not be recorded. See ~/.local/state/toros/shot/recorder.log.",
                            None,
                            |_| {},
                        ),
                    }
                }
            ),
        );
        match recorder {
            Some(recorder) => {
                self.recorder.replace(Some(recorder));
            }
            None => self.tell("Recording failed", "No file could be made in Videos/Recordings.", None, |_| {}),
        }
    }

    fn edit(self: &Rc<Self>, file: &Path) {
        let picture = fs::File::open(file).ok().and_then(|mut f| ImageSurface::create_from_png(&mut f).ok());
        match picture {
            Some(picture) => {
                Editor::open(&self.app, picture, Some(file.to_path_buf()));
            }
            None => eprintln!("toros-shot edit: {} is not a PNG picture", file.display()),
        }
    }

    /// Show a notification; the program stays until it is answered.
    fn tell(self: &Rc<Self>, title: &str, text: &str, picture: Option<&Path>, answer: impl FnOnce(bool) + 'static) {
        let known = self.notes.borrow().clone();
        let notes = known.or_else(|| {
            let notes = Notes::connect().map(Rc::new);
            self.notes.replace(notes.clone());
            notes
        });
        let hold: ApplicationHoldGuard = self.app.hold();
        notes.and_then(|notes| {
            notes.show(title, text, picture, move |clicked| {
                let _hold = hold;
                answer(clicked);
            })
        });
    }
}

/// Open Files on the folder a file is in, with the file selected.
fn show_in_files(file: &Path) {
    let _ = Command::new("toros-files")
        .arg("--select")
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn main() -> glib::ExitCode {
    // The picture is taken first of all, while GTK starts: what is on the
    // screen when the key is pressed is what gets frozen.
    let words: Vec<String> = env::args().skip(1).collect();
    let early = if starts_with_picture(&words) { capture::grab() } else { None };

    gio::resources_register_include!("icons.gresource").expect("the icons are part of the program");
    // The session draws GTK apps with OpenGL. A picture of the screen with a
    // few lines on it is drawn as fast in software, and the window is there
    // sooner.
    env::set_var("GSK_RENDERER", env::var("TOROS_SHOT_RENDERER").unwrap_or("cairo".to_string()));

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let state = Rc::new(App {
        app: app.clone(),
        early: RefCell::new(early),
        notes: RefCell::default(),
        overlay: RefCell::default(),
        recorder: RefCell::default(),
    });
    // Runs in the program that is already there when the command is run again
    app.connect_command_line(clone!(
        #[strong]
        state,
        move |_, command| {
            let words: Vec<String> =
                command.arguments().iter().skip(1).map(|w| w.to_string_lossy().into_owned()).collect();
            state.command(&words);
            glib::ExitCode::SUCCESS
        }
    ));
    // If it is: that one takes the picture, this one's is not needed
    if app.register(gio::Cancellable::NONE).is_ok() && app.is_remote() {
        if let Some(mut grim) = state.early.take() {
            let _ = grim.kill();
            let _ = grim.wait();
        }
    }
    app.run()
}
