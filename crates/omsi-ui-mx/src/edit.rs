//! Editing a line of text as a text field does: the caret and the selection (in characters,
//! not bytes), moving by characters or by words, Shift to select, deleting a word at once,
//! and the clipboard's copy, cut and paste of the selection. The fields draw themselves;
//! this is what the keys do to the text.

/// A key a text field takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey {
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    SelectAll,
    Copy,
    Cut,
    Paste,
}

/// The caret of a line of text and the other end of its selection (both in characters).
/// Nothing is selected while they are the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineEdit {
    pub caret: usize,
    pub anchor: usize,
}

/// What a key did besides the text: the text the clipboard is to take (copy, cut).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edited {
    pub clipboard: Option<String>,
}

fn byte(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map(|(b, _)| b).unwrap_or(s.len())
}

/// A word's letters and digits; the rest (spaces, punctuation) lies between words.
fn wordy(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Where the word before `pos` starts: back over what lies between the words, then over the
/// word - Ctrl+Left (Option+Left on a Mac).
pub fn word_left(text: &str, pos: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = pos.min(chars.len());
    while i > 0 && !wordy(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && wordy(chars[i - 1]) {
        i -= 1;
    }
    i
}

/// Where the word at or after `pos` ends: over what lies between the words, then to the
/// word's end - Ctrl+Right.
pub fn word_right(text: &str, pos: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = pos.min(chars.len());
    while i < chars.len() && !wordy(chars[i]) {
        i += 1;
    }
    while i < chars.len() && wordy(chars[i]) {
        i += 1;
    }
    i
}

impl LineEdit {
    /// The caret at the end of `text`, nothing selected.
    pub fn at_end(text: &str) -> LineEdit {
        let n = text.chars().count();
        LineEdit { caret: n, anchor: n }
    }

    /// The selection as (start, end), start <= end.
    pub fn range(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    /// The selected text.
    pub fn selected<'a>(&self, text: &'a str) -> &'a str {
        let (a, b) = self.range();
        &text[byte(text, a)..byte(text, b)]
    }

    /// Caret and anchor within `text` (it may have changed underneath).
    pub fn clamp(&mut self, text: &str) {
        let n = text.chars().count();
        self.caret = self.caret.min(n);
        self.anchor = self.anchor.min(n);
    }

    /// The selection taken out of `text`; true when there was one.
    fn delete_selection(&mut self, text: &mut String) -> bool {
        if !self.has_selection() {
            return false;
        }
        let (a, b) = self.range();
        text.replace_range(byte(text, a)..byte(text, b), "");
        self.caret = a;
        self.anchor = a;
        true
    }

    /// Typed text (or pasted): in place of the selection, control characters left out.
    pub fn insert(&mut self, text: &mut String, typed: &str) {
        let t: String = typed.chars().filter(|c| !c.is_control()).collect();
        if t.is_empty() {
            return;
        }
        self.clamp(text);
        self.delete_selection(text);
        text.insert_str(byte(text, self.caret), &t);
        self.caret += t.chars().count();
        self.anchor = self.caret;
    }

    /// A key on the field. `shift` extends the selection with the caret's move; `word` (Ctrl,
    /// or Option on a Mac) moves and deletes by words; `clipboard` is the text a paste puts
    /// in.
    pub fn key(&mut self, text: &mut String, key: EditKey, shift: bool, word: bool, clipboard: Option<&str>) -> Edited {
        self.clamp(text);
        let n = text.chars().count();
        let mut out = Edited::default();
        let go = |e: &mut LineEdit, to: usize| {
            e.caret = to;
            if !shift {
                e.anchor = to;
            }
        };
        match key {
            EditKey::Left => {
                // (a selection let go of without Shift leaves the caret at its start)
                let to = if word {
                    word_left(text, self.caret)
                } else if self.has_selection() && !shift {
                    self.range().0
                } else {
                    self.caret.saturating_sub(1)
                };
                go(self, to);
            }
            EditKey::Right => {
                let to = if word {
                    word_right(text, self.caret)
                } else if self.has_selection() && !shift {
                    self.range().1
                } else {
                    (self.caret + 1).min(n)
                };
                go(self, to);
            }
            EditKey::Home => go(self, 0),
            EditKey::End => go(self, n),
            EditKey::Backspace => {
                if !self.delete_selection(text) && self.caret > 0 {
                    let from = if word { word_left(text, self.caret) } else { self.caret - 1 };
                    text.replace_range(byte(text, from)..byte(text, self.caret), "");
                    self.caret = from;
                    self.anchor = from;
                }
            }
            EditKey::Delete => {
                if !self.delete_selection(text) && self.caret < n {
                    let to = if word { word_right(text, self.caret) } else { self.caret + 1 };
                    text.replace_range(byte(text, self.caret)..byte(text, to), "");
                    self.anchor = self.caret;
                }
            }
            EditKey::SelectAll => {
                self.anchor = 0;
                self.caret = n;
            }
            // (with nothing selected the whole line, as the field always copied)
            EditKey::Copy => {
                out.clipboard = Some(if self.has_selection() { self.selected(text).to_string() } else { text.clone() });
            }
            EditKey::Cut => {
                if self.has_selection() {
                    out.clipboard = Some(self.selected(text).to_string());
                    self.delete_selection(text);
                } else {
                    out.clipboard = Some(std::mem::take(text));
                    self.caret = 0;
                    self.anchor = 0;
                }
            }
            EditKey::Paste => {
                if let Some(c) = clipboard {
                    self.insert(text, c);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, caret: usize, keys: &[(EditKey, bool, bool)]) -> (String, LineEdit, Option<String>) {
        let mut t = text.to_string();
        let mut e = LineEdit { caret, anchor: caret };
        let mut clip = None;
        for &(k, shift, word) in keys {
            let r = e.key(&mut t, k, shift, word, Some("Ünter"));
            clip = r.clipboard.or(clip);
        }
        (t, e, clip)
    }

    #[test]
    fn words_are_jumped_and_deleted_at_once() {
        let s = "Berlin-Spandau  Rathaus";
        assert_eq!(word_left(s, s.chars().count()), 16);
        assert_eq!(word_left(s, 16), 7);
        assert_eq!(word_right(s, 0), 6);
        assert_eq!(word_right(s, 6), 14);
        let (t, e, _) = run(s, 23, &[(EditKey::Backspace, false, true)]);
        assert_eq!((t.as_str(), e.caret), ("Berlin-Spandau  ", 16));
        let (t, _, _) = run(s, 0, &[(EditKey::Delete, false, true)]);
        assert_eq!(t, "-Spandau  Rathaus");
    }

    #[test]
    fn shift_selects_and_the_clipboard_takes_the_selection() {
        let s = "Linie 137";
        // Shift+Ctrl+Left from the end selects "137"
        let (t, e, clip) = run(s, 9, &[(EditKey::Left, true, true), (EditKey::Copy, false, false)]);
        assert_eq!((t.as_str(), e.range(), clip.as_deref()), ("Linie 137", (6, 9), Some("137")));
        let (t, e, clip) = run(s, 9, &[(EditKey::Left, true, false), (EditKey::Left, true, false), (EditKey::Cut, false, false)]);
        assert_eq!((t.as_str(), e.caret, clip.as_deref()), ("Linie 1", 7, Some("37")));
        // a paste takes the selection's place (in characters: no byte of the umlaut split)
        let (t, e, _) = run(s, 0, &[(EditKey::Right, true, true), (EditKey::Paste, false, false)]);
        assert_eq!((t.as_str(), e.caret), ("Ünter 137", 5));
        // typing over a selection, and Left without Shift lands at its start
        let mut t = s.to_string();
        let mut e = LineEdit { caret: 9, anchor: 6 };
        e.insert(&mut t, "M45\n");
        assert_eq!((t.as_str(), e.caret), ("Linie M45", 9));
        let (_, e, _) = run("abc", 3, &[(EditKey::Home, true, false), (EditKey::Left, false, false)]);
        assert_eq!(e, LineEdit { caret: 0, anchor: 0 });
        // with nothing selected copy and cut take the whole line, as before
        assert_eq!(run(s, 3, &[(EditKey::Copy, false, false)]).2.as_deref(), Some("Linie 137"));
        let (t, _, clip) = run(s, 3, &[(EditKey::Cut, false, false)]);
        assert_eq!((t.as_str(), clip.as_deref()), ("", Some("Linie 137")));
        // select all, then a delete empties it
        assert_eq!(run(s, 2, &[(EditKey::SelectAll, false, false), (EditKey::Delete, false, false)]).0, "");
    }
}
