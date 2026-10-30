//! Reserved modeless dialog commands shared with the native adapter.

use super::model::SettingsWindowEvent;

pub(super) const APPLY_ID: usize = 1;
pub(super) const CANCEL_ID: usize = 2;
pub(super) const BROWSE_DESTINATION_ID: usize = 200;

pub(super) const fn button(id: usize) -> Option<SettingsWindowEvent> {
    match id {
        APPLY_ID => Some(SettingsWindowEvent::Apply),
        CANCEL_ID => Some(SettingsWindowEvent::RequestClose),
        BROWSE_DESTINATION_ID => Some(SettingsWindowEvent::BrowseDestination),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_and_escape_follow_reserved_dialog_commands() {
        assert!(matches!(button(1), Some(SettingsWindowEvent::Apply)));
        assert!(matches!(button(2), Some(SettingsWindowEvent::RequestClose)));
        assert!(matches!(
            button(200),
            Some(SettingsWindowEvent::BrowseDestination)
        ));
    }

    #[test]
    fn unrelated_commands_are_not_interpreted_as_dialog_actions() {
        for id in [0, 3, 4, 30, 100, 107, usize::MAX] {
            assert!(button(id).is_none());
        }
    }
}
