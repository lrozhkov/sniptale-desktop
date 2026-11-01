//! Closed safe command-label grammar shared by browser events and retained native telemetry.

pub(crate) fn valid_event_key_label(label: &str) -> bool {
    if label.is_empty() || label.len() > 64 {
        return false;
    }
    let mut parts: Vec<_> = label.split('+').collect();
    if parts.len() > 5 {
        return false;
    }
    let Some(last) = parts.last().copied() else {
        return false;
    };
    let modifier_only = modifier(last).is_some();
    if !modifier_only {
        parts.pop();
    }
    let mut seen = 0_u8;
    for part in &parts {
        let Some(flag) = modifier(part) else {
            return false;
        };
        if seen & flag != 0 {
            return false;
        }
        seen |= flag;
    }
    if modifier_only {
        return true;
    }
    let named = matches!(
        last,
        "ArrowUp"
            | "ArrowDown"
            | "ArrowLeft"
            | "ArrowRight"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "Insert"
            | "Delete"
            | "Backspace"
            | "Enter"
            | "Escape"
            | "Tab"
            | "Space"
            | "Unknown"
    );
    let function = last
        .strip_prefix('F')
        .and_then(|text| text.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number) && last == format!("F{number}"));
    named
        || function
        || seen & (1 | 2 | 8) != 0
            && last.len() == 1
            && last
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn modifier(value: &str) -> Option<u8> {
    match value {
        "Ctrl" => Some(1),
        "Alt" => Some(2),
        "Shift" => Some(4),
        "Meta" => Some(8),
        _ => None,
    }
}
