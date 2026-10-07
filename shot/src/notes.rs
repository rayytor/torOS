//! Notifications that can be clicked: "Screenshot copied", then a click on it
//! opens the editor. They go to the desktop's notification service (mako) over
//! D-Bus, which also tells when one was clicked or went away.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use gtk::gio;
use gtk::glib::{self, prelude::*};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// Called once: with true when the notification was clicked, with false when
/// it went away without a click.
type Answer = Box<dyn FnOnce(bool)>;

pub struct Notes {
    bus: gio::DBusConnection,
    waiting: Rc<RefCell<HashMap<u32, Answer>>>,
}

impl Notes {
    pub fn connect() -> Option<Notes> {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
        let waiting: Rc<RefCell<HashMap<u32, Answer>>> = Rc::default();
        for (signal, clicked) in [("ActionInvoked", true), ("NotificationClosed", false)] {
            let waiting = waiting.clone();
            // (the newer call stops listening when its handle is dropped; this
            // one listens for as long as the program runs, which is what is wanted)
            #[allow(deprecated)]
            bus.signal_subscribe(
                Some(NAME),
                Some(NAME),
                Some(signal),
                Some(PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |_, _, _, _, _, arguments| {
                    let Some(id) = arguments.try_child_value(0).and_then(|id| id.get::<u32>()) else { return };
                    // (taken out first: the answer may show the next notification)
                    let answer = waiting.borrow_mut().remove(&id);
                    if let Some(answer) = answer {
                        answer(clicked);
                    }
                },
            );
        }
        Some(Notes { bus, waiting })
    }

    /// Show a notification, with `picture` beside the text. `answer` is called
    /// when it is clicked or gone. Returns its number, or `None` (and `answer`
    /// is never called) if there is no notification service.
    pub fn show(&self, title: &str, text: &str, picture: Option<&Path>, answer: impl FnOnce(bool) + 'static) -> Option<u32> {
        let hints = glib::VariantDict::new(None);
        if let Some(picture) = picture.and_then(Path::to_str) {
            hints.insert_value("image-path", &picture.to_variant());
        }
        // a click anywhere on it is its "default" action
        let actions = ["default", "Open"];
        let message = glib::Variant::tuple_from_iter([
            "Screenshot".to_variant(),
            0u32.to_variant(),
            "".to_variant(),
            title.to_variant(),
            text.to_variant(),
            actions.as_slice().to_variant(),
            hints.end(),
            (-1i32).to_variant(),
        ]);
        let reply = self
            .bus
            .call_sync(
                Some(NAME),
                PATH,
                NAME,
                "Notify",
                Some(&message),
                None,
                gio::DBusCallFlags::NONE,
                2000,
                gio::Cancellable::NONE,
            )
            .ok()?;
        let id = reply.try_child_value(0)?.get::<u32>()?;
        self.waiting.borrow_mut().insert(id, Box::new(answer));
        Some(id)
    }

    /// Take away every notification that is still shown, so that the next
    /// picture of the screen is not a picture of them. True if there was one.
    pub fn clear(&self) -> bool {
        let shown: Vec<u32> = self.waiting.borrow().keys().copied().collect();
        for id in &shown {
            self.bus.call(
                Some(NAME),
                PATH,
                NAME,
                "CloseNotification",
                Some(&(*id,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                2000,
                gio::Cancellable::NONE,
                |_| {},
            );
        }
        !shown.is_empty()
    }
}
