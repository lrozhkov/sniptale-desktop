//! Native post-capture destination choices. Closing the prompt retains the result.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::choose_delivery_destination;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryChoice {
    Local,
    Browser(String),
    Later,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeliveryPromptRequest {
    pub(crate) targets: Vec<(String, String)>,
    pub(crate) local_saved: bool,
}
