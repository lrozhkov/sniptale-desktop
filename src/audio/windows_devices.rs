//! Windows WASAPI endpoint discovery adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use windows::{
    Win32::{
        Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE},
        Media::Audio::{
            DEVICE_STATE_ACTIVE, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
            eCapture, eConsole, eRender,
        },
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize, STGM_READ,
            StructuredStorage::{PropVariantClear, PropVariantToString},
        },
    },
    core::{GUID, PWSTR},
};

use super::devices::{AudioEndpoint, AudioEndpointState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsAudioDeviceError {
    ComUnavailable,
    EnumeratorUnavailable,
    EndpointEnumerationFailed,
}

pub(crate) fn audio_endpoints() -> Result<Vec<AudioEndpoint>, WindowsAudioDeviceError> {
    let _apartment = ComApartment::initialize()?;
    let enumerator = create_enumerator()?;
    let default_microphone_id = default_endpoint_id(&enumerator, eCapture);
    let mut endpoints = capture_endpoints(&enumerator, default_microphone_id.as_deref())?;
    if let Some(system) = default_system_loopback(&enumerator)? {
        endpoints.push(system);
    }
    Ok(endpoints)
}

pub(super) fn create_enumerator() -> Result<IMMDeviceEnumerator, WindowsAudioDeviceError> {
    // SAFETY: CoCreateInstance is called after COM initialization; the returned COM interface is
    // owned by the windows crate wrapper and released on drop.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|_| WindowsAudioDeviceError::EnumeratorUnavailable)
}

fn capture_endpoints(
    enumerator: &IMMDeviceEnumerator,
    default_id: Option<&str>,
) -> Result<Vec<AudioEndpoint>, WindowsAudioDeviceError> {
    // SAFETY: `enumerator` is a valid IMMDeviceEnumerator; only active endpoints are requested.
    let collection = unsafe { enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }
        .map_err(|_| WindowsAudioDeviceError::EndpointEnumerationFailed)?;
    // SAFETY: collection is valid and returns a bounded count owned by the COM collection.
    let count = unsafe { collection.GetCount() }
        .map_err(|_| WindowsAudioDeviceError::EndpointEnumerationFailed)?;
    let mut endpoints = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is below `count`; the returned IMMDevice wrapper owns the COM reference.
        let device = unsafe { collection.Item(index) }
            .map_err(|_| WindowsAudioDeviceError::EndpointEnumerationFailed)?;
        let id = device_id(&device)?;
        let is_default = default_id.is_some_and(|default_id| default_id == id);
        endpoints.push(AudioEndpoint::microphone(
            &id,
            &friendly_endpoint_label(&device, index),
            is_default,
            AudioEndpointState::Available,
        ));
    }
    Ok(endpoints)
}

fn default_system_loopback(
    enumerator: &IMMDeviceEnumerator,
) -> Result<Option<AudioEndpoint>, WindowsAudioDeviceError> {
    let Some(id) = default_endpoint_id(enumerator, eRender) else {
        return Ok(None);
    };
    Ok(Some(AudioEndpoint::system_loopback(
        &id,
        "System Audio",
        AudioEndpointState::Available,
    )))
}

fn default_endpoint_id(enumerator: &IMMDeviceEnumerator, flow: EDataFlow) -> Option<String> {
    // SAFETY: `enumerator` is valid; default endpoint absence is represented as `None`.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }.ok()?;
    device_id(&device).ok()
}

fn device_id(device: &IMMDevice) -> Result<String, WindowsAudioDeviceError> {
    // SAFETY: `device` is a valid IMMDevice; GetId returns a CoTaskMem string wrapped below.
    let id = unsafe { device.GetId() }
        .map_err(|_| WindowsAudioDeviceError::EndpointEnumerationFailed)?;
    let id = CoTaskMemString::new(id);
    id.to_string()
        .map_err(|_| WindowsAudioDeviceError::EndpointEnumerationFailed)
}

const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

fn friendly_endpoint_label(device: &IMMDevice, index: u32) -> String {
    let fallback = || format!("Microphone {}", index.saturating_add(1));
    // SAFETY: `device` is a live IMMDevice and the read-only store is returned as an owned COM interface.
    let Ok(store) = (unsafe { device.OpenPropertyStore(STGM_READ) }) else {
        return fallback();
    };
    // SAFETY: the property key is a process-lifetime constant and GetValue initializes the value.
    let Ok(mut value) = (unsafe { store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) }) else {
        return fallback();
    };
    let mut buffer = [0_u16; 256];
    // SAFETY: the buffer is writable for its full advertised length and PROPVARIANT is initialized.
    let converted = unsafe { PropVariantToString(&value, &mut buffer) }.is_ok();
    // SAFETY: clears exactly the PROPVARIANT initialized by IPropertyStore::GetValue.
    let _ = unsafe { PropVariantClear(&mut value) };
    if !converted {
        return fallback();
    }
    let end = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    let label = String::from_utf16_lossy(&buffer[..end])
        .chars()
        .filter(|character| !character.is_control())
        .take(120)
        .collect::<String>();
    let label = label.trim();
    if label.is_empty() {
        fallback()
    } else {
        label.to_owned()
    }
}

#[derive(Debug)]
pub(super) struct ComApartment {
    uninitialize: bool,
}

impl ComApartment {
    pub(super) fn initialize() -> Result<Self, WindowsAudioDeviceError> {
        // SAFETY: initializes COM for this thread; successful initialization is balanced in Drop.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            return Ok(Self { uninitialize: true });
        }
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self {
                uninitialize: false,
            });
        }
        Err(WindowsAudioDeviceError::ComUnavailable)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: balances a successful CoInitializeEx call made by this RAII wrapper.
            unsafe { CoUninitialize() };
        }
    }
}

#[derive(Debug)]
struct CoTaskMemString {
    value: PWSTR,
}

impl CoTaskMemString {
    fn new(value: PWSTR) -> Self {
        Self { value }
    }

    fn to_string(&self) -> Result<String, ()> {
        if self.value.is_null() {
            return Err(());
        }
        // SAFETY: IMMDevice::GetId returns a NUL-terminated CoTaskMem string valid until freed.
        unsafe { self.value.to_string() }.map_err(|_| ())
    }
}

impl Drop for CoTaskMemString {
    fn drop(&mut self) {
        if !self.value.is_null() {
            // SAFETY: the string was allocated by COM and is released exactly once.
            unsafe { CoTaskMemFree(Some(self.value.as_ptr().cast())) };
            self.value = PWSTR::null();
        }
    }
}
