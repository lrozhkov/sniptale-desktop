use super::super::{
    sanitize::{KeyCode, KeyModifiers, SanitizedKeySignal, sanitize_key},
    windows::{
        WindowsKeyEvent, WindowsKeyModifiers, WindowsMouseButton, sanitize_windows_key_event,
    },
};

#[test]
fn typed_characters_become_counts_without_text() {
    let signal = sanitize_key(KeyCode::Character('x'), KeyModifiers::default());

    assert_eq!(signal, SanitizedKeySignal::Typing { count: 1 });
    assert!(!format!("{signal:?}").contains('x'));
}

#[test]
fn shortcuts_keep_only_safe_key_labels() {
    let signal = sanitize_key(
        KeyCode::Character('s'),
        KeyModifiers::CTRL.with(KeyModifiers::SHIFT),
    );

    assert_eq!(
        signal,
        SanitizedKeySignal::Shortcut("Ctrl+Shift+S".to_owned())
    );
}

#[test]
fn windows_key_adapter_counts_printable_typing_without_text() {
    let signal = sanitize_windows_key_event(WindowsKeyEvent {
        virtual_key: 0x53,
        modifiers: WindowsKeyModifiers::default(),
    });

    assert_eq!(signal, SanitizedKeySignal::Typing { count: 1 });
    assert!(!format!("{signal:?}").contains('s'));
    assert!(!format!("{signal:?}").contains('S'));
}

#[test]
fn windows_key_adapter_keeps_safe_shortcut_and_key_labels() {
    assert_eq!(
        sanitize_windows_key_event(WindowsKeyEvent {
            virtual_key: 0x53,
            modifiers: WindowsKeyModifiers::CTRL.with(WindowsKeyModifiers::SHIFT),
        }),
        SanitizedKeySignal::Shortcut("Ctrl+Shift+S".to_owned())
    );
    assert_eq!(
        sanitize_windows_key_event(WindowsKeyEvent {
            virtual_key: 0x74,
            modifiers: WindowsKeyModifiers::default(),
        }),
        SanitizedKeySignal::KeyLabel("F5".to_owned())
    );
    assert_eq!(
        sanitize_windows_key_event(WindowsKeyEvent {
            virtual_key: 0x0D,
            modifiers: WindowsKeyModifiers::default(),
        }),
        SanitizedKeySignal::KeyLabel("Enter".to_owned())
    );
}

#[test]
fn non_text_keys_keep_safe_labels() {
    assert_eq!(
        sanitize_key(KeyCode::Enter, KeyModifiers::default()),
        SanitizedKeySignal::KeyLabel("Enter".to_owned())
    );
    assert_eq!(
        sanitize_key(KeyCode::Function(5), KeyModifiers::default()),
        SanitizedKeySignal::KeyLabel("F5".to_owned())
    );
}

#[test]
fn windows_mouse_button_labels_are_sanitized() {
    assert_eq!(WindowsMouseButton::Left.label(), "left");
    assert_eq!(WindowsMouseButton::Right.label(), "right");
    assert_eq!(WindowsMouseButton::Middle.label(), "middle");
}
