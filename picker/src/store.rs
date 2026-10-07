//! The clipboard history: one file per copied item in
//! `$XDG_RUNTIME_DIR/toros-clip`, named after the time it was copied
//! (milliseconds) with `.txt`, `.png` or `.jpg`. toros-clipd writes it and
//! toros-picker reads it. The directory is in memory and gone after a restart,
//! so what was copied (passwords too) never reaches the disk. The file
//! `.current` there has the name of the item that is on the clipboard now.

use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, io};

pub const MAX_ITEMS: usize = 50;
pub const MAX_TEXT: usize = 256 * 1024;
pub const MAX_IMAGE: usize = 8 * 1024 * 1024;
/// All items together; they are held in memory.
const MAX_TOTAL: u64 = 24 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Text,
    Png,
    Jpeg,
}

impl Kind {
    fn ext(self) -> &'static str {
        match self {
            Kind::Text => "txt",
            Kind::Png => "png",
            Kind::Jpeg => "jpg",
        }
    }

    /// The type to offer the item as when it is put back on the clipboard
    /// (for text, wl-copy adds the other usual names by itself).
    pub fn mime(self) -> &'static str {
        match self {
            Kind::Text => "text/plain;charset=utf-8",
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
        }
    }

    pub fn max_size(self) -> usize {
        if self == Kind::Text { MAX_TEXT } else { MAX_IMAGE }
    }
}

#[derive(Clone)]
pub struct Item {
    pub path: PathBuf,
    pub kind: Kind,
    pub time_ms: u64,
    pub size: u64,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn dir() -> io::Result<PathBuf> {
    let run = env::var_os("XDG_RUNTIME_DIR").ok_or(io::Error::other("XDG_RUNTIME_DIR is not set"))?;
    Ok(PathBuf::from(run).join("toros-clip"))
}

/// The history, newest first.
pub fn list() -> Vec<Item> {
    let Ok(entries) = dir().and_then(fs::read_dir) else { return Vec::new() };
    let mut items: Vec<Item> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let kind = match path.extension()?.to_str()? {
                "txt" => Kind::Text,
                "png" => Kind::Png,
                "jpg" => Kind::Jpeg,
                _ => return None,
            };
            let time_ms = path.file_stem()?.to_str()?.parse().ok()?;
            let size = e.metadata().ok()?.len();
            Some(Item { path, kind, time_ms, size })
        })
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.time_ms));
    items
}

/// Put a newly copied item at the top and return its file name. An item with
/// the same content moves up instead of appearing twice; the oldest items go
/// when there are too many.
pub fn add(kind: Kind, data: &[u8]) -> io::Result<String> {
    let dir = dir()?;
    fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let mut items = list();
    for old in &items {
        if old.kind == kind && old.size == data.len() as u64 && fs::read(&old.path).is_ok_and(|d| d == data) {
            let _ = fs::remove_file(&old.path);
        }
    }

    // complete before it gets its name, so the picker never reads half an item
    let mut time_ms = now_ms();
    while items.first().is_some_and(|newest| newest.time_ms >= time_ms) {
        time_ms += 1;
    }
    let name = format!("{time_ms:013}.{}", kind.ext());
    let tmp = dir.join(".new");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, dir.join(&name))?;

    items = list();
    let mut total: u64 = items.iter().map(|i| i.size).sum();
    while items.len() > MAX_ITEMS || (total > MAX_TOTAL && items.len() > 1) {
        let Some(oldest) = items.pop() else { break };
        total -= oldest.size;
        let _ = fs::remove_file(&oldest.path);
    }
    Ok(name)
}

/// Note which item is on the clipboard now; `None` when the clipboard is
/// empty or holds something that is not in the history.
pub fn set_current(name: Option<&str>) {
    let _ = dir().and_then(|dir| fs::write(dir.join(".current"), name.unwrap_or_default()));
}

/// The item that is on the clipboard now, if it is one of the history.
pub fn current() -> Option<Item> {
    let name = fs::read_to_string(dir().ok()?.join(".current")).ok()?;
    list().into_iter().find(|i| i.path.file_name().is_some_and(|n| n == name.as_str()))
}

pub fn remove(item: &Item) {
    let _ = fs::remove_file(&item.path);
}

pub fn clear() {
    for item in list() {
        remove(&item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history() {
        let dir = env::temp_dir().join(format!("toros-clip-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        env::set_var("XDG_RUNTIME_DIR", &dir);

        add(Kind::Text, b"one").unwrap();
        add(Kind::Text, b"two").unwrap();
        add(Kind::Png, b"one").unwrap();
        let name = add(Kind::Text, b"one").unwrap(); // copied again: moves up
        let items = list();
        assert!(current().is_none());
        set_current(Some(&name));
        assert_eq!(current().unwrap().path, items[0].path);
        set_current(None);
        assert!(current().is_none());
        let read: Vec<_> = items.iter().map(|i| (i.kind, fs::read(&i.path).unwrap())).collect();
        assert_eq!(read, [(Kind::Text, b"one".to_vec()), (Kind::Png, b"one".to_vec()), (Kind::Text, b"two".to_vec())]);

        for i in 0..MAX_ITEMS + 5 {
            add(Kind::Text, format!("item {i}").as_bytes()).unwrap();
        }
        let items = list();
        assert_eq!(items.len(), MAX_ITEMS);
        assert_eq!(fs::read(&items[0].path).unwrap(), format!("item {}", MAX_ITEMS + 4).as_bytes());

        clear();
        assert!(list().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
