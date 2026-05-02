use super::{get_input_key, prefer_physical_for_shortcut};
use winit::keyboard::{Key::Character, SmolStr};

#[test]
fn test_get_input_key() {
    // Tests all visible ASCII characters
    // TODO: it would be nice to test the following:
    // - non-Character keys (ex: named keys, dead keys)
    // - non-ascii characters to ensure shift behavior is appropriate
    for ascii_code in 32u8..127u8 {
        let input = ascii_code as char;
        let key = Character(SmolStr::from(input.to_string()));

        for shift in [false, true] {
            match get_input_key(&key, shift) {
                Character(new_value) => {
                    let new_char = new_value
                        .chars()
                        .next()
                        .expect("string should be non-empty");

                    let expected = match (input, shift) {
                        ('A'..='Z', false) => input
                            .to_lowercase()
                            .next()
                            .expect("string should be non-empty"),
                        // Case 2: a lower case letter when shift is true
                        // Should turn into upper case version
                        ('a'..='z', true) => input
                            .to_uppercase()
                            .next()
                            .expect("string should be non-empty"),
                        // Case 3: a character that should be unchanged by caps lock
                        // - An upper-case letter when shift is true
                        // - A lower-case letter when shift is false,
                        // - A non-alpha character
                        _ => input,
                    };
                    assert_eq!(
                        expected, new_char,
                        "Expected '{input}' -> '{expected}' when shift={shift}, but got '{new_char}'"
                    )
                }
                unexpected => {
                    panic!("Key '{key:?}' somehow became non-character {unexpected:?}")
                }
            }
        }
    }
}

#[test]
fn ascii_logical_key_keeps_layout_translated_key() {
    // US layout: Ctrl+P → logical_key already "p". German QWERTZ: Ctrl+Z user
    // pressed labeled-Z → logical_key "z" (correctly translated). Both should
    // pass through unchanged so existing Latin-layout shortcuts keep working.
    assert_eq!(prefer_physical_for_shortcut("p", Some("p")), None);
    assert_eq!(prefer_physical_for_shortcut("z", Some("y")), None);
    assert_eq!(prefer_physical_for_shortcut(",", Some(",")), None);
    assert_eq!(prefer_physical_for_shortcut("", None), None);
}

#[test]
fn cyrillic_logical_key_falls_back_to_physical() {
    // Cyrillic layout: Ctrl+P at the P-position physical key produces "З" via
    // the Russian layout. Since "З" never matches "ctrl-p" bindings, we fall
    // back to the layout-independent physical "p".
    assert_eq!(
        prefer_physical_for_shortcut("З", Some("p")),
        Some("p".to_owned())
    );
    // Same for Greek Ctrl+Q at the Q-position → "ς" → fall back to "q".
    assert_eq!(
        prefer_physical_for_shortcut("ς", Some("q")),
        Some("q".to_owned())
    );
}

#[test]
fn no_physical_key_means_no_fallback_even_for_non_ascii() {
    // If the platform layer can't determine a layout-independent physical key,
    // we have nothing to fall back to — keep the original (broken) behavior
    // rather than dropping the keystroke entirely.
    assert_eq!(prefer_physical_for_shortcut("З", None), None);
}

#[test]
fn non_ascii_physical_key_is_not_a_useful_fallback() {
    // The fallback only helps when physical_key is ASCII (matches Latin-only
    // binding strings). If it's also non-ASCII (degenerate platform mapping),
    // there's nothing to gain by swapping.
    assert_eq!(prefer_physical_for_shortcut("З", Some("П")), None);
    assert_eq!(prefer_physical_for_shortcut("ς", Some("")), None);
}

#[test]
fn shifted_physical_key_is_passed_through_verbatim() {
    // The caller passes shift-aware physical_key. Ctrl+Shift+P on Cyrillic
    // gives caller `physical = "P"` (uppercase). The helper just trusts what
    // it's given and doesn't change case.
    assert_eq!(
        prefer_physical_for_shortcut("П", Some("P")),
        Some("P".to_owned())
    );
}
