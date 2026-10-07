//! What is on the keys: Apple's rows of letters for the keyboard layouts
//! torOS has, and its two pages of digits and signs.

use std::fs;

/// The layouts with rows of their own; any other gets the English ones.
const LAYOUTS: [&str; 2] = ["us", "tr"];
const XKB_NAMES: &str = "/usr/share/X11/xkb/rules/evdev.lst";

/// The three rows of letters, as they are without Shift.
pub fn letters(layout: &str) -> [&'static str; 3] {
    match layout {
        "tr" => ["qwertyuıopğü", "asdfghjklşi", "zxcvbnmöç"],
        _ => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
    }
}

/// The page behind "123".
pub fn numbers(layout: &str) -> [&'static str; 3] {
    match layout {
        "tr" => ["1234567890", "-/:;()₺&@\"", ".,?!'"],
        _ => ["1234567890", "-/:;()$&@\"", ".,?!'"],
    }
}

/// The page behind "#+=".
pub fn signs(layout: &str) -> [&'static str; 3] {
    match layout {
        "tr" => ["[]{}#%^*+=", "_\\|~<>$€£•", ".,?!'"],
        _ => ["[]{}#%^*+=", "_\\|~<>€£¥•", ".,?!'"],
    }
}

/// A letter as Shift makes it. Turkish has two letters i, each with a
/// capital of its own.
pub fn upper(letter: char, layout: &str) -> char {
    match (letter, layout) {
        ('i', "tr") => 'İ',
        ('ı', _) => 'I',
        _ => letter.to_uppercase().next().unwrap_or(letter),
    }
}

/// Everything the keyboard can type, on any page of any layout.
pub fn all() -> Vec<char> {
    let mut all = vec![' '];
    for layout in LAYOUTS {
        let small = letters(layout).concat();
        let capital: String = small.chars().map(|c| upper(c, layout)).collect();
        for c in [small, capital, numbers(layout).concat(), signs(layout).concat()].concat().chars() {
            if !all.contains(&c) {
                all.push(c);
            }
        }
    }
    all
}

/// The name XKB gives a layout ("us" is "English (US)"). The panel knows the
/// keyboard in use by this name.
pub fn xkb_name(layout: &str) -> String {
    let list = fs::read_to_string(XKB_NAMES).unwrap_or_default();
    list.lines()
        .skip_while(|line| line.trim() != "! layout")
        .skip(1)
        .take_while(|line| !line.starts_with('!'))
        .filter_map(|line| line.trim().split_once(char::is_whitespace))
        .find(|(code, _)| *code == layout)
        .map(|(_, name)| name.trim().to_string())
        .unwrap_or_else(|| layout.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_capitals() {
        assert_eq!(upper('i', "tr"), 'İ');
        assert_eq!(upper('ı', "tr"), 'I');
        assert_eq!(upper('i', "us"), 'I');
        assert_eq!(upper('ş', "tr"), 'Ş');
    }

    #[test]
    fn every_key_can_be_typed() {
        let all = all();
        for layout in LAYOUTS {
            for row in letters(layout).iter().chain(&numbers(layout)).chain(&signs(layout)) {
                for c in row.chars() {
                    assert!(all.contains(&c) && all.contains(&upper(c, layout)), "{c}");
                }
            }
        }
        // one keycode each, and X11's keycodes end at 255
        assert!(all.len() < 240);
    }
}
