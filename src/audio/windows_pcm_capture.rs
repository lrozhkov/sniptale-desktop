//! Windows WASAPI PCM capture source adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ptr::null_mut, slice};

use windows::{
    Win32::{
        Media::Audio::{
            AUDCLNT_SHAREMODE_SHARED, IAudioCaptureClient, IAudioClient, IMMDevice, eConsole,
            eRender,
        },
        System::Com::CLSCTX_ALL,
    },
    core::PCWSTR,
};

mod format;
mod metrics;

use super::{
    devices::{AudioEndpoint, AudioEndpointKind},
    mixer::{AudioInputKind, PcmFormat},
    pcm_source::{OwnedPcmInputChunk, PcmChunkSource, PcmSourceError, PcmSourcePoll},
    windows_devices::{ComApartment, WindowsAudioDeviceError, create_enumerator},
};

use format::{block_align, packet_duration_100ns, silent_flag, stream_flags, wave_format};
use metrics::CaptureMetrics;

// Shared capture buffers are allowed to be larger than the engine period. A two-second buffer is
// still bounded (under 1 MiB for the owned 48 kHz stereo PCM format) and gives the recording
// worker enough headroom when WGC or the hardware encoder stalls briefly. The former 100 ms
// buffer was routinely overrun by a loaded Windows VM and left otherwise valid MP4 files with
// only their first few AAC packets.
const CAPTURE_BUFFER_DURATION_100NS: i64 = 20_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsWasapiPcmError {
    ComUnavailable,
    DeviceUnavailable,
    UnsupportedEndpointKind,
    InvalidFormat,
    ActivateFailed(i32),
    InitializeFailed(i32),
    CaptureClientUnavailable(i32),
    StartFailed(i32),
    PacketReadFailed(i32),
    BufferTooLarge,
}

#[derive(Debug)]
pub(crate) struct WindowsWasapiPcmSource {
    _apartment: ComApartment,
    audio_client: IAudioClient,
    capture_client: IAudioCaptureClient,
    kind: AudioInputKind,
    format: PcmFormat,
    block_align: usize,
    buffer_frames: u32,
    next_timestamp_100ns: i64,
    metrics: CaptureMetrics,
    started: bool,
}

impl WindowsWasapiPcmSource {
    pub(crate) fn open_microphone(
        endpoint: &AudioEndpoint,
        format: PcmFormat,
    ) -> Result<Self, WindowsWasapiPcmError> {
        if endpoint.kind != AudioEndpointKind::Microphone {
            return Err(WindowsWasapiPcmError::UnsupportedEndpointKind);
        }
        Self::open(endpoint, AudioInputKind::Microphone, format)
    }

    pub(crate) fn open_system_loopback(
        endpoint: &AudioEndpoint,
        format: PcmFormat,
    ) -> Result<Self, WindowsWasapiPcmError> {
        if endpoint.kind != AudioEndpointKind::SystemLoopback {
            return Err(WindowsWasapiPcmError::UnsupportedEndpointKind);
        }
        Self::open(endpoint, AudioInputKind::SystemLoopback, format)
    }

    fn open(
        endpoint: &AudioEndpoint,
        kind: AudioInputKind,
        format: PcmFormat,
    ) -> Result<Self, WindowsWasapiPcmError> {
        let block_align = block_align(format)?;
        let wave_format = wave_format(format)?;
        let apartment = ComApartment::initialize().map_err(com_error)?;
        let enumerator = create_enumerator().map_err(device_error)?;
        let device = endpoint_device(&enumerator, endpoint)?;
        // SAFETY: COM is initialized for this thread; `device` is a valid IMMDevice and the
        // returned IAudioClient COM interface is owned by the windows crate wrapper.
        let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }
            .map_err(|error| WindowsWasapiPcmError::ActivateFailed(hresult(error)))?;
        let stream_flags = stream_flags(kind);
        // SAFETY: `wave_format` is a valid PCM WAVEFORMATEX for the duration of the call.
        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                stream_flags,
                CAPTURE_BUFFER_DURATION_100NS,
                0,
                &wave_format,
                None,
            )
        }
        .map_err(|error| WindowsWasapiPcmError::InitializeFailed(hresult(error)))?;
        // SAFETY: the audio client has been initialized successfully and owns its endpoint buffer.
        let buffer_frames = unsafe { audio_client.GetBufferSize() }
            .map_err(|error| WindowsWasapiPcmError::InitializeFailed(hresult(error)))?;
        // SAFETY: the audio client was initialized successfully; requested service is the capture
        // client owned by the same audio stream.
        let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService() }
            .map_err(|error| WindowsWasapiPcmError::CaptureClientUnavailable(hresult(error)))?;
        // SAFETY: starts the initialized shared-mode capture stream; stopped in Drop.
        unsafe { audio_client.Start() }
            .map_err(|error| WindowsWasapiPcmError::StartFailed(hresult(error)))?;

        Ok(Self {
            _apartment: apartment,
            audio_client,
            capture_client,
            kind,
            format,
            block_align,
            buffer_frames,
            next_timestamp_100ns: 0,
            metrics: CaptureMetrics::default(),
            started: true,
        })
    }
}

