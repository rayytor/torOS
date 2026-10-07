//! The applications: what the menu entries in /usr/share/applications (and
//! the user's own) say, how often each was started from here, and starting one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use toros_overview::search::{fold, Words};

const ENTRY: &str = "Desktop Entry";
/// What runs a program that wants a terminal (GLib's own list has no foot).
const TERMINAL: &str = "foot";

pub struct App {
    pub id: String,
    pub name: String,
    pub info: gio::AppInfo,
    pub words: Words,
    /// what its windows may call themselves (their app-id), folded
    window_ids: Vec<String>,
    terminal: bool,
    /// how often it was started from the overview
    pub used: u32,
}

fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"))
        .join("toros/overview")
}

fn uses() -> HashMap<String, u32> {
    fs::read_to_string(state_dir().join("used"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter_map(|(count, id)| Some((id.to_string(), count.parse().ok()?)))
        .collect()
}

/// The applications a menu shows, by name.
pub fn load() -> Vec<App> {
    let uses = uses();
    let mut apps: Vec<App> = gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .filter_map(|info| {
            let id = info.id()?.to_string();
            let name = info.display_name().to_string();
            // what gio's AppInfo does not hand out
            let file = glib::KeyFile::new();
            let _ = file.load_from_data_dirs(Path::new("applications").join(&id), glib::KeyFileFlags::NONE);
            let text = |key: &str| file.locale_string(ENTRY, key, None).map(|s| s.to_string()).unwrap_or_default();
            let keywords = file
                .locale_string_list(ENTRY, "Keywords", None)
                .map(|list| list.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            let program = info.executable().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let plain_id = id.trim_end_matches(".desktop");
            let window_ids = [plain_id, &text("StartupWMClass"), &program]
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| fold(s))
                .collect();
            Some(App {
                words: Words::new(&name, &[&text("GenericName"), &keywords, &program]),
                window_ids,
                terminal: file.boolean(ENTRY, "Terminal").unwrap_or(false),
                used: uses.get(&id).copied().unwrap_or(0),
                id,
                name,
                info,
            })
        })
        .collect();
    apps.sort_by_cached_key(|app| (fold(&app.name), app.id.clone()));
    apps
}

/// The application a window belongs to, by the window's app-id.
pub fn of_window<'a>(apps: &'a [App], app_id: &str) -> Option<&'a App> {
    let app_id = fold(app_id);
    apps.iter()
        .find(|app| app.window_ids.first() == Some(&app_id))
        .or_else(|| apps.iter().find(|app| app.window_ids.contains(&app_id)))
}

/// Start an application, remember that it was, and call `done` when it is on
/// its way. `renderer` is what the session draws GTK apps with: the overview
/// draws itself differently (see main.rs) and must not hand that on.
///
/// Files, Text Editor and their like are started by a message to the session
/// bus, and the bus drops that message if its sender is gone before the
/// application has come up. So `done` comes only when it has, and the
/// overview must not end before.
pub fn launch(app: &App, renderer: Option<&str>, done: impl FnOnce() + 'static) {
    match renderer {
        Some(renderer) => env::set_var("GSK_RENDERER", renderer),
        None => env::remove_var("GSK_RENDERER"),
    }
    let mut uses = uses();
    *uses.entry(app.id.clone()).or_insert(0) += 1;
    let mut lines: Vec<String> = uses.iter().map(|(id, count)| format!("{count} {id}\n")).collect();
    lines.sort();
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir).and_then(|()| fs::write(dir.join("used"), lines.concat()));

    if app.terminal {
        // the command without the places for files ("%f", "%U")
        let command = app.info.commandline().map(|c| c.to_string_lossy().to_string()).unwrap_or_default();
        let command: Vec<&str> = command.split_whitespace().filter(|word| !word.starts_with('%')).collect();
        let _ = Command::new(TERMINAL).args(["-e", "sh", "-c", &command.join(" ")]).stdin(Stdio::null()).spawn();
        done();
    } else {
        let context = gdk::Display::default().map(|display| display.app_launch_context());
        app.info.launch_uris_async(&[], context.as_ref(), gio::Cancellable::NONE, move |_| done());
    }
}
