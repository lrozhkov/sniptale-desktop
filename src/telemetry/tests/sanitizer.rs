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
    use crate::protocol::trajectory::CursorTrajectoryButton;
    assert_eq!(
        CursorTrajectoryButton::from(WindowsMouseButton::Left),
        CursorTrajectoryButton::Left
    );
    assert_eq!(
        CursorTrajectoryButton::from(WindowsMouseButton::Right),
        CursorTrajectoryButton::Right
    );
    assert_eq!(
        CursorTrajectoryButton::from(WindowsMouseButton::Middle),
        CursorTrajectoryButton::Middle
    );
    assert_eq!(
        CursorTrajectoryButton::from(WindowsMouseButton::X1),
        CursorTrajectoryButton::X1
    );
    assert_eq!(
        CursorTrajectoryButton::from(WindowsMouseButton::X2),
        CursorTrajectoryButton::X2
    );
}
#[test]
fn native_key_transitions_preserve_repeats_and_do_not_invent_a_preexisting_onset() {
    use crate::telemetry::windows::{WindowsKeyPhase, WindowsKeyTransition};
    let mut key = WindowsKeyTransition::default();
    assert_eq!(key.observe(false), WindowsKeyPhase::UnclassifiedDown);
    assert_eq!(key.observe(false), WindowsKeyPhase::Repeat);
    assert_eq!(key.observe(true), WindowsKeyPhase::Up);
    assert_eq!(key.observe(false), WindowsKeyPhase::Down);
    for _ in 0..70_000 {
        assert_eq!(key.observe(false), WindowsKeyPhase::Repeat);
    }
    assert_eq!(key.observe(true), WindowsKeyPhase::Up);
    assert_eq!(key.observe(false), WindowsKeyPhase::Down);
}

#[test]
fn native_modifier_facts_require_observed_transitions_and_keep_unknown_states_nullable() {
    use super::super::windows::WindowsKeyTransition as State;
    let mut keys = [State::Unknown; 256];
    let snapshot = WindowsKeyModifiers::from_observed_keys(&keys, false);
    assert_eq!(snapshot.facts(), (None, Some(false), None, None));
    assert_eq!(
        sanitize_windows_key_event(WindowsKeyEvent {
            virtual_key: 0x41,
            modifiers: snapshot
        }),
        SanitizedKeySignal::Typing { count: 1 }
    );
    keys[0xa2].observe(false);
    let snapshot = WindowsKeyModifiers::from_observed_keys(&keys, false);
    assert_eq!(snapshot.facts(), (Some(true), Some(false), None, None));
    assert_eq!(
        sanitize_windows_key_event(WindowsKeyEvent {
            virtual_key: 0x41,
            modifiers: snapshot
        }),
        SanitizedKeySignal::Shortcut("Ctrl+A".to_owned())
    );
    keys.fill(State::Unknown);
    let snapshot = WindowsKeyModifiers::from_observed_keys(&keys, false);
    assert_eq!(
        sanitize_windows_key_event(WindowsKeyEvent {
            virtual_key: 0x41,
            modifiers: snapshot
        }),
        SanitizedKeySignal::Typing { count: 1 }
    );
    keys.fill(State::Released);
    assert_eq!(
        WindowsKeyModifiers::from_observed_keys(&keys, false).facts(),
        (Some(false), Some(false), Some(false), Some(false))
    );
}

#[test]
fn altgr_or_an_unknown_initial_right_alt_never_exposes_printable_key_identity() {
    use super::super::windows::WindowsKeyTransition as State;
    for state in [State::Unknown, State::Pressed] {
        let mut keys = [State::Unknown; 256];
        keys[0xa2] = State::Pressed;
        keys[0xa5] = state;
        let modifiers = WindowsKeyModifiers::from_observed_keys(&keys, true);
        for virtual_key in [0x51, 0x30, 0xba, 0x20] {
            assert_eq!(
                sanitize_windows_key_event(WindowsKeyEvent {
                    virtual_key,
                    modifiers
                }),
                SanitizedKeySignal::Typing { count: 1 }
            );
        }
        assert_eq!(
            sanitize_windows_key_event(WindowsKeyEvent {
                virtual_key: 0x74,
                modifiers
            }),
            SanitizedKeySignal::Shortcut("Ctrl+Alt+F5".to_owned())
        );
    }
}
