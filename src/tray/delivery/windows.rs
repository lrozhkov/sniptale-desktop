//! Application-owned destination choice. Dismissal retains captured media.

use super::{DeliveryChoice, DeliveryPromptRequest};
use crate::tray::desktop::{prompts::PromptContent, show_prompt};

pub(crate) fn choose_delivery_destination(request: &DeliveryPromptRequest) -> DeliveryChoice {
    if request.targets.len() > 16 {
        return DeliveryChoice::Later;
    }
    match show_prompt(
        PromptContent::Destination {
            local_saved: request.local_saved,
            targets: request
                .targets
                .iter()
                .map(|(_, label)| label.clone())
                .collect(),
        },
        std::time::Duration::from_secs(180),
    ) {
        Some(1) => DeliveryChoice::Local,
        Some(index @ 2..=17) => request
            .targets
            .get(usize::from(index - 2))
            .map_or(DeliveryChoice::Later, |(id, _)| {
                DeliveryChoice::Browser(id.clone())
            }),
        _ => DeliveryChoice::Later,
    }
}
