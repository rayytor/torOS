//! What the three windows share: colours, icons, buttons.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, fs};

use gtk::prelude::*;
use gtk::{gdk, gio};

const ICON_SIZE: i32 = 16;
const PALETTE: &str = "/usr/share/toros/palette";

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
        ("window", "bg", "@theme_bg_color"),
        ("field", "headerbar", "alpha(@theme_fg_color, 0.07)"),
        ("fg", "fg", "@theme_fg_color"),
        ("dim", "dim", "alpha(@theme_fg_color, 0.55)"),
        ("hover", "select", "alpha(@theme_fg_color, 0.09)"),
        ("border", "border", "@borders"),
        ("accent", "accent", "@theme_selected_bg_color"),
        ("accent_fg", "accent_fg", "@theme_selected_fg_color"),
        ("accent_text", "accent_text", "@theme_selected_bg_color"),
    ]
    .iter()
    .map(|(name, key, fallback)| {
        format!("@define-color sh_{name} {};\n", from_palette(key).unwrap_or(fallback.to_string()))
    })
    .collect()
}

thread_local! {
    static STYLE: gtk::CssProvider = gtk::CssProvider::new();
    static STYLED: Cell<bool> = const { Cell::new(false) };
}

/// Load the style sheet. Done again at every command, since the desktop may
/// have changed between light and dark while the program was running.
pub fn load_style() {
    STYLE.with(|provider| {
        provider.load_from_string(&(colours() + include_str!("style.css")));
        if let (Some(display), false) = (gdk::Display::default(), STYLED.replace(true)) {
            gtk::style_context_add_provider_for_display(&display, provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
    });
}

/// Where the program keeps what it remembers, and the recorder's log.
pub fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".local/state"))
        .join("toros/shot")
}

/// What was chosen the last time (the shape of a screenshot, recording with
/// or without sound), kept in ~/.local/state/toros/shot.
pub fn remembered(name: &str) -> Option<String> {
    fs::read_to_string(state_dir().join(name)).ok().map(|value| value.trim().to_string())
}

pub fn remember(name: &str, value: &str) {
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir).and_then(|()| fs::write(dir.join(name), value));
}

/// One of the program's own icons. They are symbolic icons: drawn in the text
/// colour of what they are in, whatever state that is in.
pub fn icon(name: &str) -> gtk::Image {
    let file = gio::File::for_uri(&format!("resource:///org/toros/Shot/icons/{name}-symbolic.svg"));
    let scale = gdk::Display::default()
        .and_then(|d| d.monitors().iter::<gdk::Monitor>().flatten().map(|m| m.scale_factor()).max())
        .unwrap_or(1);
    let image = gtk::Image::from_paintable(Some(&gtk::IconPaintable::for_file(&file, ICON_SIZE, scale)));
    image.set_pixel_size(ICON_SIZE);
    image
}

pub fn icon_button(name: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder().child(&icon(name)).tooltip_text(tooltip).can_focus(false).build()
}

pub fn icon_toggle(name: &str, tooltip: &str) -> gtk::ToggleButton {
    gtk::ToggleButton::builder().child(&icon(name)).tooltip_text(tooltip).can_focus(false).build()
}

/// A toggle with an icon and a word beside it.
pub fn word_toggle(name: &str, word: &str, tooltip: &str) -> gtk::ToggleButton {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&icon(name));
    content.append(&gtk::Label::new(Some(word)));
    gtk::ToggleButton::builder().child(&content).tooltip_text(tooltip).can_focus(false).build()
}

/// A thin line between two groups of buttons.
pub fn rule() -> gtk::Box {
    gtk::Box::builder().css_classes(["rule"]).build()
}
