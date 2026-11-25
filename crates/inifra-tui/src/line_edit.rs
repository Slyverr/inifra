//! One readline-style keymap shared by every text input.
//!
//! A terminal program receives key events, not the shell's own bindings, so the familiar
//! editing keys live here once. The chat composer and the form fields both call [`apply`], so
//! they cannot drift apart.
//!
//! | Keys | Action |
//! | --- | --- |
//! | ←/→ | move one character |
//! | Ctrl/Alt+←/→, Alt+B/F | move one word |
//! | Home, Ctrl+A / End, Ctrl+E | start / end of the line |
//! | Backspace, Ctrl+H / Delete, Ctrl+D | delete one character |
//! | Ctrl+W | delete the whitespace-separated word before the cursor |
//! | Alt+Backspace, Ctrl+Backspace | delete the word before the cursor |
//! | Alt+D, Ctrl+Delete | delete the word after the cursor |
//! | Ctrl+U / Ctrl+K | delete to the start / end of the line |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What [`apply`] did with a key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Edit {
    /// The key is not an editing key; the caller decides what it means.
    Ignored,
    /// The cursor moved or nothing changed.
    Moved,
    /// The text changed.
    Changed,
}

/// Applies an editing key to `input` at `cursor`, measured in characters.
pub(crate) fn apply(input: &mut Vec<char>, cursor: &mut usize, key: KeyEvent) -> Edit {
    *cursor = (*cursor).min(input.len());
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Left if control || alt => *cursor = word_start(input, *cursor),
        KeyCode::Right if control || alt => *cursor = word_end(input, *cursor),
        KeyCode::Left => *cursor = cursor.saturating_sub(1),
        KeyCode::Right => *cursor = (*cursor + 1).min(input.len()),
        KeyCode::Home => *cursor = line_start(input, *cursor),
        KeyCode::End => *cursor = line_end(input, *cursor),
        KeyCode::Backspace if control || alt => {
            return remove(input, cursor, word_start(input, *cursor), *cursor);
        }
        KeyCode::Backspace => return remove(input, cursor, cursor.saturating_sub(1), *cursor),
        KeyCode::Delete if control || alt => {
            return remove(input, cursor, *cursor, word_end(input, *cursor));
        }
        KeyCode::Delete => return remove(input, cursor, *cursor, (*cursor + 1).min(input.len())),
        KeyCode::Char(character) if control => match character.to_ascii_lowercase() {
            'a' => *cursor = line_start(input, *cursor),
            'e' => *cursor = line_end(input, *cursor),
            'h' => return remove(input, cursor, cursor.saturating_sub(1), *cursor),
            'd' => return remove(input, cursor, *cursor, (*cursor + 1).min(input.len())),
            'w' => return remove(input, cursor, space_word_start(input, *cursor), *cursor),
            'u' => return remove(input, cursor, line_start(input, *cursor), *cursor),
            'k' => return remove(input, cursor, *cursor, line_end(input, *cursor)),
            _ => return Edit::Ignored,
        },
        KeyCode::Char(character) if alt => match character.to_ascii_lowercase() {
            'b' => *cursor = word_start(input, *cursor),
            'f' => *cursor = word_end(input, *cursor),
            'd' => return remove(input, cursor, *cursor, word_end(input, *cursor)),
            _ => return Edit::Ignored,
        },
        KeyCode::Char(character) if !character.is_control() => {
            input.insert(*cursor, character);
            *cursor += 1;
            return Edit::Changed;
        }
        _ => return Edit::Ignored,
    }

    Edit::Moved
}

