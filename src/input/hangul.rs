//! Korean 2-set (dubeolsik) layout: the US key each Hangul jamo sits on.
//!
//! macOS terminals such as iTerm2 report Ctrl+B typed with the Korean input
//! source as Ctrl+\u{3160} without the Kitty base-layout field, so nothing in
//! the report names the physical key. Dubeolsik is the standard Korean layout,
//! so its jamo map back to their keys without guessing between layouts.

use crossterm::event::KeyCode;

use super::TerminalKey;

/// The unshifted US key for a dubeolsik jamo. Shifted jamo (\u{3143},
/// \u{3149}, ...) are left out: they need Shift, which the key does not say.
pub(super) fn dubeolsik_key(jamo: char) -> Option<char> {
    let key = match jamo {
        'ㅂ' => 'q',
        'ㅈ' => 'w',
        'ㄷ' => 'e',
        'ㄱ' => 'r',
        'ㅅ' => 't',
        'ㅛ' => 'y',
        'ㅕ' => 'u',
        'ㅑ' => 'i',
        'ㅐ' => 'o',
        'ㅔ' => 'p',
        'ㅁ' => 'a',
        'ㄴ' => 's',
        'ㅇ' => 'd',
        'ㄹ' => 'f',
        'ㅎ' => 'g',
        'ㅗ' => 'h',
        'ㅓ' => 'j',
        'ㅏ' => 'k',
        'ㅣ' => 'l',
        'ㅋ' => 'z',
        'ㅌ' => 'x',
        'ㅊ' => 'c',
        'ㅍ' => 'v',
        'ㅠ' => 'b',
        'ㅜ' => 'n',
        'ㅡ' => 'm',
        _ => return None,
    };
    Some(key)
}

/// `key` as typed on the US layout when it is a dubeolsik jamo, keeping its
/// modifiers, so prefix-mode bindings such as `prefix+n` match `ㅜ`.
pub(super) fn dubeolsik_terminal_key(key: &TerminalKey) -> Option<TerminalKey> {
    let KeyCode::Char(jamo) = key.code else {
        return None;
    };
    let latin = dubeolsik_key(jamo)?;
    Some(TerminalKey::new(KeyCode::Char(latin), key.modifiers).with_kind(key.kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn maps_every_unshifted_letter_key_once() {
        let mut keys = "ㅂㅈㄷㄱㅅㅛㅕㅑㅐㅔㅁㄴㅇㄹㅎㅗㅓㅏㅣㅋㅌㅊㅍㅠㅜㅡ"
            .chars()
            .map(|jamo| dubeolsik_key(jamo).expect("dubeolsik jamo"))
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys.into_iter().collect::<String>(),
            "abcdefghijklmnopqrstuvwxyz"
        );
        assert_eq!(dubeolsik_key('ㅃ'), None);
        assert_eq!(dubeolsik_key('b'), None);
    }

    #[test]
    fn terminal_key_keeps_modifiers() {
        let key = TerminalKey::new(KeyCode::Char('ㅜ'), KeyModifiers::empty());
        let latin = dubeolsik_terminal_key(&key).expect("jamo key");
        assert_eq!(
            (latin.code, latin.modifiers),
            (KeyCode::Char('n'), KeyModifiers::empty())
        );
        let arrow = TerminalKey::new(KeyCode::Right, KeyModifiers::empty());
        assert!(dubeolsik_terminal_key(&arrow).is_none());
    }
}