impl PcmChunkSource for WindowsWasapiPcmSource {
    fn next_pcm_chunk(&mut self) -> Result<PcmSourcePoll, PcmSourceError> {
        self.read_next_packet()
            .map_err(|_| PcmSourceError::Source(self.kind))
    }
}

impl WindowsWasapiPcmSource {
    fn read_next_packet(&mut self) -> Result<PcmSourcePoll, WindowsWasapiPcmError> {
        // SAFETY: capture client is valid while `self` is alive.
        let packet_frames = unsafe { self.capture_client.GetNextPacketSize() }
            .map_err(|error| WindowsWasapiPcmError::PacketReadFailed(hresult(error)))?;
        if packet_frames == 0 {
            self.metrics.observe_pending();
            return Ok(PcmSourcePoll::Pending);
        }

        let mut data = null_mut();
        let mut frames = 0_u32;
        let mut flags = 0_u32;
        // SAFETY: output pointers are valid for the call; buffer is released by CapturePacketGuard.
        unsafe {
            self.capture_client
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
        }
        .map_err(|error| WindowsWasapiPcmError::PacketReadFailed(hresult(error)))?;
        let guard = CapturePacketGuard::new(&self.capture_client, frames);
        self.metrics.observe_packet(frames, flags);
        let byte_len = usize::try_from(frames)
            .ok()
            .and_then(|frames| frames.checked_mul(self.block_align))
            .ok_or(WindowsWasapiPcmError::BufferTooLarge)?;
        let bytes = if flags & silent_flag() != 0 {
            vec![0; byte_len]
        } else {
            if data.is_null() {
                return Err(WindowsWasapiPcmError::PacketReadFailed(0));
            }
            // SAFETY: WASAPI returns `frames * block_align` bytes valid until ReleaseBuffer.
            unsafe { slice::from_raw_parts(data.cast::<u8>(), byte_len) }.to_vec()
        };
        guard.release()?;
        let duration_100ns = packet_duration_100ns(frames, self.format.sample_rate_hz)?;
        let timestamp_100ns = self.next_timestamp_100ns;
        self.next_timestamp_100ns = self
            .next_timestamp_100ns
            .checked_add(duration_100ns)
            .ok_or(WindowsWasapiPcmError::BufferTooLarge)?;

        Ok(PcmSourcePoll::Chunk(OwnedPcmInputChunk {
            kind: self.kind,
            format: self.format,
            bytes,
            timestamp_100ns,
            duration_100ns,
        }))
    }
}

impl Drop for WindowsWasapiPcmSource {
    fn drop(&mut self) {
        if self.started {
            // SAFETY: best-effort stop for an initialized audio client owned by this source.
            let _ = unsafe { self.audio_client.Stop() };
            self.started = false;
        }
        self.metrics.log(self.kind, self.buffer_frames);
    }
}

#[derive(Debug)]
struct CapturePacketGuard<'a> {
    capture_client: &'a IAudioCaptureClient,
    frames: u32,
    released: bool,
}

impl<'a> CapturePacketGuard<'a> {
    const fn new(capture_client: &'a IAudioCaptureClient, frames: u32) -> Self {
        Self {
            capture_client,
            frames,
            released: false,
        }
    }

    fn release(mut self) -> Result<(), WindowsWasapiPcmError> {
        self.release_inner()
    }

    fn release_inner(&mut self) -> Result<(), WindowsWasapiPcmError> {
        if !self.released {
            // SAFETY: releases the currently held WASAPI capture buffer exactly once.
            unsafe { self.capture_client.ReleaseBuffer(self.frames) }
                .map_err(|error| WindowsWasapiPcmError::PacketReadFailed(hresult(error)))?;
            self.released = true;
        }
        Ok(())
    }
}

impl Drop for CapturePacketGuard<'_> {
    fn drop(&mut self) {
        let _ = self.release_inner();
    }
}

fn endpoint_device(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    endpoint: &AudioEndpoint,
) -> Result<IMMDevice, WindowsWasapiPcmError> {
    match endpoint.kind {
        AudioEndpointKind::Microphone => {
            let endpoint_id = wide_string(&endpoint.stable_id);
            // SAFETY: endpoint id is a NUL-terminated UTF-16 string for the duration of the call.
            unsafe { enumerator.GetDevice(PCWSTR(endpoint_id.as_ptr())) }
                .map_err(|_| WindowsWasapiPcmError::DeviceUnavailable)
        }
        AudioEndpointKind::SystemLoopback => {
            // SAFETY: enumerator is valid; default render endpoint backs loopback capture.
            unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
                .map_err(|_| WindowsWasapiPcmError::DeviceUnavailable)
        }
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn hresult(error: windows::core::Error) -> i32 {
    error.code().0
}

const fn com_error(error: WindowsAudioDeviceError) -> WindowsWasapiPcmError {
    match error {
        WindowsAudioDeviceError::ComUnavailable => WindowsWasapiPcmError::ComUnavailable,
        WindowsAudioDeviceError::EnumeratorUnavailable
        | WindowsAudioDeviceError::EndpointEnumerationFailed => {
            WindowsWasapiPcmError::DeviceUnavailable
        }
    }
}

const fn device_error(_error: WindowsAudioDeviceError) -> WindowsWasapiPcmError {
    WindowsWasapiPcmError::DeviceUnavailable
}
