
cfg_if::cfg_if! {
    if #[cfg(any(test, feature = "simulation"))] {
        use std::string::String;
        use std::format;

    } else {
        use alloc::string::String;
        use alloc::format;
    }
}

use crate::ui_types::{AppUiState, MainWindow};

use slint::{ComponentHandle, SharedString};

fn append_digit(digit: SharedString, existing_value: SharedString) -> SharedString {
    if digit == "." {
        if existing_value.contains(".") {
            return existing_value;
        } else if existing_value.is_empty() {
            return "0.".into();
        }
    } else if existing_value == "0" {
        return digit.into();
    }

    format!("{}{}", existing_value, digit).into()
}

fn backspace(existing_value: SharedString) -> SharedString {
    if existing_value.is_empty() {
        return existing_value;
    }
    let mut chars = existing_value.chars();
    chars.next_back();
    chars.collect::<String>().into()
}

fn append_character(character: SharedString, existing_value: SharedString) -> SharedString {
    format!("{}{}", existing_value, character).into()
}

fn keyboard_backspace(existing_value: SharedString) -> SharedString {
    backspace(existing_value)
}


pub fn register_global_keypad_callbacks(app_window: &MainWindow) {
    let app_config = app_window.global::<AppUiState>();

    app_config.on_keypad_append_digit(|digit, current_value| {
        append_digit(digit, current_value)
    });

    app_config.on_keypad_backspace(|current_value| {
        backspace(current_value)
    });

    app_config.on_keyboard_append_character(|character, current_value| {
        append_character(character, current_value)
    });

    app_config.on_keyboard_backspace(|current_value| {
        keyboard_backspace(current_value)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_digit() {
        // These should all append the digits normally
        assert_eq!(append_digit("1".into(), "".into()), <&str as Into<SharedString>>::into("1"));
        assert_eq!(append_digit("2".into(), "".into()), <&str as Into<SharedString>>::into("2"));
        assert_eq!(append_digit("3".into(), "".into()), <&str as Into<SharedString>>::into("3"));
        assert_eq!(append_digit(".".into(), "1".into()), <&str as Into<SharedString>>::into("1."));
        assert_eq!(append_digit(".".into(), "12".into()), <&str as Into<SharedString>>::into("12."));
        assert_eq!(append_digit(".".into(), "123".into()), <&str as Into<SharedString>>::into("123."));

        // Appending a second decimal point should be ignored
        assert_eq!(append_digit(".".into(), "1234.5".into()), <&str as Into<SharedString>>::into("1234.5"));

        // Appending a leading zero should be allowed only once 
        assert_eq!(append_digit("0".into(), "".into()), <&str as Into<SharedString>>::into("0"));
        assert_eq!(append_digit("0".into(), "0".into()), <&str as Into<SharedString>>::into("0"));

        // Appending a decimal point without a leading zero should add one
        assert_eq!(append_digit(".".into(), "".into()), <&str as Into<SharedString>>::into("0."));

        // Appending a decimal point with a leading zero should be preserve it
        assert_eq!(append_digit(".".into(), "0".into()), <&str as Into<SharedString>>::into("0."));

        // Appending a non-decimal digit should erase any leading zeros
        assert_eq!(append_digit("1".into(), "0".into()), <&str as Into<SharedString>>::into("1"));
        assert_eq!(append_digit("1".into(), "00".into()), <&str as Into<SharedString>>::into("001"));
    }

    #[test]
    fn test_backspace() {
        assert_eq!(backspace("".into()), <&str as Into<SharedString>>::into(""));
        assert_eq!(backspace("1".into()), <&str as Into<SharedString>>::into(""));
        assert_eq!(backspace("12".into()), <&str as Into<SharedString>>::into("1"));
        assert_eq!(backspace("123".into()), <&str as Into<SharedString>>::into("12"));
        assert_eq!(backspace("1234".into()), <&str as Into<SharedString>>::into("123"));
        assert_eq!(backspace("0.".into()), <&str as Into<SharedString>>::into("0"));
    }
}