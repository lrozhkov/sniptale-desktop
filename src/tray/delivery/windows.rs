//! Native destination dialog; strings and button storage outlive the synchronous call.
#![allow(unsafe_code)]

use super::{DeliveryChoice, DeliveryPromptRequest};
use windows::{
    Win32::UI::Controls::{
        TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TDF_ALLOW_DIALOG_CANCELLATION, TDF_USE_COMMAND_LINKS,
        TaskDialogIndirect,
    },
    core::PCWSTR,
};

pub(crate) fn choose_delivery_destination(request: &DeliveryPromptRequest) -> DeliveryChoice {
    if request.targets.len() > 16 {
        return DeliveryChoice::Later;
    }
    let title = wide("Sniptale");
    let heading = wide("Where do you want to save this capture?");
    let content = wide(if request.local_saved {
        "A local copy is saved in your selected folder. You can also send the capture to a browser."
    } else {
        "Local saving failed. Your capture is retained for recovery. Choose a browser or keep it for later."
    });
    let mut labels = vec![wide("Local folder"), wide("Keep for later")];
    labels.extend(request.targets.iter().map(|(_, label)| wide(label)));
    let buttons = labels
        .iter()
        .enumerate()
        .map(|(index, label)| TASKDIALOG_BUTTON {
            nButtonID: i32::try_from(index).unwrap_or(0) + 100,
            pszButtonText: PCWSTR(label.as_ptr()),
        })
        .collect::<Vec<_>>();
    let configuration = TASKDIALOGCONFIG {
        cbSize: u32::try_from(std::mem::size_of::<TASKDIALOGCONFIG>()).unwrap_or(0),
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_USE_COMMAND_LINKS,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszMainInstruction: PCWSTR(heading.as_ptr()),
        pszContent: PCWSTR(content.as_ptr()),
        cButtons: u32::try_from(buttons.len()).unwrap_or(0),
        pButtons: buttons.as_ptr(),
        nDefaultButton: 101,
        ..TASKDIALOGCONFIG::default()
    };
    let mut selected = 0;
    // SAFETY: configuration, button array, UTF-16 strings and result storage remain live
    // until this synchronous call returns; no callbacks or handles escape this adapter.
    if unsafe {
        TaskDialogIndirect(
            &raw const configuration,
            Some(&raw mut selected),
            None,
            None,
        )
    }
    .is_err()
    {
        return DeliveryChoice::Later;
    }
    match selected {
        100 => DeliveryChoice::Local,
        102..=117 => request
            .targets
            .get(usize::try_from(selected - 102).unwrap_or(usize::MAX))
            .map_or(DeliveryChoice::Later, |(id, _)| {
                DeliveryChoice::Browser(id.clone())
            }),
        _ => DeliveryChoice::Later,
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
