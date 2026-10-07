//! The open windows, as the compositor tells them to panels and docks.
//!
//! Two lists are read. wlr-foreign-toplevel-management knows which window is
//! in use and can bring one to the front or close it; ext-foreign-toplevel-list
//! gives each window the name that grim takes its picture by. The compositor
//! announces a window on both in the same order, so the n-th window with a
//! given title and application on one list is the n-th on the other.
//!
//! This is a connection of its own beside GTK's, read from a timer while the
//! overview is open (see `Ui::watch` in main.rs).

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{delegate_noop, event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self as named, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self as names, ExtForeignToplevelListV1},
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self as handle, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self as manager, ZwlrForeignToplevelManagerV1},
};

/// One open window.
#[derive(Clone, Debug)]
pub struct Window {
    /// what `Windows::activate` and `close` know it by
    pub key: u32,
    pub title: String,
    pub app_id: String,
    pub minimized: bool,
    /// the name grim takes its picture by, when the compositor gives one
    pub capture: Option<String>,
}

/// What happened to the windows since the last look.
#[derive(Debug, PartialEq)]
pub enum Change {
    /// a window was closed
    Closed(u32),
    /// a window was opened, or another one came to the front: the overview is
    /// no longer what the user is looking at
    Moved,
}

struct Top {
    handle: ZwlrForeignToplevelHandleV1,
    title: String,
    app_id: String,
    active: bool,
    minimized: bool,
    /// a dialog of another window
    child: bool,
    /// its first description is complete
    known: bool,
}

struct Named {
    handle: ExtForeignToplevelHandleV1,
    id: String,
    title: String,
    app_id: String,
}

#[derive(Default)]
struct State {
    tops: Vec<Top>,
    named: Vec<Named>,
    /// the first description of every window has been read
    listed: bool,
    changes: Vec<Change>,
}

pub struct Windows {
    conn: Connection,
    queue: EventQueue<State>,
    state: State,
    seat: Option<WlSeat>,
}

impl Windows {
    /// Ask the compositor for its windows. `None` where it does not tell
    /// (not a wlroots compositor): the overview then shows applications only.
    pub fn connect() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
        let handle = queue.handle();
        let _manager: ZwlrForeignToplevelManagerV1 = globals.bind(&handle, 1..=3, ()).ok()?;
        let _names: Option<ExtForeignToplevelListV1> = globals.bind(&handle, 1..=1, ()).ok();
        let seat: Option<WlSeat> = globals.bind(&handle, 1..=1, ()).ok();
        let mut state = State::default();
        // the windows are announced, then described
        queue.roundtrip(&mut state).ok()?;
        queue.roundtrip(&mut state).ok()?;
        state.listed = true;
        state.changes.clear();
        Some(Windows { conn, queue, state, seat })
    }

    /// The windows, oldest first, without dialogs.
    pub fn list(&self) -> Vec<Window> {
        let same = |a: (&str, &str), b: (&str, &str)| a == b;
        self.state
            .tops
            .iter()
            .enumerate()
            .filter(|(_, top)| top.known && !top.child)
            .map(|(at, top)| {
                let this = (top.app_id.as_str(), top.title.as_str());
                let nth = self.state.tops[..at].iter().filter(|t| same((&t.app_id, &t.title), this)).count();
                let capture = self
                    .state
                    .named
                    .iter()
                    .filter(|n| same((&n.app_id, &n.title), this))
                    .nth(nth)
                    .map(|n| n.id.clone())
                    .filter(|id| !id.is_empty());
                Window {
                    key: top.handle.id().protocol_id(),
                    title: top.title.clone(),
                    app_id: top.app_id.clone(),
                    minimized: top.minimized,
                    capture,
                }
            })
            .collect()
    }

    fn top(&self, key: u32) -> Option<&Top> {
        self.state.tops.iter().find(|t| t.handle.id().protocol_id() == key)
    }

    /// Bring a window to the front (and back from being minimised).
    pub fn activate(&mut self, key: u32) {
        if let (Some(top), Some(seat)) = (self.top(key), &self.seat) {
            if top.minimized {
                top.handle.unset_minimized();
            }
            top.handle.activate(seat);
        }
        let _ = self.queue.roundtrip(&mut self.state);
    }

    /// Ask a window to close, as its own close button does.
    pub fn close(&mut self, key: u32) {
        if let Some(top) = self.top(key) {
            top.handle.close();
        }
        let _ = self.conn.flush();
    }

    /// Read what the compositor has sent since the last call, without waiting.
    pub fn changes(&mut self) -> Vec<Change> {
        let _ = self.conn.flush();
        if let Some(guard) = self.conn.prepare_read() {
            // (nothing there to read is the usual case, and no error)
            let _ = guard.read();
        }
        let _ = self.queue.dispatch_pending(&mut self.state);
        std::mem::take(&mut self.state.changes)
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let manager::Event::Toplevel { toplevel } = event {
            state.tops.push(Top {
                handle: toplevel,
                title: String::new(),
                app_id: String::new(),
                active: false,
                minimized: false,
                child: false,
                known: false,
            });
        }
    }

    event_created_child!(State, ZwlrForeignToplevelManagerV1, [
        manager::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(at) = state.tops.iter().position(|t| &t.handle == proxy) else { return };
        match event {
            handle::Event::Title { title } => state.tops[at].title = title,
            handle::Event::AppId { app_id } => state.tops[at].app_id = app_id,
            handle::Event::Parent { parent } => state.tops[at].child = parent.is_some(),
            handle::Event::State { state: flags } => {
                let has = |flag: handle::State| {
                    flags.as_chunks::<4>().0.iter().any(|f| u32::from_ne_bytes(*f) == flag as u32)
                };
                let active = has(handle::State::Activated);
                if active && !state.tops[at].active && state.listed {
                    state.changes.push(Change::Moved);
                }
                state.tops[at].active = active;
                state.tops[at].minimized = has(handle::State::Minimized);
            }
            handle::Event::Done => {
                if !state.tops[at].known && state.listed && !state.tops[at].child {
                    state.changes.push(Change::Moved);
                }
                state.tops[at].known = true;
            }
            handle::Event::Closed => {
                let top = state.tops.remove(at);
                state.changes.push(Change::Closed(top.handle.id().protocol_id()));
                top.handle.destroy();
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: names::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let names::Event::Toplevel { toplevel } = event {
            state.named.push(Named { handle: toplevel, id: String::new(), title: String::new(), app_id: String::new() });
        }
    }

    event_created_child!(State, ExtForeignToplevelListV1, [
        names::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: named::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(at) = state.named.iter().position(|n| &n.handle == proxy) else { return };
        match event {
            named::Event::Identifier { identifier } => state.named[at].id = identifier,
            named::Event::Title { title } => state.named[at].title = title,
            named::Event::AppId { app_id } => state.named[at].app_id = app_id,
            named::Event::Closed => state.named.remove(at).handle.destroy(),
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_noop!(State: ignore WlSeat);
