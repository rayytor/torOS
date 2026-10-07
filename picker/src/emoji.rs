//! The emoji table (data/emoji.txt, written by emoji.py) and the search in it.

use std::borrow::Cow;

pub struct Emoji {
    pub text: &'static str,
    /// The same emoji with the light skin tone, or "" if it has none.
    toned: &'static str,
    pub name: &'static str,
    /// Lower-case words to find it by; begins with the name.
    find: &'static str,
    pub group: usize,
}

/// The groups of the table in its order: the icon on the group's button
/// (data/icons) and the button's tooltip.
pub const GROUPS: [(&str, &str); 9] = [
    ("smileys", "Smileys"),
    ("people", "People"),
    ("animals", "Animals and nature"),
    ("food", "Food and drink"),
    ("travel", "Travel and places"),
    ("activities", "Activities"),
    ("objects", "Objects"),
    ("symbols", "Symbols"),
    ("flags", "Flags"),
];

/// Skin tones: none (yellow), then light to dark.
pub const TONES: [&str; 6] = ["", "\u{1F3FB}", "\u{1F3FC}", "\u{1F3FD}", "\u{1F3FE}", "\u{1F3FF}"];

impl Emoji {
    pub fn with_tone(&self, tone: usize) -> Cow<'static, str> {
        if tone == 0 || self.toned.is_empty() {
            Cow::Borrowed(self.text)
        } else {
            Cow::Owned(self.toned.replace(TONES[1], TONES[tone]))
        }
    }
}

pub fn load() -> Vec<Emoji> {
    let mut all = Vec::with_capacity(2000);
    let mut groups = 0;
    for line in include_str!("../data/emoji.txt").lines() {
        if line.starts_with("# ") {
            groups += 1;
            continue;
        }
        let mut field = line.split('\t');
        if let (Some(text), Some(toned), Some(name), Some(find)) = (field.next(), field.next(), field.next(), field.next()) {
            all.push(Emoji { text, toned, name, find, group: groups - 1 });
        }
    }
    all
}

/// The emoji that have every word of the query, as indexes into `all`: first
/// the one with that name, then those whose name begins with the query, those
/// with a word that begins with it, and the rest.
pub fn search(all: &[Emoji], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let Some(first) = words.first() else { return Vec::new() };
    let word_start = format!(" {first}");
    let mut ranks: [Vec<usize>; 4] = Default::default();
    for (i, e) in all.iter().enumerate() {
        if words.iter().all(|w| e.find.contains(w)) {
            let rank = if e.name.eq_ignore_ascii_case(query.trim()) {
                0
            } else if e.find.starts_with(first) {
                1
            } else if e.find.contains(&word_start) {
                2
            } else {
                3
            };
            ranks[rank].push(i);
        }
    }
    ranks.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_and_search() {
        let all = load();
        assert!(all.len() > 1800);
        assert_eq!(all.last().unwrap().group, GROUPS.len() - 1);
        assert!(all.iter().all(|e| !e.text.is_empty() && e.find.starts_with(&e.name.to_lowercase())));

        let first = |query| all[search(&all, query)[0]].text;
        assert_eq!(first("fire"), "🔥");
        assert_eq!(first("Thumbs up"), "👍");
        assert!(search(&all, "kalp").iter().any(|&i| all[i].name == "red heart")); // Turkish
        assert!(search(&all, "no such emoji").is_empty());

        let thumb = &all[search(&all, "thumbs up")[0]];
        assert_eq!(thumb.with_tone(0), "👍");
        assert_eq!(thumb.with_tone(3), "👍🏽");
        assert_eq!(all[0].with_tone(3), "😀");
    }
}