fn remove(input: &mut Vec<char>, cursor: &mut usize, from: usize, to: usize) -> Edit {
    if from >= to {
        return Edit::Moved;
    }
    input.drain(from..to);
    *cursor = from;
    Edit::Changed
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The start of the word at or before `cursor`: skip separators back, then the word itself.
pub(crate) fn word_start(input: &[char], cursor: usize) -> usize {
    let mut index = cursor;
    while index > 0 && !is_word(input[index - 1]) {
        index -= 1;
    }
    while index > 0 && is_word(input[index - 1]) {
        index -= 1;
    }
    index
}

/// The end of the word at or after `cursor`: skip separators forward, then the word itself.
pub(crate) fn word_end(input: &[char], cursor: usize) -> usize {
    let mut index = cursor;
    while index < input.len() && !is_word(input[index]) {
        index += 1;
    }
    while index < input.len() && is_word(input[index]) {
        index += 1;
    }
    index
}

/// Like the shell's Ctrl+W: whitespace, not punctuation, separates words.
fn space_word_start(input: &[char], cursor: usize) -> usize {
    let mut index = cursor;
    while index > 0 && input[index - 1].is_whitespace() {
        index -= 1;
    }
    while index > 0 && !input[index - 1].is_whitespace() {
        index -= 1;
    }
    index
}

/// The index of the first character on the cursor's line.
pub(crate) fn line_start(input: &[char], cursor: usize) -> usize {
    input[..cursor]
        .iter()
        .rposition(|character| *character == '\n')
        .map_or(0, |index| index + 1)
}

/// The index just past the last character on the cursor's line.
pub(crate) fn line_end(input: &[char], cursor: usize) -> usize {
    cursor
        + input[cursor..]
            .iter()
            .position(|character| *character == '\n')
            .unwrap_or(input.len() - cursor)
}

#[cfg(test)]
mod tests {
    use super::{Edit, apply};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn run(
        text: &str,
        cursor: usize,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> (String, usize, Edit) {
        let mut input = text.chars().collect::<Vec<_>>();
        let mut cursor = cursor;
        let edit = apply(&mut input, &mut cursor, KeyEvent::new(code, modifiers));
        (input.into_iter().collect(), cursor, edit)
    }

    const CTRL: KeyModifiers = KeyModifiers::CONTROL;
    const ALT: KeyModifiers = KeyModifiers::ALT;
    const NONE: KeyModifiers = KeyModifiers::NONE;

    #[test]
    fn ctrl_w_deletes_the_whitespace_separated_word_before_the_cursor() {
        assert_eq!(
            run("open the-file.txt", 17, KeyCode::Char('w'), CTRL),
            ("open ".to_owned(), 5, Edit::Changed)
        );
        assert_eq!(
            run("open  ", 6, KeyCode::Char('w'), CTRL),
            (String::new(), 0, Edit::Changed)
        );
        assert_eq!(
            run("", 0, KeyCode::Char('w'), CTRL),
            (String::new(), 0, Edit::Moved)
        );
    }

    #[test]
    fn word_motion_uses_ctrl_or_alt_arrows_and_alt_b_f() {
        let text = "one two_three  four";
        assert_eq!(run(text, 19, KeyCode::Left, CTRL).1, 15);
        assert_eq!(run(text, 15, KeyCode::Left, CTRL).1, 4);
        assert_eq!(run(text, 4, KeyCode::Left, ALT).1, 0);
        assert_eq!(run(text, 0, KeyCode::Right, CTRL).1, 3);
        assert_eq!(run(text, 3, KeyCode::Right, CTRL).1, 13);
        assert_eq!(run(text, 0, KeyCode::Char('f'), ALT).1, 3);
        assert_eq!(run(text, 19, KeyCode::Char('b'), ALT).1, 15);
    }

    #[test]
    fn word_deletion_has_backward_and_forward_forms() {
        assert_eq!(
            run("one two three", 7, KeyCode::Backspace, ALT),
            ("one  three".to_owned(), 4, Edit::Changed)
        );
        assert_eq!(
            run("one two three", 4, KeyCode::Char('d'), ALT),
            ("one  three".to_owned(), 4, Edit::Changed)
        );
        assert_eq!(
            run("one two three", 4, KeyCode::Delete, CTRL),
            ("one  three".to_owned(), 4, Edit::Changed)
        );
    }

    #[test]
    fn line_keys_stay_on_the_current_line() {
        let text = "first\nsecond";
        assert_eq!(run(text, 9, KeyCode::Char('a'), CTRL).1, 6);
        assert_eq!(run(text, 9, KeyCode::Char('e'), CTRL).1, 12);
        assert_eq!(run(text, 3, KeyCode::End, NONE).1, 5);
        assert_eq!(
            run(text, 9, KeyCode::Char('u'), CTRL),
            ("first\nond".to_owned(), 6, Edit::Changed)
        );
        assert_eq!(
            run(text, 2, KeyCode::Char('k'), CTRL),
            ("fi\nsecond".to_owned(), 2, Edit::Changed)
        );
    }

    #[test]
    fn characters_insert_in_multibyte_text_and_other_keys_are_ignored() {
        assert_eq!(
            run("héllo", 2, KeyCode::Char('é'), NONE),
            ("hééllo".to_owned(), 3, Edit::Changed)
        );
        assert_eq!(run("a", 1, KeyCode::Char('c'), CTRL).2, Edit::Ignored);
        assert_eq!(run("a", 1, KeyCode::Enter, NONE).2, Edit::Ignored);
        assert_eq!(run("a", 1, KeyCode::Tab, NONE).2, Edit::Ignored);
        assert_eq!(run("a", 1, KeyCode::Char('x'), ALT).2, Edit::Ignored);
    }
}
