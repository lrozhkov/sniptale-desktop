//! Decode only this application's version-four tray notifications.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayActivation {
    CapturePanel,
    ContextMenu,
}

pub(crate) const fn tray_activation(word: isize) -> Option<TrayActivation> {
    if (word >> 16) & 0xffff != 1 {
        return None;
    }
    match word & 0xffff {
        0x0202 | 0x0203 | 0x0400 | 0x0401 => Some(TrayActivation::CapturePanel),
        0x007b | 0x0205 => Some(TrayActivation::ContextMenu),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_and_keyboard_activation_open_capture_while_context_events_open_the_menu() {
        for event in [0x0202, 0x0203, 0x0400, 0x0401] {
            assert_eq!(
                tray_activation((1 << 16) | event),
                Some(TrayActivation::CapturePanel)
            );
        }
        for event in [0x007b, 0x0205] {
            assert_eq!(
                tray_activation((1 << 16) | event),
                Some(TrayActivation::ContextMenu)
            );
        }
        for event in [0x0201, 0x0200, 0x0206, 0xffff] {
            assert_eq!(tray_activation((1 << 16) | event), None);
        }
        assert_eq!(tray_activation((2 << 16) | 0x0400), None);
        assert_eq!(tray_activation(0x0400), None);
    }
}
