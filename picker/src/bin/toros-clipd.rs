//! toros-clipd: keeps the clipboard history that toros-picker shows.
//!
//! Started once from labwc's autostart. It asks the compositor to be told
//! about every change of the clipboard (wlr-data-control), reads what was
//! copied and files it (see store.rs). It has no window and does not link
//! GTK, so it costs about a megabyte while the desktop runs.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::sync::mpsc;
use std::time::Duration;
use std::{io, process, thread};

use toros_picker::store::{self, Kind};
use wayland_client::backend::ObjectId;
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{delegate_noop, event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self as device, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self as offer, ZwlrDataControlOfferV1},
};

const TEXT_TYPES: [&str; 5] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "TEXT", "STRING"];
const IMAGE_TYPES: [(&str, Kind); 2] = [("image/png", Kind::Png), ("image/jpeg", Kind::Jpeg)];
/// Password managers mark what they copy with this type.
const SECRET_TYPE: &str = "x-kde-passwordManagerHint";
/// How long the program that owns the clipboard gets to hand over the data.
const READ_TIMEOUT: Duration = Duration::from_secs(3);

/// The types each clipboard offer was announced with, until it is used.
#[derive(Default)]
struct State {
    offers: HashMap<ObjectId, Vec<String>>,
}

/// What to keep of an offer: text if there is any, otherwise a picture.
fn choose(types: &[String]) -> Option<(&'static str, Kind)> {
    if types.iter().any(|t| t == SECRET_TYPE) {
        return None;
    }
    let has = |name: &str| types.iter().any(|t| t == name);
    TEXT_TYPES
        .into_iter()
        .find(|t| has(t))
        .map(|t| (t, Kind::Text))
        .or_else(|| IMAGE_TYPES.into_iter().find(|(t, _)| has(t)))
}

/// Ask the clipboard's owner for the data and file it. Returns the item's file
/// name, or `None` for what is not kept.
fn keep(conn: &Connection, offer: &ZwlrDataControlOfferV1, types: &[String]) -> io::Result<Option<String>> {
    let Some((mime, kind)) = choose(types) else { return Ok(None) };
    let (reader, writer) = io::pipe()?;
    offer.receive(mime.to_string(), writer.as_fd());
    conn.flush().map_err(io::Error::other)?;
    drop(writer);

    // Read on a thread, so an owner that never finishes cannot stop the history
    let limit = kind.max_size();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut data = Vec::new();
        let read = reader.take(limit as u64 + 1).read_to_end(&mut data);
        let _ = tx.send(read.map(|_| data));
    });
    let data = rx.recv_timeout(READ_TIMEOUT).map_err(io::Error::other)??;
    if data.len() > limit || data.is_empty() {
        return Ok(None);
    }
    if kind == Kind::Text {
        let text = String::from_utf8_lossy(&data);
        if text.trim().is_empty() {
            return Ok(None);
        }
        return store::add(kind, text.as_bytes()).map(Some);
    }
    store::add(kind, &data).map(Some)
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        event: device::Event,
        _: &(),
        conn: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            device::Event::DataOffer { id } => {
                state.offers.insert(id.id(), Vec::new());
            }
            device::Event::Selection { id: Some(offer) } => {
                let types = state.offers.remove(&offer.id()).unwrap_or_default();
                let kept = keep(conn, &offer, &types).unwrap_or_else(|e| {
                    eprintln!("toros-clipd: {e}");
                    None
                });
                store::set_current(kept.as_deref());
                offer.destroy();
            }
            device::Event::Selection { id: None } => store::set_current(None),
            // the text selected with the mouse is not part of the history
            device::Event::PrimarySelection { id: Some(offer) } => {
                state.offers.remove(&offer.id());
                offer.destroy();
            }
            device::Event::Finished => process::exit(0),
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        device::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let offer::Event::Offer { mime_type } = event {
            state.offers.entry(offer.id()).or_default().push(mime_type);
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ZwlrDataControlManagerV1);

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let manager: ZwlrDataControlManagerV1 = globals.bind(&qh, 1..=2, ())?;
    let seat: WlSeat = globals.bind(&qh, 1..=1, ())?;
    manager.get_data_device(&seat, &qh, ());
    let mut state = State::default();
    loop {
        queue.blocking_dispatch(&mut state)?;
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("toros-clipd: {e}");
        process::exit(1);
    }
}
