//! Which applications (and windows) fit what was typed, and how well.

/// Lower case, and the Turkish letters as their plain Latin ones, so that
/// "isik" finds "Işık" and "IŞIK" alike.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            'ı' | 'İ' | 'I' => 'i'.to_lowercase(),
            'ş' | 'Ş' => 's'.to_lowercase(),
            'ğ' | 'Ğ' => 'g'.to_lowercase(),
            'ü' | 'Ü' => 'u'.to_lowercase(),
            'ö' | 'Ö' => 'o'.to_lowercase(),
            'ç' | 'Ç' => 'c'.to_lowercase(),
            c => c.to_lowercase(),
        })
        .collect()
}

/// What an application is found by, already folded.
pub struct Words {
    /// its name
    pub name: String,
    /// everything else: what kind of program it is, its keywords, its command
    pub more: String,
}

impl Words {
    pub fn new(name: &str, more: &[&str]) -> Self {
        Words { name: fold(name), more: fold(&more.join(" ")) }
    }

    /// How well one typed word fits; 0 when it does not.
    fn word(&self, word: &str) -> u32 {
        let starts = |text: &str| text.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(word));
        let initials: String =
            self.name.split(|c: char| !c.is_alphanumeric()).filter_map(|w| w.chars().next()).collect();
        if self.name.starts_with(word) {
            100
        } else if starts(&self.name) {
            80
        } else if word.chars().count() > 1 && initials.starts_with(word) {
            70
        } else if self.name.contains(word) {
            60
        } else if starts(&self.more) {
            40
        } else if self.more.contains(word) {
            20
        } else {
            0
        }
    }

    /// How well the typed text fits: every word of it has to be found, and
    /// the worst of them counts. The whole name typed out is the best there is.
    pub fn score(&self, typed: &str) -> u32 {
        let typed = fold(typed);
        let typed = typed.trim();
        if typed.is_empty() {
            return 1;
        }
        if self.name == typed {
            return 200;
        }
        if self.name.starts_with(typed) {
            return 150;
        }
        typed.split_whitespace().map(|w| self.word(w)).min().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Words {
        Words::new("Text Editor", &["Text Editor", "write notepad", "gnome-text-editor"])
    }

    #[test]
    fn name_beats_keywords() {
        let files = Words::new("Files", &["folder manager explore disk", "nautilus"]);
        let disks = Words::new("Disk Usage", &["files space"]);
        assert!(files.score("fi") > disks.score("fi"));
        assert!(disks.score("disk") > files.score("disk"));
        assert!(files.score("disk") > 0);
    }

    #[test]
    fn words_initials_and_order() {
        let e = editor();
        assert_eq!(e.score("text editor"), 200);
        assert_eq!(e.score("Text"), 150);
        assert_eq!(e.score("edi"), 80);
        assert_eq!(e.score("te"), 150);
        assert_eq!(e.score("editor text"), 80);
        assert_eq!(Words::new("Bluetooth Manager", &[]).score("bm"), 70);
        assert_eq!(e.score("notepad"), 40);
        assert_eq!(e.score("zzz"), 0);
        assert_eq!(e.score("text zzz"), 0);
        assert_eq!(e.score("  "), 1);
    }

    #[test]
    fn turkish_letters() {
        assert_eq!(fold("IŞIK Çöğü"), "isik cogu");
        assert!(Words::new("Ayarlar", &["görüntü"]).score("goruntu") > 0);
        assert!(Words::new("Işık", &[]).score("isik") > 0);
    }
}
